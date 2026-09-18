import { useEffect, useState } from "react";
import { ExternalLink, Copy, FolderOpen, RefreshCw } from "lucide-react";
import { api, type Diagnostics, type UpdateInfo } from "../lib/api";
import { useEngine } from "../lib/engine";
import { useT, type LangSetting } from "../lib/i18n";
import { Segmented, Switch } from "./ui";

const APP_VERSION = __APP_VERSION__;
const REPO_URL = "https://github.com/reffer-191/bandwidth-limiter";

const LICENSES: { name: string; license: string; url: string }[] = [
  { name: "WinDivert 2.2.2", license: "LGPL-3.0", url: "https://github.com/basil00/WinDivert" },
  { name: "Tauri", license: "MIT / Apache-2.0", url: "https://tauri.app" },
  { name: "React", license: "MIT", url: "https://react.dev" },
  { name: "Lucide icons", license: "ISC", url: "https://lucide.dev" },
  { name: "Vite", license: "MIT", url: "https://vitejs.dev" },
  { name: "SQLite (via rusqlite)", license: "Public domain / MIT", url: "https://sqlite.org" },
  { name: "tauri-winrt-notification", license: "MIT / Apache-2.0", url: "https://github.com/tauri-apps/winrt-notification" },
  { name: "window-vibrancy, parking_lot, serde, libloading, png (Rust crates)", license: "MIT / Apache-2.0", url: "https://crates.io" },
  { name: "NSIS", license: "zlib/libpng", url: "https://nsis.sourceforge.io" },
];

function openExternal(url: string) {
  import("@tauri-apps/plugin-opener").then((m) => m.openUrl(url)).catch(() => window.open(url, "_blank"));
}

