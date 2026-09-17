import { useEffect, useMemo, useRef, useState } from "react";
import { Activity, SlidersHorizontal, Network, Settings, Search, AlertTriangle, ShieldAlert, Download, BarChart3 } from "lucide-react";
import { useEngine } from "./lib/engine";
import { api, onEvent, ruleActive, type UpdateInfo } from "./lib/api";
import { formatRate } from "./lib/format";
import { I18nProvider, resolveLang, useT, type Key } from "./lib/i18n";
import { WindowControls, Switch, useWindowChrome, useStoredWidth, ResizeHandle } from "./components/ui";
import { ActivityView } from "./components/ActivityView";
import { RulesView } from "./components/RulesView";
import { NetworkView } from "./components/NetworkView";
import { SettingsView } from "./components/SettingsView";
import { StatsView } from "./components/StatsView";
import { Onboarding } from "./components/Onboarding";

type View = "activity" | "stats" | "rules" | "network" | "settings";

const VIEWS: View[] = ["activity", "stats", "rules", "network", "settings"];

const TITLE_KEYS: Record<View, Key> = {
  activity: "nav.activity",
  stats: "nav.stats",
  rules: "nav.rules",
  network: "nav.network",
  settings: "nav.settings",
};

export default function App() {
  const { config } = useEngine();
  const lang = resolveLang(config?.language);
  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);
  return (
    <I18nProvider lang={lang}>
      <Shell />
    </I18nProvider>
  );
}

