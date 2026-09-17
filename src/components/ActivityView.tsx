import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowDown, ArrowUp, Ban, Globe, Wifi, Gauge, Monitor, X, FolderOpen, Clock, PieChart, ChevronsUp, ChevronsDown } from "lucide-react";
import { api, emptyRule, ruleActive, type AppRate, type FlowView, type Rule, type RuleState, type Sample } from "../lib/api";
import { useEngine } from "../lib/engine";
import { formatBytes, formatRate, formatTime, type Units } from "../lib/format";
import { useT, type T } from "../lib/i18n";
import { TrafficChart, type Series } from "./TrafficChart";
import { AppIcon, Segmented, ResizeHandle, useStoredWidth, appName, appDescription } from "./ui";
import { RuleEditor } from "./RuleEditor";

type SortKey = "name" | "dl" | "ul";
type Dir = 1 | -1;
interface Sort {
  key: SortKey;
  dir: Dir;
}

const SORT_STORAGE = "sort:activity";
const DEFAULT_DIR: Record<SortKey, Dir> = { name: 1, dl: -1, ul: -1 };
function loadSort(): Sort {
  try {
    const v = JSON.parse(localStorage.getItem(SORT_STORAGE) ?? "");
    if (v && (v.key === "name" || v.key === "dl" || v.key === "ul") && (v.dir === 1 || v.dir === -1)) return v;
  } catch {}
  return { key: "name", dir: 1 };
}