export function SettingsView() {
  const t = useT();
  const { config, updateConfig, status, refreshStatus } = useEngine();
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autostartError, setAutostartError] = useState<string | null>(null);
  const [licenses, setLicenses] = useState(false);
  const [notifyError, setNotifyError] = useState<string | null>(null);
  const [updateState, setUpdateState] = useState<{ status: "idle" | "checking" | "none" | "available" | "installing" | "error"; info?: UpdateInfo; error?: string }>({ status: "idle" });

  const checkNow = async () => {
    setUpdateState({ status: "checking" });
    try {
      const info = await api.checkUpdate();
      setUpdateState(info ? { status: "available", info } : { status: "none" });
    } catch (e) {
      setUpdateState({ status: "error", error: String(e) });
    }
  };
  const installNow = async () => {
    setUpdateState((s) => ({ ...s, status: "installing" }));
    try {
      await api.installUpdate();
    } catch (e) {
      setUpdateState({ status: "error", error: String(e) });
    }
  };

  useEffect(() => {
    api.autostart().then(setAutostart).catch(() => setAutostart(false));
    // Packet counter / last error are live values.
    const id = setInterval(() => refreshStatus().catch(() => {}), 2000);
    return () => clearInterval(id);
  }, [refreshStatus]);
  const toggleAutostart = async (v: boolean) => {
    setAutostart(v);
    setAutostartError(null);
    try {
      setAutostart(await api.setAutostart(v));
    } catch (e) {
      setAutostart(!v);
      setAutostartError(String(e));
    }
  };

  if (!config) return null;
  return (
    <div className="content">
      <div className="content-scroll">
        <div className="section-title">{t("set.appearance")}</div>
        <div className="card">
          <Row title={t("set.language")} desc={t("set.language.desc")}>
            <Segmented<LangSetting>
              value={config.language}
              onChange={(language) => updateConfig((c) => ({ ...c, language }))}
              options={[
                { value: "system", label: t("set.language.system") },
                { value: "es", label: "Español" },
                { value: "en", label: "English" },
              ]}
            />
          </Row>
          <Row title={t("set.theme")} desc={t("set.theme.desc")}>
            <Segmented
              value={config.theme}
              onChange={(theme) => updateConfig((c) => ({ ...c, theme }))}
              options={[
                { value: "system", label: t("set.theme.system") },
                { value: "light", label: t("set.theme.light") },
                { value: "dark", label: t("set.theme.dark") },
              ]}
            />
          </Row>
          <Row title={t("set.units")} desc={t("set.units.desc")}>
            <Segmented
              value={config.units}
              onChange={(units) => updateConfig((c) => ({ ...c, units }))}
              options={[
                { value: "bits", label: "Mbit/s" },
                { value: "bytes", label: "MB/s" },
              ]}
            />
          </Row>
          <Row title={t("set.history")} desc={t("set.history.desc")}>
            <Segmented
              value={config.historyMinutes}
              onChange={(historyMinutes) => updateConfig((c) => ({ ...c, historyMinutes }))}
              options={[1, 5, 10, 15, 30, 60].map((m) => ({ value: m, label: `${m}m` }))}
            />
          </Row>
        </div>

        <div className="section-title">{t("set.startup")}</div>
        <div className="card">
          <Row title={t("set.autostart")} desc={<>{t("set.autostart.desc")}{autostartError && <span style={{ color: "var(--red)" }}> {autostartError}</span>}</>}>
            <Switch label={t("set.autostart")} on={!!autostart} onChange={toggleAutostart} disabled={autostart === null} />
          </Row>
          <Row title={t("set.startMin")} desc={t("set.startMin.desc")}>
            <Switch label={t("set.startMin")} on={config.startMinimized} onChange={(startMinimized) => updateConfig((c) => ({ ...c, startMinimized }))} />
          </Row>
        </div>

        <div className="section-title">{t("set.updates")}</div>
        <div className="card">
          <Row title={t("set.checkUpdates")} desc={t("set.checkUpdates.desc")}>
            <Switch label={t("set.checkUpdates")} on={config.checkUpdates} onChange={(checkUpdates) => updateConfig((c) => ({ ...c, checkUpdates }))} />
          </Row>
          <Row
            title={t("set.version", { version: APP_VERSION })}
            desc={
              <>
                {updateState.status === "idle" && t("set.update.idle")}
                {updateState.status === "checking" && t("set.update.checking")}
                {updateState.status === "none" && t("set.update.none")}
                {updateState.status === "available" && t("set.update.available", { version: updateState.info?.version ?? "" })}
                {updateState.status === "installing" && t("set.update.installing")}
                {updateState.status === "error" && <span style={{ color: "var(--red)" }}>{updateState.error}</span>}
              </>
            }
          >
            {updateState.status === "available" ? (
              <button className="btn primary" onClick={installNow}>{t("update.install")}</button>
            ) : (
              <button className="btn" onClick={checkNow} disabled={updateState.status === "checking" || updateState.status === "installing"}>{t("set.update.check")}</button>
            )}
          </Row>
        </div>

        <div className="section-title">{t("set.notifications")}</div>
        <div className="card">
          <Row title={t("set.notify.newApp")} desc={t("set.notify.newApp.desc")}>
            <Switch label={t("set.notify.newApp")} on={config.notifyNewApp} onChange={(notifyNewApp) => updateConfig((c) => ({ ...c, notifyNewApp }))} />
          </Row>
          <Row title={t("set.notify.quota")} desc={t("set.notify.quota.desc")}>
            <Switch label={t("set.notify.quota")} on={config.notifyQuota} onChange={(notifyQuota) => updateConfig((c) => ({ ...c, notifyQuota }))} />
          </Row>
          <Row title={t("set.notify.schedule")} desc={<>{t("set.notify.schedule.desc")}{notifyError && <span style={{ color: "var(--red)" }}> {notifyError}</span>}</>}>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <button className="btn ghost" onClick={() => { setNotifyError(null); api.testNotification().catch((e) => setNotifyError(String(e))); }}>{t("set.notify.test")}</button>
              <Switch label={t("set.notify.schedule")} on={config.notifySchedule} onChange={(notifySchedule) => updateConfig((c) => ({ ...c, notifySchedule }))} />
            </div>
          </Row>
        </div>

        <div className="section-title">{t("set.tray")}</div>
        <div className="card">
          <Row title={t("set.minToTray")} desc={t("set.minToTray.desc")}>
            <Switch label={t("set.minToTray")} on={config.minimizeToTray} onChange={(minimizeToTray) => updateConfig((c) => ({ ...c, minimizeToTray }))} />
          </Row>
          <Row title={t("set.closeToTray")} desc={t("set.closeToTray.desc")}>
            <Switch label={t("set.closeToTray")} on={config.closeToTray} onChange={(closeToTray) => updateConfig((c) => ({ ...c, closeToTray }))} />
          </Row>
        </div>

        <div className="section-title">{t("set.engine")}</div>
        <div className="card">
          <Row title={t("set.admin")} desc={t("set.admin.desc")}>
            <span className={`badge ${status?.elevated ? "green" : "block"}`}>{status?.elevated ? t("set.elevated") : t("set.notElevated")}</span>
          </Row>
          <Row
            title={t("set.driver")}
            desc={
              <>
                <span className="mono" style={{ fontSize: 11 }}>{status?.windivertPath || t("set.driver.notLoaded")}</span>
                {status?.driverOk && <><br />{t("set.packets", { n: (status.packets ?? 0).toLocaleString() })}</>}
                {status?.lastError && <><br /><span style={{ color: "var(--red)" }}>{status.lastError}</span></>}
              </>
            }
          >
            <span className={`badge ${status?.driverOk ? "green" : "block"}`}>{status?.driverOk ? t("set.active") : status?.driverError ?? t("set.inactive")}</span>
          </Row>
          <Row title={t("set.forward")} desc={t("set.forward.desc")}>
            <span className={`badge ${status?.forwardOk ? "green" : ""}`}>{status?.forwardOk ? t("set.active") : status?.forwardError ?? t("set.inactive")}</span>
          </Row>
          <Row title={t("set.countHeaders")} desc={t("set.countHeaders.desc")}>
            <Switch label={t("set.countHeaders")} on={config.countHeaders} onChange={(countHeaders) => updateConfig((c) => ({ ...c, countHeaders }))} />
          </Row>
        </div>

        <div className="section-title">{t("set.diag")}</div>
        <DiagnosticsCard />

        <div className="section-title">{t("set.data")}</div>
        <div className="card">
          <Row title={t("set.config")} desc={<span className="mono" style={{ fontSize: 11 }}>{status?.configPath || "—"}</span>}>
            <span className="badge">{status?.configPath?.toLowerCase().includes("appdata") ? t("set.installed") : t("set.portable")}</span>
          </Row>
          <Row title={t("set.usage")} desc={<><span>{t("set.usage.desc")}</span><br /><span className="mono" style={{ fontSize: 11 }}>{status?.configPath ? status.configPath.replace(/config[.]json$/i, "usage.db") : "—"}</span></>} />
        </div>

        <div className="section-title">{t("set.about")}</div>
        <div className="card">
          <Row title={`Bandwidth Limiter ${APP_VERSION}`} desc={<>{t("set.about.desc")} MIT · © 2026 reffer-191</>}>
            <button className="btn" onClick={() => openExternal(REPO_URL)}>
              <ExternalLink style={{ width: 13, height: 13, verticalAlign: -2, marginRight: 5 }} />
              {t("set.website")}
            </button>
          </Row>
          <Row title={t("set.licenses")} desc={licenses ? undefined : "WinDivert (LGPL-3.0), Tauri, React, Lucide…"}>
            <button className="btn ghost" onClick={() => setLicenses((v) => !v)}>{licenses ? t("set.licenses.hide") : t("set.licenses.show")}</button>
          </Row>
          {licenses && (
            <div className="licenses">
              {LICENSES.map((l) => (
                <div className="kv" key={l.name}>
                  <span className="k">
                    <a href={l.url} onClick={(e) => { e.preventDefault(); openExternal(l.url); }}>{l.name}</a>
                  </span>
                  <span className="v">{l.license}</span>
                </div>
              ))}
            </div>
          )}
          <Row title={t("set.onboarding")}>
            <button className="btn ghost" onClick={() => updateConfig((c) => ({ ...c, onboardingDone: false }))}>{t("set.licenses.show")}</button>
          </Row>
        </div>
      </div>
    </div>
  );
}