function Shell() {
  const t = useT();
  const { config, status, tick, updateConfig, error } = useEngine();
  // "#rules" etc. selects the initial view (handy for screenshots/docs).
  const [view, setView] = useState<View>(() => {
    const h = location.hash.replace("#", "");
    return h in TITLE_KEYS ? (h as View) : "activity";
  });
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const { focused, maximized } = useWindowChrome();
  const [sidebarWidth, setSidebarWidth] = useStoredWidth("sidebar", 218, 160, 380);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [installing, setInstalling] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const p = onEvent<UpdateInfo>("update-available", setUpdate);
    return () => {
      p.then((f) => f());
    };
  }, []);

  // Theme: config override, else OS preference.
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const th = config?.theme ?? "system";
      const dark = th === "dark" || (th === "system" && mq.matches);
      document.documentElement.setAttribute("data-theme", dark ? "dark" : "light");
    };
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [config?.theme]);

  // Global shortcuts: Ctrl+F focuses the search, Ctrl+1..5 switch views,
  // Escape clears the search / closes the detail panel.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const inField = (e.target as HTMLElement)?.tagName === "INPUT" || (e.target as HTMLElement)?.tagName === "SELECT";
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        setView("activity");
        setTimeout(() => searchRef.current?.focus(), 0);
      } else if ((e.ctrlKey || e.metaKey) && ["1", "2", "3", "4", "5"].includes(e.key)) {
        e.preventDefault();
        setView(VIEWS[Number(e.key) - 1]);
      } else if (e.key === "Escape" && !inField) {
        setSelected(null);
      } else if (e.key === "Escape" && inField && (e.target as HTMLInputElement) === searchRef.current) {
        setFilter("");
        searchRef.current?.blur();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const ruleCount = useMemo(() => {
    if (!config) return 0;
    return (
      Object.values(config.apps).filter(ruleActive).length +
      (ruleActive(config.global) ? 1 : 0) +
      (ruleActive(config.hotspot) ? 1 : 0) +
      config.connections.filter((c) => c.enabled && ruleActive(c)).length +
      config.adapters.filter((a) => a.enabled && ruleActive(a)).length
    );
  }, [config]);

  const goToApp = (key: string) => {
    setSelected(key);
    setView("activity");
  };

  const units = config?.units ?? "bits";

  return (
    <div className={`window ${focused ? "" : "blurred"} ${maximized ? "maximized" : ""}`}>
      <aside className="sidebar" style={{ width: sidebarWidth }} data-tauri-drag-region>
        <ResizeHandle side="right" width={sidebarWidth} onResize={(w) => setSidebarWidth(w < 0 ? 218 : w)} />
        <div className="brand" data-tauri-drag-region>
          <img src="/logo.png" alt="" draggable={false} data-tauri-drag-region />
          <span data-tauri-drag-region>Bandwidth Limiter</span>
        </div>
        <nav className="nav" aria-label="Main">
          <NavItem icon={<Activity />} label={t("nav.activity")} hint="Ctrl+1" active={view === "activity"} onClick={() => setView("activity")} />
          <NavItem icon={<BarChart3 />} label={t("nav.stats")} hint="Ctrl+2" active={view === "stats"} onClick={() => setView("stats")} />
          <NavItem icon={<SlidersHorizontal />} label={t("nav.rules")} hint="Ctrl+3" active={view === "rules"} onClick={() => setView("rules")} count={ruleCount || undefined} />
          <NavItem icon={<Network />} label={t("nav.network")} hint="Ctrl+4" active={view === "network"} onClick={() => setView("network")} />
          <NavItem icon={<Settings />} label={t("nav.settings")} hint="Ctrl+5" active={view === "settings"} onClick={() => setView("settings")} />
        </nav>
        <div className="sidebar-footer">
          <div className="master-row">
            <div>
              <div className="label">{t("limiter")}</div>
              <div className="sub">{config?.master ? (tick?.limiting ? t("limiter.active") : t("limiter.noRules")) : t("limiter.paused")}</div>
            </div>
            <Switch label={t("limiter")} on={config?.master ?? false} onChange={(v) => updateConfig((c) => ({ ...c, master: v }))} disabled={!config} />
          </div>
          <div className="status-line">
            <span className={`dot ${status ? (status.driverOk ? "ok" : "bad") : ""}`} />
            {status ? (status.driverOk ? t("driver.active") : t("driver.inactive")) : t("connecting")}
          </div>
          {tick && (
            <div className="status-line num">
              <span style={{ color: "var(--dl)" }}>↓ {formatRate(tick.total.dl, units)}</span>
              <span style={{ color: "var(--ul)" }}>↑ {formatRate(tick.total.ul, units)}</span>
            </div>
          )}
        </div>
      </aside>

      <main className="main">
        <header className="titlebar" data-tauri-drag-region>
          <h1 data-tauri-drag-region>{t(TITLE_KEYS[view])}</h1>
          <div className="spacer" data-tauri-drag-region />
          {view === "activity" && (
            <label className="search" title={t("search.hint")}>
              <Search />
              <input ref={searchRef} placeholder={t("search.placeholder")} aria-label={t("search.placeholder")} value={filter} onChange={(e) => setFilter(e.target.value)} />
            </label>
          )}
          <WindowControls maximized={maximized} />
        </header>

        {update && (
          <div style={{ padding: "12px 18px 0" }}>
            <div className="banner info">
              <Download />
              <div style={{ flex: 1 }}>
                <b>{t("update.available", { version: update.version })}</b> {t("update.current", { current: update.current })}
                {summarizeNotes(update.notes) && <span className="muted"> {summarizeNotes(update.notes)}</span>}
              </div>
              <button className="btn primary" disabled={installing} onClick={async () => { setInstalling(true); try { await api.installUpdate(); } catch { setInstalling(false); } }}>
                {installing ? t("update.installing") : t("update.install")}
              </button>
              <button className="btn ghost" onClick={() => setUpdate(null)}>{t("update.later")}</button>
            </div>
          </div>
        )}
        {(error || (status && !status.driverOk)) && (
          <div style={{ padding: "12px 18px 0" }}>
            <div className={`banner ${status && !status.elevated ? "" : "warn"}`}>
              {status && !status.elevated ? <ShieldAlert /> : <AlertTriangle />}
              <div>
                <b>{status && !status.elevated ? t("banner.admin") : t("banner.engine")} </b>
                {status?.driverError ?? error ?? t("banner.restart")}
              </div>
            </div>
          </div>
        )}

        {view === "activity" && <ActivityView filter={filter} selected={selected} onSelect={setSelected} />}
        {view === "stats" && <StatsView onSelect={goToApp} />}
        {view === "rules" && <RulesView onSelect={goToApp} />}
        {view === "network" && <NetworkView onSelect={goToApp} />}
        {view === "settings" && <SettingsView />}
      </main>

      {config && !config.onboardingDone && <Onboarding onDone={() => updateConfig((c) => ({ ...c, onboardingDone: true }))} />}
    </div>
  );
}

/** First real sentence of the release notes, without Markdown scaffolding. */
function summarizeNotes(notes: string | null): string {
  if (!notes) return "";
  for (const raw of notes.split("\n")) {
    const line = raw
      .replace(/^\s*#{1,6}\s*/, "") // headings
      .replace(/^\s*[-*+]\s+/, "") // list markers
      .replace(/[*_`]/g, "") // emphasis / code
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1") // links
      .trim();
    // Skip empty lines and heading-only lines such as "Added" / "Fixed".
    if (line.length > 20) return line.length > 160 ? line.slice(0, 157) + "…" : line;
  }
  return "";
}

function NavItem({ icon, label, hint, active, onClick, count }: { icon: React.ReactNode; label: string; hint?: string; active: boolean; onClick: () => void; count?: number }) {
  return (
    <button className={`nav-item ${active ? "active" : ""}`} onClick={onClick} title={hint} aria-current={active ? "page" : undefined}>
      {icon}
      {label}
      {count !== undefined && <span className="count">{count}</span>}
    </button>
  );
}
