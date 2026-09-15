import { useEffect, useMemo, useState } from "react";
import { Activity, SlidersHorizontal, Network, Settings, Search, AlertTriangle, ShieldAlert } from "lucide-react";
import { useEngine } from "./lib/engine";
import { ruleActive } from "./lib/api";
import { formatRate } from "./lib/format";
import { WindowControls, Switch, useWindowChrome, useStoredWidth, ResizeHandle } from "./components/ui";
import { ActivityView } from "./components/ActivityView";
import { RulesView } from "./components/RulesView";
import { NetworkView } from "./components/NetworkView";
import { SettingsView } from "./components/SettingsView";

type View = "activity" | "rules" | "network" | "settings";

const TITLES: Record<View, string> = {
  activity: "Actividad",
  rules: "Reglas",
  network: "Red",
  settings: "Ajustes",
};

export default function App() {
  const { config, status, tick, updateConfig, error } = useEngine();
  // "#rules" etc. selects the initial view (handy for screenshots/docs).
  const [view, setView] = useState<View>(() => {
    const h = location.hash.replace("#", "");
    return h in TITLES ? (h as View) : "activity";
  });
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const { focused, maximized } = useWindowChrome();
  const [sidebarWidth, setSidebarWidth] = useStoredWidth("sidebar", 218, 160, 380);

  // Theme: config override, else OS preference.
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const t = config?.theme ?? "system";
      const dark = t === "dark" || (t === "system" && mq.matches);
      document.documentElement.setAttribute("data-theme", dark ? "dark" : "light");
    };
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [config?.theme]);

  const ruleCount = useMemo(() => {
    if (!config) return 0;
    return Object.values(config.apps).filter(ruleActive).length + (ruleActive(config.global) ? 1 : 0) + (ruleActive(config.hotspot) ? 1 : 0);
  }, [config]);

  const goToApp = (key: string) => {
    setSelected(key);
    setView("activity");
  };

  return (
    <div className={`window ${focused ? "" : "blurred"} ${maximized ? "maximized" : ""}`}>
      <aside className="sidebar" style={{ width: sidebarWidth }} data-tauri-drag-region>
        <ResizeHandle side="right" width={sidebarWidth} onResize={(w) => setSidebarWidth(w < 0 ? 218 : w)} />
        <div className="brand" data-tauri-drag-region>
          <img src="/logo.png" alt="" draggable={false} data-tauri-drag-region />
          <span data-tauri-drag-region>Bandwidth Limiter</span>
        </div>
        <nav className="nav">
          <NavItem icon={<Activity />} label="Actividad" active={view === "activity"} onClick={() => setView("activity")} />
          <NavItem icon={<SlidersHorizontal />} label="Reglas" active={view === "rules"} onClick={() => setView("rules")} count={ruleCount || undefined} />
          <NavItem icon={<Network />} label="Red" active={view === "network"} onClick={() => setView("network")} />
          <NavItem icon={<Settings />} label="Ajustes" active={view === "settings"} onClick={() => setView("settings")} />
        </nav>
        <div className="sidebar-footer">
          <div className="master-row">
            <div>
              <div className="label">Limitador</div>
              <div className="sub">{config?.master ? (tick?.limiting ? "Aplicando reglas" : "Sin reglas activas") : "En pausa"}</div>
            </div>
            <Switch on={config?.master ?? false} onChange={(v) => updateConfig((c) => ({ ...c, master: v }))} disabled={!config} />
          </div>
          <div className="status-line">
            <span className={`dot ${status ? (status.driverOk ? "ok" : "bad") : ""}`} />
            {status ? (status.driverOk ? "Driver activo" : "Driver inactivo") : "Conectando…"}
          </div>
          {tick && (
            <div className="status-line num">
              <span style={{ color: "var(--dl)" }}>↓ {formatRate(tick.total.dl, config?.units ?? "bits")}</span>
              <span style={{ color: "var(--ul)" }}>↑ {formatRate(tick.total.ul, config?.units ?? "bits")}</span>
            </div>
          )}
        </div>
      </aside>

      <main className="main">
        <header className="titlebar" data-tauri-drag-region>
          <h1 data-tauri-drag-region>{TITLES[view]}</h1>
          <div className="spacer" data-tauri-drag-region />
          {view === "activity" && (
            <label className="search">
              <Search />
              <input placeholder="Buscar aplicación" value={filter} onChange={(e) => setFilter(e.target.value)} />
            </label>
          )}
          <WindowControls maximized={maximized} />
        </header>

        {(error || (status && !status.driverOk)) && (
          <div style={{ padding: "12px 18px 0" }}>
            <div className={`banner ${status && !status.elevated ? "" : "warn"}`}>
              {status && !status.elevated ? <ShieldAlert /> : <AlertTriangle />}
              <div>
                <b>{status && !status.elevated ? "Se necesitan permisos de administrador. " : "El motor de captura no está activo. "}</b>
                {status?.driverError ?? error ?? "Reinicia la aplicación como administrador para poder monitorizar y limitar el tráfico."}
              </div>
            </div>
          </div>
        )}

        {view === "activity" && <ActivityView filter={filter} selected={selected} onSelect={setSelected} />}
        {view === "rules" && <RulesView onSelect={goToApp} />}
        {view === "network" && <NetworkView onSelect={goToApp} />}
        {view === "settings" && <SettingsView />}
      </main>
    </div>
  );
}

function NavItem({ icon, label, active, onClick, count }: { icon: React.ReactNode; label: string; active: boolean; onClick: () => void; count?: number }) {
  return (
    <button className={`nav-item ${active ? "active" : ""}`} onClick={onClick}>
      {icon}
      {label}
      {count !== undefined && <span className="count">{count}</span>}
    </button>
  );
}
