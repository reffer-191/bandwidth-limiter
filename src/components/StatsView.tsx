import { useEffect, useMemo, useRef, useState } from "react";
import { X } from "lucide-react";
import { api, type StatApp, type Stats, type StatsRange } from "../lib/api";
import { useEngine } from "../lib/engine";
import { formatBytes } from "../lib/format";
import { useLang, useT } from "../lib/i18n";
import { AppIcon, Segmented, appDescription, appName } from "./ui";

const RANGE_STORAGE = "stats:range";

export function StatsView({ onSelect }: { onSelect: (key: string) => void }) {
  const t = useT();
  const { config } = useEngine();
  const [range, setRange] = useState<StatsRange>(() => {
    try {
      const v = localStorage.getItem(RANGE_STORAGE);
      if (v === "day" || v === "week" || v === "month") return v;
    } catch {}
    return "day";
  });
  const [app, setApp] = useState<string | null>(null);
  const [stats, setStats] = useState<Stats | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () => api.stats(range, app).then((s) => alive && setStats(s)).catch(() => {});
    load();
    const id = setInterval(load, 15000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [range, app]);

  const pick = (r: StatsRange) => {
    setRange(r);
    try {
      localStorage.setItem(RANGE_STORAGE, r);
    } catch {}
  };
  const selected = app ? stats?.apps.find((a) => a.key === app) ?? null : null;
  const total = stats?.total ?? { t: 0, dl: 0, ul: 0 };
  const grand = total.dl + total.ul;

  return (
    <div className="content">
      <div className="content-scroll">
        <div className="card">
          <div className="card-header">
            <h2>{selected ? appName(selected, t) : t("stats.all")}</h2>
            {selected && (
              <button className="btn ghost icon" onClick={() => setApp(null)} title={t("stats.all")} aria-label={t("stats.all")}><X /></button>
            )}
            <div className="chart-legend">
              <span><i style={{ background: "var(--dl)" }} /> {t("download")}</span>
              <span><i style={{ background: "var(--ul)" }} /> {t("upload")}</span>
            </div>
            <div className="spacer" />
            <Segmented<StatsRange>
              value={range}
              onChange={pick}
              options={[
                { value: "day", label: t("stats.today") },
                { value: "week", label: t("stats.week") },
                { value: "month", label: t("stats.month") },
              ]}
            />
          </div>
          <div className="card-body" style={{ paddingTop: 4 }}>
            <UsageChart stats={stats} range={range} />
          </div>
        </div>

        <div className="tiles three">
          <div className="card tile">
            <span className="label">{t("download")}</span>
            <span className="value dl">{formatBytes(selected ? selected.dl : total.dl)}</span>
            <span className="sub">{range === "day" ? t("stats.today") : range === "week" ? t("stats.week") : t("stats.month")}</span>
          </div>
          <div className="card tile">
            <span className="label">{t("upload")}</span>
            <span className="value ul">{formatBytes(selected ? selected.ul : total.ul)}</span>
            <span className="sub">{range === "day" ? t("stats.today") : range === "week" ? t("stats.week") : t("stats.month")}</span>
          </div>
          <div className="card tile">
            <span className="label">{t("stats.total")}</span>
            <span className="value">{formatBytes(selected ? selected.dl + selected.ul : grand)}</span>
            <span className="sub">{selected && grand > 0 ? `${Math.round(((selected.dl + selected.ul) / grand) * 100)}% · ${t("stats.all").toLowerCase()} ${formatBytes(grand)}` : " "}</span>
          </div>
        </div>

        <div className="card apps-card">
          <div className="card-header" style={{ paddingBottom: 6 }}>
            <h2>{t("stats.byApp")}</h2>
            <span className="faint">{stats?.apps.length ?? 0}</span>
            <div className="spacer" />
            <span className="faint" style={{ fontSize: 11.5 }}>{t("stats.hint")}</span>
          </div>
          <div className="table-scroll">
            <table className="table">
              <thead>
                <tr>
                  <th>{t("col.name")}</th>
                  <th className="num">{t("download")}</th>
                  <th className="num">{t("upload")}</th>
                  <th className="num">{t("stats.total")}</th>
                  <th className="share-col">{t("stats.share")}</th>
                </tr>
              </thead>
              <tbody>
                {(stats?.apps ?? []).map((a) => (
                  <StatRow key={a.key} app={a} grand={grand} selected={app === a.key} onClick={() => setApp(app === a.key ? null : a.key)} onOpen={() => onSelect(a.key)} />
                ))}
                {stats && stats.apps.length === 0 && (
                  <tr>
                    <td colSpan={5} className="empty">{t("stats.empty")}</td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
        {config && <span className="faint" style={{ fontSize: 11, padding: "0 4px" }}>{t("set.usage.desc")}</span>}
      </div>
    </div>
  );
}

function StatRow({ app, grand, selected, onClick, onOpen }: { app: StatApp; grand: number; selected: boolean; onClick: () => void; onOpen: () => void }) {
  const t = useT();
  const total = app.dl + app.ul;
  const pct = grand > 0 ? (total / grand) * 100 : 0;
  return (
    <tr tabIndex={0} className={selected ? "selected" : ""} aria-selected={selected} onClick={onClick} onDoubleClick={onOpen} onKeyDown={(e) => (e.key === "Enter" ? onClick() : undefined)}>
      <td>
        <div className="app-cell">
          <AppIcon appKey={app.key} hasExe={!!app.exe} isDevice={app.isDevice} />
          <div className="app-name">
            <span className="n">{appName(app, t)}</span>
            <span className="d">{appDescription(app, t) || app.exe}</span>
          </div>
        </div>
      </td>
      <td className="num rate dl">{formatBytes(app.dl)}</td>
      <td className="num rate ul">{formatBytes(app.ul)}</td>
      <td className="num">{formatBytes(total)}</td>
      <td className="share-col">
        <div className="share">
          <div className="bar"><div className="fill" style={{ width: `${pct}%` }} /></div>
          <span className="faint">{pct >= 10 ? pct.toFixed(0) : pct.toFixed(1)}%</span>
        </div>
      </td>
    </tr>
  );
}

/** Stacked bars (download + upload) per hour or per day, with a hover readout. */
function UsageChart({ stats, range }: { stats: Stats | null; range: StatsRange }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [hover, setHover] = useState<number | null>(null);
  const t = useT();
  const lang = useLang();
  const buckets = stats?.buckets ?? [];
  const max = useMemo(() => Math.max(1, ...buckets.map((b) => b.dl + b.ul)), [buckets]);

  const label = (i: number) => {
    const b = buckets[i];
    if (!b) return "";
    const d = new Date(b.t);
    if (range === "day") return `${String(d.getHours()).padStart(2, "0")}:00`;
    return d.toLocaleDateString(lang === "es" ? "es-ES" : "en-US", { weekday: "short", day: "numeric" });
  };

  useEffect(() => {
    const c = ref.current;
    if (!c) return;
    const dpr = Math.max(1, window.devicePixelRatio || 1);
    const w = c.clientWidth;
    const h = c.clientHeight;
    c.width = w * dpr;
    c.height = h * dpr;
    const ctx = c.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const css = getComputedStyle(document.documentElement);
    const text = css.getPropertyValue("--text-3").trim() || "#888";
    const grid = css.getPropertyValue("--divider").trim() || "rgba(0,0,0,0.08)";
    const padL = 46;
    const padB = 18;
    const padT = 6;
    const plotW = w - padL - 6;
    const plotH = h - padB - padT;
    const n = buckets.length || 1;
    // Grid + y labels (4 lines).
    ctx.font = "10px system-ui, sans-serif";
    ctx.textBaseline = "middle";
    for (let i = 0; i <= 4; i++) {
      const y = padT + plotH - (plotH * i) / 4;
      ctx.strokeStyle = grid;
      ctx.beginPath();
      ctx.moveTo(padL, y);
      ctx.lineTo(w - 6, y);
      ctx.stroke();
      ctx.fillStyle = text;
      ctx.textAlign = "right";
      ctx.fillText(i === 0 ? "0" : formatBytes((max * i) / 4).replace(/\.\d+/, ""), padL - 6, y);
    }
    const slot = plotW / n;
    const bw = Math.max(2, Math.min(28, slot * 0.62));
    const dl = css.getPropertyValue("--dl").trim() || "#0a84ff";
    const ul = css.getPropertyValue("--ul").trim() || "#ff9f0a";
    buckets.forEach((b, i) => {
      const x = padL + slot * i + (slot - bw) / 2;
      const hDl = (b.dl / max) * plotH;
      const hUl = (b.ul / max) * plotH;
      const dim = hover !== null && hover !== i;
      ctx.globalAlpha = dim ? 0.45 : 1;
      ctx.fillStyle = dl;
      roundRect(ctx, x, padT + plotH - hDl, bw, hDl, 3);
      ctx.fillStyle = ul;
      roundRect(ctx, x, padT + plotH - hDl - hUl, bw, hUl, 3);
      ctx.globalAlpha = 1;
    });
    // X labels: at most ~8.
    ctx.fillStyle = text;
    ctx.textAlign = "center";
    ctx.textBaseline = "alphabetic";
    const step = Math.ceil(n / 8);
    buckets.forEach((_, i) => {
      if (i % step !== 0) return;
      ctx.fillText(label(i), padL + slot * i + slot / 2, h - 4);
    });
  }, [buckets, max, hover, range, lang]);

  const onMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const c = ref.current;
    if (!c || !buckets.length) return;
    const rect = c.getBoundingClientRect();
    const x = e.clientX - rect.left - 46;
    const slot = (rect.width - 52) / buckets.length;
    const i = Math.floor(x / slot);
    setHover(i >= 0 && i < buckets.length ? i : null);
  };
  const hb = hover !== null ? buckets[hover] : null;

  return (
    <div className="usage-chart">
      <canvas ref={ref} onMouseMove={onMove} onMouseLeave={() => setHover(null)} />
      <div className="usage-readout">
        {hb ? (
          <>
            <b>{label(hover!)}</b>
            <span style={{ color: "var(--dl)" }}>↓ {formatBytes(hb.dl)}</span>
            <span style={{ color: "var(--ul)" }}>↑ {formatBytes(hb.ul)}</span>
            <span className="faint">{formatBytes(hb.dl + hb.ul)}</span>
          </>
        ) : (
          <span className="faint">{range === "day" ? t("stats.perHour") : t("stats.perDay")}</span>
        )}
      </div>
    </div>
  );
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  if (h <= 0) return;
  const rr = Math.min(r, h / 2, w / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.lineTo(x + w - rr, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + rr);
  ctx.lineTo(x + w, y + h);
  ctx.lineTo(x, y + h);
  ctx.lineTo(x, y + rr);
  ctx.quadraticCurveTo(x, y, x + rr, y);
  ctx.closePath();
  ctx.fill();
}