export function ActivityView({ filter, selected, onSelect }: {
  filter: string;
  selected: string | null;
  onSelect: (key: string | null) => void;
}) {
  const t = useT();
  const { tick, history, config, appsById, updateConfig } = useEngine();
  const units: Units = config?.units ?? "bits";
  const [minutes, setMinutes] = useState(config?.historyMinutes ?? 10);
  const [series, setSeries] = useState<Series>("total");
  const [sort, setSortState] = useState<Sort>(loadSort);
  /** Same key again flips the direction; a new key starts in its natural one. */
  const toggleSort = (k: SortKey) => {
    setSortState((s) => {
      const next: Sort = s.key === k ? { key: k, dir: (s.dir * -1) as Dir } : { key: k, dir: DEFAULT_DIR[k] };
      try {
        localStorage.setItem(SORT_STORAGE, JSON.stringify(next));
      } catch {}
      return next;
    });
  };
  const arrow = (k: SortKey) => (sort.key === k ? (sort.dir === 1 ? " ↑" : " ↓") : "");
  const [showIdle, setShowIdle] = useState(true);
  /** Instant frozen by clicking the chart; the table then shows that moment. */
  const [pinned, setPinned] = useState<Sample | null>(null);
  const tableRef = useRef<HTMLTableSectionElement>(null);

  useEffect(() => {
    if (config) setMinutes(config.historyMinutes);
  }, [config?.historyMinutes]); // eslint-disable-line react-hooks/exhaustive-deps

  const apps = useMemo(() => {
    let list = tick?.apps ?? [];
    if (pinned) {
      // Replace live rates with the pinned instant's; apps absent from that
      // sample were idle at the time.
      const at = new Map(pinned.top.map(([id, r]) => [id, r]));
      list = list.map((a) => {
        const r = at.get(a.id);
        return { ...a, dl: r?.dl ?? 0, ul: r?.ul ?? 0 };
      });
    }
    const q = filter.trim().toLowerCase();
    if (q) list = list.filter((a) => appName(a, t).toLowerCase().includes(q) || appDescription(a, t).toLowerCase().includes(q) || a.exe.toLowerCase().includes(q));
    if (!showIdle) list = list.filter((a) => a.dl + a.ul > 0 || a.pids.length > 0 || a.isDevice);
    // Static ordering: by name, or by *accumulated* 30-day usage (never by
    // the live rate, which would reshuffle the rows every second).
    const byName = (a: AppRate, b: AppRate) => appName(a, t).localeCompare(appName(b, t), undefined, { sensitivity: "base" }) || a.key.localeCompare(b.key);
    // Live view ranks by accumulated 30-day usage; a pinned snapshot ranks by
    // the rates of that instant (both are stable, unlike live rates).
    const dlOf = (a: AppRate) => (pinned ? a.dl : a.totalDl);
    const ulOf = (a: AppRate) => (pinned ? a.ul : a.totalUl);
    const cmp = (a: AppRate, b: AppRate) => {
      let c: number;
      switch (sort.key) {
        case "dl":
          c = dlOf(a) - dlOf(b);
          break;
        case "ul":
          c = ulOf(a) - ulOf(b);
          break;
        default:
          c = byName(a, b);
      }
      return c * sort.dir || byName(a, b);
    };
    return [...list].sort(cmp);
  }, [tick, filter, sort, showIdle, pinned, t]);

  const selectedApp = selected ? appsById.get(keyToId(selected, appsById)) ?? null : null;

  // Keyboard navigation inside the table: arrows move the selection, Enter
  // toggles the detail panel, Escape closes it.
  const onTableKey = (e: React.KeyboardEvent) => {
    if (!apps.length) return;
    const idx = apps.findIndex((a) => a.key === selected);
    const focusRow = (i: number) => {
      const row = tableRef.current?.children[i] as HTMLElement | undefined;
      row?.focus();
      row?.scrollIntoView({ block: "nearest" });
    };
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = e.key === "ArrowDown" ? Math.min(apps.length - 1, idx + 1) : Math.max(0, idx - 1);
      onSelect(apps[next].key);
      focusRow(next);
    } else if (e.key === "Enter" || e.key === " ") {
      const row = e.target as HTMLElement;
      const key = row.dataset.key;
      if (key) {
        e.preventDefault();
        onSelect(selected === key ? null : key);
      }
    } else if (e.key === "Escape") {
      onSelect(null);
    }
  };

  return (
    <div className="content">
      <div className="content-scroll">
        <div className="tiles">
          <Tile icon={<ArrowDown />} label={t("download")} value={tick?.total.dl ?? 0} units={units} cls="dl" sub={`${t("internet")} ${formatRate(tick?.internet.dl ?? 0, units)}`} />
          <Tile icon={<ArrowUp />} label={t("upload")} value={tick?.total.ul ?? 0} units={units} cls="ul" sub={`${t("internet")} ${formatRate(tick?.internet.ul ?? 0, units)}`} />
          <Tile2 icon={<Monitor />} label={t("local")} rate={tick?.local ?? { dl: 0, ul: 0 }} units={units} />
          <Tile2 icon={<Wifi />} label={t("hotspot")} rate={tick?.hotspot ?? { dl: 0, ul: 0 }} units={units} sub={tick?.limiting ? t("queued", { bytes: formatBytes(tick.queuedBytes) }) : undefined} />
        </div>

        <div className="card">
          <div className="card-header">
            <h2>{t("history")}</h2>
            <div className="chart-legend">
              <span><i style={{ background: "var(--dl)" }} /> {t("download")}</span>
              <span><i style={{ background: "var(--ul)" }} /> {t("upload")}</span>
            </div>
            <div className="spacer" />
            <Segmented<Series>
              value={series}
              onChange={setSeries}
              options={[
                { value: "total", label: t("all") },
                { value: "internet", label: t("internet") },
                { value: "local", label: t("localShort") },
                { value: "hotspot", label: t("hotspot") },
              ]}
            />
            <Segmented<number>
              value={minutes}
              onChange={(m) => {
                setMinutes(m);
                updateConfig((c) => ({ ...c, historyMinutes: m }));
              }}
              options={[1, 5, 10, 15, 30, 60].map((m) => ({ value: m, label: `${m}m` }))}
            />
          </div>
          <div className="card-body" style={{ paddingTop: 8 }}>
            <TrafficChart history={history} minutes={minutes} units={units} series={series} appsById={appsById} onPin={setPinned} pinnedTs={pinned ? pinned.ts : null} />
          </div>
        </div>

        <div className="card apps-card">
          {pinned && (
            <div className="pin-banner">
              <span>
                {t("pinned.showing")} <b>{formatTime(pinned.ts)}</b> · ↓ {formatRate(pinned.total.dl, units)} · ↑ {formatRate(pinned.total.ul, units)}
              </span>
              <button className="btn" onClick={() => setPinned(null)}>{t("pinned.back")}</button>
            </div>
          )}
          <div className="card-header" style={{ paddingBottom: 6 }}>
            <h2>{t("apps")}</h2>
            <span className="faint">{apps.length}</span>
            <span className="faint keys-hint" title={t("keys.hint")}>⌨</span>
            <div className="spacer" />
            <span className="faint" style={{ fontSize: 11.5 }} title={pinned ? t("sort.hintPinned") : t("sort.hintLive")}>{t("sort")}</span>
            <Segmented<SortKey>
              value={sort.key}
              onChange={toggleSort}
              allowReselect
              options={[
                { value: "name", label: `${t("sort.byName")}${arrow("name")}` },
                { value: "dl", label: `${t("download")}${arrow("dl")}` },
                { value: "ul", label: `${t("upload")}${arrow("ul")}` },
              ]}
            />
            <Segmented<string>
              value={showIdle ? "all" : "active"}
              onChange={(v) => setShowIdle(v === "all")}
              options={[
                { value: "all", label: t("filter.all") },
                { value: "active", label: t("filter.active") },
              ]}
            />
          </div>
          <div className="table-scroll">
            <table className="table" onKeyDown={onTableKey}>
              <thead>
                <tr>
                  <th className="sortable" onClick={() => toggleSort("name")}>{t("col.name")}{arrow("name")}</th>
                  <th className="num sortable" onClick={() => toggleSort("dl")} title={pinned ? t("sort.byDl.pinned") : t("sort.byDl.live")}>{t("download")}{arrow("dl")}</th>
                  <th className="num sortable" onClick={() => toggleSort("ul")} title={pinned ? t("sort.byUl.pinned") : t("sort.byUl.live")}>{t("upload")}{arrow("ul")}</th>
                  <th className="num" title={t("col.30d.hint")}>{t("col.30d")}</th>
                  <th className="num">{t("col.rules")}</th>
                </tr>
              </thead>
              <tbody ref={tableRef}>
                {apps.map((a) => {
                  const rule = config?.apps[a.key];
                  return (
                    <tr
                      key={a.key}
                      data-key={a.key}
                      tabIndex={0}
                      aria-selected={selected === a.key}
                      className={selected === a.key ? "selected" : ""}
                      onClick={() => onSelect(selected === a.key ? null : a.key)}
                    >
                      <td>
                        <div className="app-cell">
                          <span className={`online ${a.pids.length || a.isDevice ? "" : "off"}`} />
                          <AppIcon appKey={a.key} hasExe={!!a.exe} isDevice={a.isDevice} />
                          <div className="app-name">
                            <span className="n">{appName(a, t)}</span>
                            <span className="d">{appDescription(a, t) || (a.exe ? a.exe : "")}</span>
                          </div>
                        </div>
                      </td>
                      <td className={`num rate dl ${a.dl === 0 ? "zero" : ""}`}>{formatRate(a.dl, units)}</td>
                      <td className={`num rate ul ${a.ul === 0 ? "zero" : ""}`}>{formatRate(a.ul, units)}</td>
                      <td className="num faint" title={`↓ ${formatBytes(a.totalDl)} · ↑ ${formatBytes(a.totalUl)}`}>{formatBytes(a.totalDl + a.totalUl)}</td>
                      <td className="num">
                        <RuleBadges rule={rule} units={units} state={tick?.states[a.key]} />
                      </td>
                    </tr>
                  );
                })}
                {apps.length === 0 && (
                  <tr>
                    <td colSpan={5} className="empty">{pinned ? t("empty.pinned") : t("empty.live")}</td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      {selectedApp && (
        <DetailPanel
          app={selectedApp}
          units={units}
          rule={config?.apps[selectedApp.key]}
          state={tick?.states[selectedApp.key]}
          history={history}
          minutes={minutes}
          onClose={() => onSelect(null)}
          onRule={(r) =>
            updateConfig((c) => {
              const apps = { ...c.apps };
              if (ruleActive(r)) {
                apps[selectedApp.key] = { ...r, name: selectedApp.name, description: selectedApp.description, exe: selectedApp.exe };
              } else {
                delete apps[selectedApp.key];
              }
              return { ...c, apps };
            })
          }
        />
      )}
    </div>
  );
}

function formatAgo(ts: number, t: T): string {
  const s = Math.max(0, (Date.now() - ts) / 1000);
  if (s < 5) return t("ago.now");
  if (s < 60) return t("ago.s", { n: Math.round(s) });
  if (s < 3600) return t("ago.min", { n: Math.round(s / 60) });
  if (s < 86400) return t("ago.h", { n: Math.round(s / 3600) });
  return t("ago.d", { n: Math.round(s / 86400) });
}

function keyToId(key: string, apps: Map<number, AppRate>): number {
  for (const a of apps.values()) if (a.key === key) return a.id;
  return -1;
}

export function RuleBadges({ rule, units, state }: { rule: Rule | undefined; units: Units; state?: RuleState }) {
  const t = useT();
  if (!rule || !ruleActive(rule)) return <span className="faint">—</span>;
  const off = state ? !state.active : false;
  const over = !!state?.quotaExceeded && rule.quota.action === "block";
  return (
    <div className={`limits ${off ? "off" : ""}`} title={off ? t("badge.off") : undefined}>
      {rule.priority === "high" && <span className="badge green" title={`${t("rule.priority")}: ${t("rule.priority.high")}`}><ChevronsUp /></span>}
      {rule.priority === "low" && <span className="badge" title={`${t("rule.priority")}: ${t("rule.priority.low")}`}><ChevronsDown /></span>}
      {over && <span className="badge block" title={t("rule.quota.exceeded")}><Ban /> {t("rule.quota.exceeded")}</span>}
      {!over && rule.blockDl && <span className="badge block"><Ban /> ↓</span>}
      {!over && rule.blockUl && <span className="badge block"><Ban /> ↑</span>}
      {!over && rule.dl.enabled && !rule.blockDl && <span className="badge dl"><ArrowDown /> {formatRate(rule.dl.rate, units)}</span>}
      {!over && rule.ul.enabled && !rule.blockUl && <span className="badge ul"><ArrowUp /> {formatRate(rule.ul.rate, units)}</span>}
      {rule.schedule.enabled && <span className="badge" title={off ? t("badge.off") : t("badge.schedule")}><Clock /></span>}
      {rule.quota.enabled && !over && <span className="badge" title={t("badge.quota")}><PieChart /></span>}
    </div>
  );
}

function Tile({ icon, label, value, units, cls, sub }: { icon: React.ReactNode; label: string; value: number; units: Units; cls: string; sub?: string }) {
  return (
    <div className="card tile">
      <span className="label">{icon} {label}</span>
      <span className={`value ${cls}`}>{formatRate(value, units)}</span>
      {sub && <span className="sub">{sub}</span>}
    </div>
  );
}

function Tile2({ icon, label, rate, units, sub }: { icon: React.ReactNode; label: string; rate: { dl: number; ul: number }; units: Units; sub?: string }) {
  return (
    <div className="card tile">
      <span className="label">{icon} {label}</span>
      <span className="row2">
        <span className="dl">↓ {formatRate(rate.dl, units)}</span>
        <span className="ul">↑ {formatRate(rate.ul, units)}</span>
      </span>
      <span className="sub">{sub ?? " "}</span>
    </div>
  );
}

/** Tiny download/upload chart of one application over the history window. */
function AppSparkline({ appId, history, minutes, units }: { appId: number; history: Sample[]; minutes: number; units: Units }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const t = useT();
  const series = useMemo(() => {
    const n = minutes * 60;
    const end = history.length ? history[history.length - 1].ts : Date.now();
    const start = end - (n - 1) * 1000;
    const dl = new Float64Array(n);
    const ul = new Float64Array(n);
    for (let i = history.length - 1; i >= 0; i--) {
      const s = history[i];
      const idx = Math.round((s.ts - start) / 1000);
      if (idx < 0) break;
      if (idx >= n) continue;
      const r = s.top.find(([id]) => id === appId)?.[1];
      if (r) {
        dl[idx] = r.dl;
        ul[idx] = r.ul;
      }
    }
    let max = 0;
    for (let i = 0; i < n; i++) max = Math.max(max, dl[i], ul[i]);
    return { dl, ul, max, n };
  }, [history, minutes, appId]);

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
    const { dl, ul, max, n } = series;
    const y = (v: number) => h - 1 - (max > 0 ? (v / max) * (h - 4) : 0);
    const x = (i: number) => (i / Math.max(1, n - 1)) * w;
    for (const [arr, color, fill] of [
      [dl, "#0a84ff", "rgba(10,132,255,0.18)"],
      [ul, "#ff9f0a", "rgba(255,159,10,0.18)"],
    ] as const) {
      ctx.beginPath();
      ctx.moveTo(0, h);
      for (let i = 0; i < n; i++) ctx.lineTo(x(i), y(arr[i]));
      ctx.lineTo(w, h);
      ctx.closePath();
      ctx.fillStyle = fill;
      ctx.fill();
      ctx.beginPath();
      for (let i = 0; i < n; i++) (i ? ctx.lineTo : ctx.moveTo).call(ctx, x(i), y(arr[i]));
      ctx.strokeStyle = color;
      ctx.lineWidth = 1.2;
      ctx.stroke();
    }
  }, [series]);

  return (
    <div className="sparkline-wrap">
      <canvas ref={ref} className="sparkline" />
      <span className="faint sparkline-label">{t("detail.chart", { minutes })} · {t("chart.now")}: {formatRate(series.max, units)} max</span>
    </div>
  );
}

