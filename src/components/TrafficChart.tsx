import { useEffect, useMemo, useRef, useState } from "react";
import type { AppRate, Rate, Sample } from "../lib/api";
import { axisLabel, formatRate, formatTime, niceMax, type Units } from "../lib/format";
import { useT } from "../lib/i18n";

export type Series = "total" | "internet" | "local" | "hotspot";

interface Props {
  history: Sample[];
  minutes: number;
  units: Units;
  series: Series;
  appsById: Map<number, AppRate>;
  /** Called with the pinned sample (click) or null when unpinned. */
  onPin?: (sample: Sample | null) => void;
  /** Set to null by the parent to clear an existing pin. */
  pinnedTs?: number | null;
}

interface Hover {
  /** Index into the *visible* slots. */
  index: number;
  x: number;
  pinned: boolean;
}

/** Zoomed view: `endTs` anchors the right edge in absolute time (null = live). */
interface View {
  zoom: number;
  endTs: number | null;
}

const PAD = { top: 10, right: 12, bottom: 22, left: 52 };
const MAX_ZOOM = 60;
const TOOLTIP_W = 210;
const TOOLTIP_GAP = 44;

export function TrafficChart({ history, minutes, units, series, appsById, onPin, pinnedTs }: Props) {
  const t = useT();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<Hover | null>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [view, setView] = useState<View>({ zoom: 1, endTs: null });

  // Full window of samples: last N minutes, one slot per second.
  const full = useMemo(() => {
    const n = minutes * 60;
    const end = history.length ? history[history.length - 1].ts : Date.now();
    const start = end - (n - 1) * 1000;
    const slots: (Sample | null)[] = new Array(n).fill(null);
    for (let i = history.length - 1; i >= 0; i--) {
      const s = history[i];
      const idx = Math.round((s.ts - start) / 1000);
      if (idx < 0) break;
      if (idx < n) slots[idx] = s;
    }
    return { slots, start, end, n };
  }, [history, minutes]);

  // Reset zoom when the history range changes.
  useEffect(() => {
    setView({ zoom: 1, endTs: null });
  }, [minutes]);

  // Visible portion of the window (zoom + pan), in whole slots.
  const visible = useMemo(() => {
    const span = Math.max(10, Math.round(full.n / view.zoom));
    let i1 = full.n - 1;
    if (view.zoom > 1 && view.endTs !== null) {
      i1 = Math.round((view.endTs - full.start) / 1000);
      i1 = Math.max(span - 1, Math.min(full.n - 1, i1));
    }
    const i0 = Math.max(0, i1 - span + 1);
    return { slots: full.slots.slice(i0, i1 + 1), start: full.start + i0 * 1000, n: i1 - i0 + 1, i0 };
  }, [full, view]);

  const pick = (s: Sample): Rate => s[series];

  const yMax = useMemo(() => {
    let m = 0;
    for (const s of visible.slots) if (s) m = Math.max(m, pick(s).dl, pick(s).ul);
    return niceMax(m, units);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible, units, series]);

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    // Initial size right away; ResizeObserver only reports once the page paints.
    setSize({ w: el.clientWidth, h: el.clientHeight });
    const ro = new ResizeObserver(([e]) => {
      const { width, height } = e.contentRect;
      setSize({ w: Math.floor(width), h: Math.floor(height) });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || size.w === 0) return;
    const dpr = window_dpr();
    canvas.width = size.w * dpr;
    canvas.height = size.h * dpr;
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    draw(ctx, size.w, size.h, visible.slots, (i) => (full.end - (visible.start + i * 1000)) / 1000, pick, yMax, units, hover, dark(), t("chart.now"));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [size, visible, yMax, units, hover, series, t]);

  const plotW = size.w - PAD.left - PAD.right;
  const xOf = (index: number) => PAD.left + (index / Math.max(1, visible.n - 1)) * plotW;

  const indexAt = (clientX: number) => {
    const rect = canvasRef.current!.getBoundingClientRect();
    const x = clientX - rect.left - PAD.left;
    const idx = Math.round((x / plotW) * (visible.n - 1));
    return Math.max(0, Math.min(visible.n - 1, idx));
  };

  const onMove = (e: React.MouseEvent) => {
    if (hover?.pinned) return;
    const index = indexAt(e.clientX);
    setHover({ index, x: xOf(index), pinned: false });
  };
  const onLeave = () => {
    if (!hover?.pinned) setHover(null);
  };
  const onClick = (e: React.MouseEvent) => {
    if (hover?.pinned) {
      setHover(null);
      onPin?.(null);
      return;
    }
    const index = indexAt(e.clientX);
    setHover({ index, x: xOf(index), pinned: true });
    onPin?.(visible.slots[index] ?? null);
  };

  // Wheel: zoom around the cursor; Shift+wheel (or a horizontal wheel) pans.
  // Registered natively because React's wheel listener is passive.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const handler = (e: WheelEvent) => {
      e.preventDefault();
      setView((v) => {
        const span = Math.max(10, Math.round(full.n / v.zoom));
        const curEnd = v.zoom > 1 && v.endTs !== null ? v.endTs : full.end;
        const pan = e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY);
        if (pan) {
          if (v.zoom <= 1) return v;
          const delta = (e.shiftKey ? e.deltaY : e.deltaX) > 0 ? 1 : -1;
          const next = curEnd + delta * Math.max(1000, Math.round(span * 0.15) * 1000);
          return { zoom: v.zoom, endTs: clampEnd(next, span) };
        }
        const factor = e.deltaY < 0 ? 1.25 : 1 / 1.25;
        const zoom = Math.max(1, Math.min(MAX_ZOOM, v.zoom * factor));
        if (zoom === 1) return { zoom: 1, endTs: null };
        const rect = canvas.getBoundingClientRect();
        const frac = Math.max(0, Math.min(1, (e.clientX - rect.left - PAD.left) / plotW));
        const cursorTs = curEnd - (span - 1) * 1000 * (1 - frac);
        const newSpan = Math.max(10, Math.round(full.n / zoom));
        const newEnd = cursorTs + (newSpan - 1) * 1000 * (1 - frac);
        return { zoom, endTs: clampEnd(newEnd, newSpan) };
      });
    };
    const clampEnd = (end: number, span: number) => {
      const min = full.start + (span - 1) * 1000;
      return Math.round(Math.max(min, Math.min(full.end, end)) / 1000) * 1000;
    };
    canvas.addEventListener("wheel", handler, { passive: false });
    return () => canvas.removeEventListener("wheel", handler);
  }, [full, plotW]);

  // Parent cleared the pin (e.g. "back to live" button).
  useEffect(() => {
    if (pinnedTs === null && hover?.pinned) setHover(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pinnedTs]);

  // The window shifts one slot per second; keep a pinned crosshair glued to
  // its sample instead of drifting with the time axis.
  useEffect(() => {
    if (!hover?.pinned || pinnedTs == null) return;
    const index = Math.round((pinnedTs - visible.start) / 1000);
    if (pinnedTs < full.start) {
      setHover(null);
      onPin?.(null);
      return;
    }
    if (index < 0 || index >= visible.n) {
      // Scrolled out of the zoomed view: keep the pin, hide the crosshair.
      if (hover.index !== -1) setHover({ index: -1, x: -100, pinned: true });
      return;
    }
    if (index !== hover.index) {
      setHover({ index, x: xOf(index), pinned: true });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible]);

  const hovered = hover && hover.index >= 0 ? visible.slots[hover.index] : null;
  // Keep the tooltip well clear of the crosshair so the point stays visible.
  const tooltipLeft = hover ? (hover.x > size.w / 2 ? hover.x - TOOLTIP_GAP - TOOLTIP_W : hover.x + TOOLTIP_GAP) : 0;
  const compact = !!hover && !hover.pinned;
  const zoomed = view.zoom > 1;

  return (
    <div className="chart-wrap" ref={wrapRef}>
      <canvas ref={canvasRef} onMouseMove={onMove} onMouseLeave={onLeave} onClick={onClick} onDoubleClick={() => setView({ zoom: 1, endTs: null })} />
      {zoomed && (
        <div className="chart-zoom">
          <span>×{view.zoom >= 10 ? view.zoom.toFixed(0) : view.zoom.toFixed(1)}</span>
          <button className="btn ghost" onClick={() => setView({ zoom: 1, endTs: null })} title={t("chart.resetHint")}>
            {t("chart.reset")}
          </button>
        </div>
      )}
      {hover && hover.index >= 0 && (
        <div className={`chart-tooltip ${compact ? "compact" : ""}`} style={{ left: tooltipLeft, top: 6 }}>
          <div className="t">
            <span>{formatTime(visible.start + hover.index * 1000)}</span>
            {hover.pinned && <span>{t("chart.pinned")}</span>}
          </div>
          {hovered ? (
            <>
              <div className="totals">
                <span className="dl">↓ {formatRate(pick(hovered).dl, units)}</span>
                <span className="ul">↑ {formatRate(pick(hovered).ul, units)}</span>
              </div>
              {hovered.top.length > 0 ? (
                <div className="apps">
                  {hovered.top.slice(0, compact ? 3 : 8).map(([id, r]) => {
                    const a = appsById.get(id);
                    return (
                      <div className="app" key={id}>
                        <span className="n">{a ? (a.key === "unknown" ? t("app.unknown") : a.name) : `#${id}`}</span>
                        <span className="dl">{formatRate(r.dl, units)}</span>
                        <span className="ul">{formatRate(r.ul, units)}</span>
                      </div>
                    );
                  })}
                  {compact && hovered.top.length > 3 && <div className="faint">{t("chart.moreWhenPinned", { n: hovered.top.length - 3 })}</div>}
                </div>
              ) : (
                <div className="faint">{t("chart.noTraffic")}</div>
              )}
              {!hover.pinned && <div className="pin">{t("chart.clickToPin")}</div>}
            </>
          ) : (
            <div className="faint">{t("chart.noData")}</div>
          )}
        </div>
      )}
    </div>
  );
}

function window_dpr() {
  return Math.max(1, Math.min(3, globalThis.devicePixelRatio || 1));
}

function dark() {
  return document.documentElement.getAttribute("data-theme") === "dark";
}

function agoLabel(secs: number, now: string): string {
  const s = Math.round(secs);
  if (s <= 0) return now;
  if (s < 60) return `-${s}s`;
  const m = Math.floor(s / 60);
  const r = s % 60;
  return r === 0 ? `-${m}m` : `-${m}m${r}s`;
}

function draw(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  slots: (Sample | null)[],
  secsAgoOf: (i: number) => number,
  pick: (s: Sample) => Rate,
  yMax: number,
  units: Units,
  hover: Hover | null,
  isDark: boolean,
  nowLabel: string,
) {
  ctx.clearRect(0, 0, w, h);
  const plotW = w - PAD.left - PAD.right;
  const plotH = h - PAD.top - PAD.bottom;
  const n = slots.length;
  const x = (i: number) => PAD.left + (i / Math.max(1, n - 1)) * plotW;
  const y = (v: number) => PAD.top + plotH - (Math.min(v, yMax) / yMax) * plotH;

  const grid = isDark ? "rgba(255,255,255,0.07)" : "rgba(0,0,0,0.06)";
  const textCol = isDark ? "#a1a1a6" : "#6e6e73";
  ctx.font = "10.5px -apple-system, 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif";

  // Horizontal grid + y labels.
  const rows = 4;
  ctx.strokeStyle = grid;
  ctx.lineWidth = 1;
  ctx.fillStyle = textCol;
  ctx.textAlign = "right";
  ctx.textBaseline = "middle";
  for (let r = 0; r <= rows; r++) {
    const v = (yMax * r) / rows;
    const yy = Math.round(y(v)) + 0.5;
    ctx.beginPath();
    ctx.moveTo(PAD.left, yy);
    ctx.lineTo(PAD.left + plotW, yy);
    ctx.stroke();
    ctx.fillText(axisLabel(v, units), PAD.left - 8, yy);
  }

  // X labels: ~6 ticks, relative to "now".
  ctx.textAlign = "center";
  ctx.textBaseline = "top";
  const ticks = 6;
  for (let t = 0; t <= ticks; t++) {
    const i = Math.round((t / ticks) * (n - 1));
    ctx.fillText(agoLabel(secsAgoOf(i), nowLabel), x(i), PAD.top + plotH + 6);
  }

  const drawSeries = (key: "dl" | "ul", color: string, fillA: string, fillB: string) => {
    // Area
    const g = ctx.createLinearGradient(0, PAD.top, 0, PAD.top + plotH);
    g.addColorStop(0, fillA);
    g.addColorStop(1, fillB);
    ctx.beginPath();
    let started = false;
    for (let i = 0; i < n; i++) {
      const s = slots[i];
      const v = s ? pick(s)[key] : 0;
      const px = x(i);
      const py = y(v);
      if (!started) {
        ctx.moveTo(px, PAD.top + plotH);
        ctx.lineTo(px, py);
        started = true;
      } else {
        ctx.lineTo(px, py);
      }
    }
    ctx.lineTo(x(n - 1), PAD.top + plotH);
    ctx.closePath();
    ctx.fillStyle = g;
    ctx.fill();
    // Line
    ctx.beginPath();
    for (let i = 0; i < n; i++) {
      const s = slots[i];
      const v = s ? pick(s)[key] : 0;
      const px = x(i);
      const py = y(v);
      if (i === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    }
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.lineJoin = "round";
    ctx.stroke();
    // Individual points once zoomed in enough to tell them apart.
    if (plotW / n > 8) {
      ctx.fillStyle = color;
      for (let i = 0; i < n; i++) {
        const s = slots[i];
        if (!s) continue;
        ctx.beginPath();
        ctx.arc(x(i), y(pick(s)[key]), 2, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  };

  drawSeries("dl", "#0a84ff", "rgba(10,132,255,0.35)", "rgba(10,132,255,0.02)");
  drawSeries("ul", "#ff9f0a", "rgba(255,159,10,0.35)", "rgba(255,159,10,0.02)");

  // Hover crosshair
  if (hover && hover.index >= 0) {
    const hx = Math.round(hover.x) + 0.5;
    ctx.strokeStyle = isDark ? "rgba(255,255,255,0.35)" : "rgba(0,0,0,0.3)";
    ctx.setLineDash([3, 3]);
    ctx.beginPath();
    ctx.moveTo(hx, PAD.top);
    ctx.lineTo(hx, PAD.top + plotH);
    ctx.stroke();
    ctx.setLineDash([]);
    const s = slots[hover.index];
    if (s) {
      for (const [key, color] of [
        ["dl", "#0a84ff"],
        ["ul", "#ff9f0a"],
      ] as const) {
        ctx.beginPath();
        ctx.arc(hover.x, y(pick(s)[key]), 3.5, 0, Math.PI * 2);
        ctx.fillStyle = color;
        ctx.fill();
        ctx.strokeStyle = isDark ? "#1c1c1e" : "#fff";
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
    }
  }
}