function DiagnosticsCard() {
  const t = useT();
  const [d, setD] = useState<Diagnostics | null>(null);
  const [copied, setCopied] = useState(false);
  const load = () => api.diagnostics().then(setD).catch(() => {});
  useEffect(() => {
    load();
    const id = setInterval(load, 5000);
    return () => clearInterval(id);
  }, []);
  const copy = async () => {
    if (!d) return;
    try {
      await navigator.clipboard.writeText(d.report);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = d.report;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };
  const up = (s: number) => (s >= 3600 ? `${Math.floor(s / 3600)} h ${Math.floor((s % 3600) / 60)} min` : s >= 60 ? `${Math.floor(s / 60)} min` : `${s} s`);
  if (!d) return <div className="card"><div className="empty">…</div></div>;
  const st = d.status;
  const healthy = st.driverOk && st.threads >= 3 && !st.lastError;
  return (
    <div className="card">
      <Row title={t("set.diag.state")} desc={t("set.diag.state.desc", { uptime: up(d.uptimeSecs), windows: d.windows })}>
        <span className={`badge ${healthy ? "green" : st.driverOk ? "" : "block"}`}>{healthy ? t("set.diag.ok") : st.driverOk ? t("set.diag.degraded") : t("set.inactive")}</span>
      </Row>
      <div className="diag-grid">
        <span className="k">{t("set.diag.threads")}</span><span className="v">{st.threads} / 4</span>
        <span className="k">{t("set.diag.packets")}</span><span className="v">{st.packets.toLocaleString()}</span>
        <span className="k">{t("set.diag.queued")}</span><span className="v">{d.queuedBytes.toLocaleString()} B</span>
        <span className="k">{t("set.diag.dropped")}</span><span className="v">{d.dropped.toLocaleString()}</span>
        <span className="k">{t("set.diag.sendErrors")}</span><span className="v">{st.sendErrors.toLocaleString()}</span>
        <span className="k">{t("set.diag.restarts")}</span><span className="v">{st.restarts}</span>
        <span className="k">{t("set.diag.adapters")}</span><span className="v">{d.adapters}{d.metered ? ` · ${t("metered")}` : ""}</span>
        <span className="k">{t("set.diag.log")}</span><span className="v mono" style={{ fontSize: 11 }}>{d.logPath}</span>
      </div>
      {(st.lastError || d.problems.length > 0) && (
        <div className="diag-problems">
          {st.lastError && <div className="line err">{st.lastError}</div>}
          {d.problems.map((l, i) => <div className="line" key={i}>{l}</div>)}
        </div>
      )}
      <Row title={t("set.diag.report")} desc={t("set.diag.report.desc")}>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn ghost icon" onClick={load} title={t("net.refresh")} aria-label={t("net.refresh")}><RefreshCw /></button>
          <button className="btn ghost" onClick={() => api.openLogFolder().catch(() => {})}><FolderOpen /> {t("set.diag.openLogs")}</button>
          <button className="btn" onClick={copy}><Copy /> {copied ? t("set.diag.copied") : t("set.diag.copy")}</button>
        </div>
      </Row>
    </div>
  );
}

function Row({ title, desc, children }: { title: string; desc?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="settings-row">
      <div className="l">
        <span className="t">{title}</span>
        {desc && <span className="s">{desc}</span>}
      </div>
      {children}
    </div>
  );
}
