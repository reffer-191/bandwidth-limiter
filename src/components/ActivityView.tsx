import { useEffect, useMemo, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Ban,
  Globe,
  Wifi,
  Gauge,
  Monitor,
  X,
  FolderOpen,
} from "lucide-react";
import { api, emptyRule, ruleActive, type AppRate, type FlowView, type Rule, type Sample } from "../lib/api";
import { useEngine } from "../lib/engine";
import { formatBytes, formatRate, formatTime, type Units } from "../lib/format";
import { TrafficChart, type Series } from "./TrafficChart";
import { AppIcon, Segmented, ResizeHandle, useStoredWidth } from "./ui";
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
    if (q) list = list.filter((a) => a.name.toLowerCase().includes(q) || a.description.toLowerCase().includes(q) || a.exe.toLowerCase().includes(q));
    if (!showIdle) list = list.filter((a) => a.dl + a.ul > 0 || a.pids.length > 0 || a.isDevice);
    // Static ordering: by name, or by *accumulated* 30-day usage (never by
    // the live rate, which would reshuffle the rows every second).
    const byName = (a: AppRate, b: AppRate) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) || a.key.localeCompare(b.key);
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
  }, [tick, filter, sort, showIdle, pinned]);

  const selectedApp = selected ? appsById.get(keyToId(selected, appsById)) ?? null : null;


  return (
    <div className="content">
      <div className="content-scroll">
        <div className="tiles">
          <Tile icon={<ArrowDown />} label="Descarga" value={tick?.total.dl ?? 0} units={units} cls="dl" sub={`Internet ${formatRate(tick?.internet.dl ?? 0, units)}`} />
          <Tile icon={<ArrowUp />} label="Subida" value={tick?.total.ul ?? 0} units={units} cls="ul" sub={`Internet ${formatRate(tick?.internet.ul ?? 0, units)}`} />
          <Tile2 icon={<Monitor />} label="Red local" rate={tick?.local ?? { dl: 0, ul: 0 }} units={units} />
          <Tile2 icon={<Wifi />} label="Hotspot" rate={tick?.hotspot ?? { dl: 0, ul: 0 }} units={units} sub={tick?.limiting ? `${formatBytes(tick.queuedBytes)} en cola` : undefined} />
        </div>

        <div className="card">
          <div className="card-header">
            <h2>Historial</h2>
            <div className="chart-legend">
              <span><i style={{ background: "var(--dl)" }} /> Descarga</span>
              <span><i style={{ background: "var(--ul)" }} /> Subida</span>
            </div>
            <div className="spacer" />
            <Segmented<Series>
              value={series}
              onChange={setSeries}
              options={[
                { value: "total", label: "Todo" },
                { value: "internet", label: "Internet" },
                { value: "local", label: "Local" },
                { value: "hotspot", label: "Hotspot" },
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
                Mostrando el instante <b>{formatTime(pinned.ts)}</b> · ↓ {formatRate(pinned.total.dl, units)} · ↑ {formatRate(pinned.total.ul, units)}
              </span>
              <button className="btn" onClick={() => setPinned(null)}>Volver al directo</button>
            </div>
          )}
          <div className="card-header" style={{ paddingBottom: 6 }}>
            <h2>Aplicaciones</h2>
            <span className="faint">{apps.length}</span>
            <div className="spacer" />
            <span className="faint" style={{ fontSize: 11.5 }} title={pinned ? "Descarga/Subida ordenan por la velocidad en el instante fijado" : "Descarga/Subida ordenan por el consumo acumulado de 30 días"}>Ordenar</span>
            <Segmented<SortKey>
              value={sort.key}
              onChange={toggleSort}
              allowReselect
              options={[
                { value: "name", label: `Nombre${arrow("name")}` },
                { value: "dl", label: `Descarga${arrow("dl")}` },
                { value: "ul", label: `Subida${arrow("ul")}` },
              ]}
            />
            <Segmented<string>
              value={showIdle ? "all" : "active"}
              onChange={(v) => setShowIdle(v === "all")}
              options={[
                { value: "all", label: "Todas" },
                { value: "active", label: "Activas" },
              ]}
            />
          </div>
          <div className="table-scroll">
          <table className="table">
            <thead>
              <tr>
                <th className="sortable" onClick={() => toggleSort("name")}>Nombre{arrow("name")}</th>
                <th className="num sortable" onClick={() => toggleSort("dl")} title={pinned ? "Ordenar por descarga en el instante fijado" : "Ordenar por descarga acumulada (30 días)"}>Descarga{arrow("dl")}</th>
                <th className="num sortable" onClick={() => toggleSort("ul")} title={pinned ? "Ordenar por subida en el instante fijado" : "Ordenar por subida acumulada (30 días)"}>Subida{arrow("ul")}</th>
                <th className="num" title="Datos acumulados en los últimos 30 días">30 días</th>
                <th className="num">Reglas</th>
              </tr>
            </thead>
            <tbody>
              {apps.map((a) => {
                const rule = config?.apps[a.key];
                return (
                  <tr key={a.key} className={selected === a.key ? "selected" : ""} onClick={() => onSelect(selected === a.key ? null : a.key)}>
                    <td>
                      <div className="app-cell">
                        <span className={`online ${a.pids.length || a.isDevice ? "" : "off"}`} />
                        <AppIcon appKey={a.key} hasExe={!!a.exe} isDevice={a.isDevice} />
                        <div className="app-name">
                          <span className="n">{a.name}</span>
                          <span className="d">{a.description || (a.exe ? a.exe : "")}</span>
                        </div>
                      </div>
                    </td>
                    <td className={`num rate dl ${a.dl === 0 ? "zero" : ""}`}>{formatRate(a.dl, units)}</td>
                    <td className={`num rate ul ${a.ul === 0 ? "zero" : ""}`}>{formatRate(a.ul, units)}</td>
                    <td className="num faint" title={`↓ ${formatBytes(a.totalDl)} · ↑ ${formatBytes(a.totalUl)}`}>{formatBytes(a.totalDl + a.totalUl)}</td>
                    <td className="num">
                      <RuleBadges rule={rule} units={units} />
                    </td>
                  </tr>
                );
              })}
              {apps.length === 0 && (
                <tr>
                  <td colSpan={5} className="empty">{pinned ? "Sin tráfico en ese instante" : "Sin actividad todavía"}</td>
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

function formatAgo(ts: number): string {
  const s = Math.max(0, (Date.now() - ts) / 1000);
  if (s < 5) return "ahora";
  if (s < 60) return `hace ${Math.round(s)} s`;
  if (s < 3600) return `hace ${Math.round(s / 60)} min`;
  if (s < 86400) return `hace ${Math.round(s / 3600)} h`;
  return `hace ${Math.round(s / 86400)} d`;
}

function keyToId(key: string, apps: Map<number, AppRate>): number {
  for (const a of apps.values()) if (a.key === key) return a.id;
  return -1;
}

export function RuleBadges({ rule, units }: { rule: Rule | undefined; units: Units }) {
  if (!rule || !ruleActive(rule)) return <span className="faint">—</span>;
  return (
    <div className="limits">
      {rule.blockDl && <span className="badge block"><Ban /> ↓</span>}
      {rule.blockUl && <span className="badge block"><Ban /> ↑</span>}
      {rule.dl.enabled && !rule.blockDl && <span className="badge dl"><ArrowDown /> {formatRate(rule.dl.rate, units)}</span>}
      {rule.ul.enabled && !rule.blockUl && <span className="badge ul"><ArrowUp /> {formatRate(rule.ul.rate, units)}</span>}
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

function DetailPanel({ app, units, rule, onClose, onRule }: {
  app: AppRate;
  units: Units;
  rule: Rule | undefined;
  onClose: () => void;
  onRule: (r: Rule) => void;
}) {
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
    <aside className="detail fade-in" style={{ width }}>
      <ResizeHandle side="left" width={width} onResize={(w) => setWidth(w < 0 ? 320 : w)} />
      <div className="detail-head">
        <AppIcon appKey={app.key} hasExe={!!app.exe} isDevice={app.isDevice} large />
        <div className="titles">
          <div className="n">{app.name}</div>
          <div className="d">{app.description || app.exe || "—"}</div>
        </div>
        <button className="btn ghost icon" onClick={onClose} title="Cerrar">
          <X />
        </button>
      </div>

      <div className="detail-section">
        <h3><Gauge style={{ width: 11, height: 11, verticalAlign: -1 }} /> Límites</h3>
        <RuleEditor rule={current} units={units} onChange={onRule} />
      </div>

      <div className="detail-section">
        <h3>Actividad</h3>
        <div className="kv"><span className="k">Descarga</span><span className="v" style={{ color: "var(--dl)" }}>{formatRate(app.dl, units)}</span></div>
        <div className="kv"><span className="k">Subida</span><span className="v" style={{ color: "var(--ul)" }}>{formatRate(app.ul, units)}</span></div>
        <div className="kv"><span className="k">Últimos 30 días</span><span className="v">↓ {formatBytes(app.totalDl)} · ↑ {formatBytes(app.totalUl)}</span></div>
        {app.lastSeen > 0 && <div className="kv"><span className="k">Última actividad</span><span className="v">{formatAgo(app.lastSeen)}</span></div>}
        {!app.isDevice && <div className="kv"><span className="k">PID</span><span className="v">{app.pids.length ? app.pids.join(", ") : "sin proceso activo"}</span></div>}
        {app.exe && (
          <div className="kv">
            <span className="k">Ruta</span>
            <span className="v wrap mono" style={{ fontSize: 11 }}>{app.exe}</span>
          </div>
        )}
        {app.exe && (
          <button className="btn" style={{ alignSelf: "flex-start" }} onClick={() => import("@tauri-apps/plugin-opener").then((m) => m.revealItemInDir(app.exe)).catch(() => {})}>
            <FolderOpen style={{ width: 13, height: 13, verticalAlign: -2, marginRight: 5 }} />
            Mostrar en el Explorador
          </button>
        )}
      </div>

      {!app.isDevice && (
        <div className="detail-section">
          <h3><Globe style={{ width: 11, height: 11, verticalAlign: -1 }} /> Conexiones · {flows.length}</h3>
          <div className="flows">
            {flows.slice(0, 60).map((f, i) => (
              <span key={i}>{f.protocol} {f.remote}</span>
            ))}
            {flows.length === 0 && <span className="faint">Sin conexiones activas</span>}
          </div>
        </div>
      )}
    </aside>
  );
}