function DetailPanel({ app, units, rule, state, history, minutes, onClose, onRule }: {
  app: AppRate;
  units: Units;
  rule: Rule | undefined;
  state?: RuleState;
  history: Sample[];
  minutes: number;
  onClose: () => void;
  onRule: (r: Rule) => void;
}) {
  const t = useT();
  const [flows, setFlows] = useState<FlowView[]>([]);
  const [width, setWidth] = useStoredWidth("detail", 320, 260, 560);
  useEffect(() => {
    let alive = true;
    const load = () => api.flows(app.key).then((f) => alive && setFlows(f)).catch(() => {});
    load();
    const id = setInterval(load, 2000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [app.key]);

  const current = rule ?? emptyRule();
  return (
    <aside className="detail slide-in" style={{ width }} aria-label={appName(app, t)}>
      <ResizeHandle side="left" width={width} onResize={(w) => setWidth(w < 0 ? 320 : w)} />
      <div className="detail-head">
        <AppIcon appKey={app.key} hasExe={!!app.exe} isDevice={app.isDevice} large />
        <div className="titles">
          <div className="n">{appName(app, t)}</div>
          <div className="d">{appDescription(app, t) || app.exe || "—"}</div>
        </div>
        <button className="btn ghost icon" onClick={onClose} title={`${t("win.close")} (Esc)`} aria-label={t("win.close")}>
          <X />
        </button>
      </div>

      <div className="detail-section">
        <h3><Gauge style={{ width: 11, height: 11, verticalAlign: -1 }} /> {t("detail.limits")}</h3>
        <RuleEditor rule={current} units={units} onChange={onRule} showPriority state={state} />
      </div>

      <div className="detail-section">
        <h3>{t("detail.activity")}</h3>
        <AppSparkline appId={app.id} history={history} minutes={minutes} units={units} />
        <div className="kv"><span className="k">{t("download")}</span><span className="v" style={{ color: "var(--dl)" }}>{formatRate(app.dl, units)}</span></div>
        <div className="kv"><span className="k">{t("upload")}</span><span className="v" style={{ color: "var(--ul)" }}>{formatRate(app.ul, units)}</span></div>
        <div className="kv"><span className="k">{t("detail.last30")}</span><span className="v">↓ {formatBytes(app.totalDl)} · ↑ {formatBytes(app.totalUl)}</span></div>
        {app.lastSeen > 0 && <div className="kv"><span className="k">{t("detail.lastSeen")}</span><span className="v">{formatAgo(app.lastSeen, t)}</span></div>}
        {!app.isDevice && <div className="kv"><span className="k">{t("detail.pid")}</span><span className="v">{app.pids.length ? app.pids.join(", ") : t("detail.noProcess")}</span></div>}
        {app.exe && (
          <div className="kv">
            <span className="k">{t("detail.path")}</span>
            <span className="v wrap mono" style={{ fontSize: 11 }}>{app.exe}</span>
          </div>
        )}
        {app.exe && (
          <button className="btn" style={{ alignSelf: "flex-start" }} onClick={() => import("@tauri-apps/plugin-opener").then((m) => m.revealItemInDir(app.exe)).catch(() => {})}>
            <FolderOpen style={{ width: 13, height: 13, verticalAlign: -2, marginRight: 5 }} />
            {t("detail.reveal")}
          </button>
        )}
      </div>

      {!app.isDevice && (
        <div className="detail-section">
          <h3><Globe style={{ width: 11, height: 11, verticalAlign: -1 }} /> {t("detail.connections")} · {flows.length}</h3>
          <div className="flows">
            {flows.slice(0, 60).map((f, i) => (
              <span key={i}>{f.protocol} {f.remote}</span>
            ))}
            {flows.length === 0 && <span className="faint">{t("detail.noConnections")}</span>}
          </div>
        </div>
      )}
    </aside>
  );
}
