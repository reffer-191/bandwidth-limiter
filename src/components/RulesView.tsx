import { useEffect, useMemo, useState } from "react";
import { Laptop, Wifi, Trash2, Cable, Gauge, Globe, Plus, Download, Upload, Layers, Pencil, X } from "lucide-react";
import { api, emptyRule, newId, ruleActive, type Adapter, type AdapterRule, type ConnRule, type Config, type Rule } from "../lib/api";
import { useEngine } from "../lib/engine";
import type { Units } from "../lib/format";
import { useT, type T } from "../lib/i18n";
import { MIN_RATE_GENERAL, RuleEditor } from "./RuleEditor";
import { AppIcon, Switch, appDescription, appName } from "./ui";

export function RulesView({ onSelect }: { onSelect: (key: string) => void }) {
  const t = useT();
  const { config, updateConfig, tick, status } = useEngine();
  if (!config) return null;
  const units: Units = config.units;
  const appRules = Object.entries(config.apps).sort((a, b) => a[1].name.localeCompare(b[1].name));
  const states = tick?.states ?? {};

  const setGlobal = (r: Rule) => updateConfig((c) => ({ ...c, global: r }));
  const setHotspot = (r: Rule) => updateConfig((c) => ({ ...c, hotspot: r }));

  return (
    <div className="content">
      <div className="content-scroll">
        <ProfilesBar config={config} />

        <div className="section-title">{t("rules.general")}</div>

        <div className="card rule-card">
          <div className="who">
            <div className="app-icon lg" style={{ background: "var(--accent-soft)", color: "var(--accent)" }}>
              <Laptop />
            </div>
            <div className="titles">
              <div className="n">{t("rules.pc")}</div>
              <div className="d">{t("rules.pc.desc")}</div>
              <div className="actions" style={{ alignItems: "center", gap: 8 }}>
                <Switch small on={config.globalInternetOnly} onChange={(v) => updateConfig((c) => ({ ...c, globalInternetOnly: v }))} />
                <span className="muted" style={{ fontSize: 12 }}>{t("rules.internetOnly")}</span>
              </div>
            </div>
          </div>
          <RuleEditor rule={config.global} units={units} onChange={setGlobal} minRate={MIN_RATE_GENERAL} state={states.global} />
        </div>

        <div className="card rule-card">
          <div className="who">
            <div className="app-icon lg" style={{ background: "rgba(48,209,88,0.15)", color: "var(--green)" }}>
              <Wifi />
            </div>
            <div className="titles">
              <div className="n">{t("rules.hotspot")}</div>
              <div className="d">{t("rules.hotspot.desc")}</div>
              {status && !status.forwardOk && (
                <div className="d" style={{ color: "var(--red)", marginTop: 4 }}>
                  {t("rules.forwardUnavailable", { error: status.forwardError ?? t("rules.driverNotLoaded") })}
                </div>
              )}
            </div>
          </div>
          <RuleEditor rule={config.hotspot} units={units} onChange={setHotspot} minRate={MIN_RATE_GENERAL} state={states.hotspot} />
        </div>

        <AdapterRules config={config} units={units} metered={tick?.metered ?? false} />
        <ConnectionRules config={config} units={units} />

        <div className="section-title">{t("rules.appsAndDevices")} · {appRules.length}</div>

        {appRules.length === 0 && (
          <div className="card empty">
            {t("rules.empty.pre")} <b>{t("nav.activity")}</b> {t("rules.empty.post")}
          </div>
        )}

        {appRules.map(([key, r]) => {
          const live = tick?.apps.find((a) => a.key === key);
          const isDevice = key.startsWith("hotspot:");
          const meta = { key, name: r.name, description: r.description, isDevice };
          return (
            <div className="card rule-card" key={key}>
              <div className="who">
                <AppIcon appKey={key} hasExe={!!r.exe} isDevice={isDevice} large />
                <div className="titles">
                  <div className="n" style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    {appName(meta, t)}
                    <span className={`online ${live && (live.pids.length || live.isDevice) ? "" : "off"}`} title={live?.pids.length ? t("rules.running") : t("rules.notRunning")} />
                  </div>
                  <div className="d">{appDescription(meta, t)}</div>
                  {r.exe && <div className="p">{r.exe}</div>}
                  <div className="actions">
                    <button className="btn ghost" onClick={() => onSelect(key)}>{t("rules.viewActivity")}</button>
                    <button
                      className="btn ghost danger"
                      onClick={() =>
                        updateConfig((c) => {
                          const apps = { ...c.apps };
                          delete apps[key];
                          return { ...c, apps };
                        })
                      }
                    >
                      <Trash2 style={{ width: 13, height: 13, verticalAlign: -2, marginRight: 4 }} />
                      {t("rules.remove")}
                    </button>
                  </div>
                </div>
              </div>
              <RuleEditor
                rule={r}
                units={units}
                showPriority
                state={states[key]}
                onChange={(nr) =>
                  updateConfig((c) => {
                    const apps = { ...c.apps };
                    if (ruleActive(nr)) apps[key] = { ...r, ...nr };
                    else delete apps[key];
                    return { ...c, apps };
                  })
                }
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}

/* ---------- Profiles + import/export ---------- */

function ProfilesBar({ config }: { config: Config }) {
  const t = useT();
  const { updateConfig, setConfigFromBackend } = useEngine();
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** Inline editor: creating / renaming / confirming a delete. */
  const [mode, setMode] = useState<"new" | "rename" | "delete" | null>(null);
  const [name, setName] = useState("");
  useEffect(() => {
    if (!msg) return;
    const id = setTimeout(() => setMsg(null), 4000);
    return () => clearTimeout(id);
  }, [msg]);

  const switchTo = async (name: string) => {
    try {
      setConfigFromBackend(await api.switchProfile(name));
    } catch (e) {
      setError(String(e));
    }
  };
  const commit = () => {
    const n = name.trim();
    if (mode === "new" && n) {
      updateConfig((c) => {
        const live = { global: c.global, globalInternetOnly: c.globalInternetOnly, hotspot: c.hotspot, apps: c.apps, connections: c.connections, adapters: c.adapters };
        const profiles = c.profiles.filter((p) => p.name.toLowerCase() !== n.toLowerCase());
        // The profile we leave keeps the rules as they are right now.
        const prev = profiles.find((p) => p.name === c.activeProfile);
        if (prev) prev.rules = structuredClone(live);
        profiles.push({ name: n, rules: structuredClone(live) });
        return { ...c, profiles, activeProfile: n };
      });
    } else if (mode === "rename" && n && n !== config.activeProfile) {
      updateConfig((c) => ({
        ...c,
        profiles: c.profiles.filter((p) => p.name.toLowerCase() !== n.toLowerCase() || p.name === c.activeProfile).map((p) => (p.name === c.activeProfile ? { ...p, name: n } : p)),
        activeProfile: n,
      }));
    } else if (mode === "delete") {
      updateConfig((c) => ({ ...c, profiles: c.profiles.filter((p) => p.name !== c.activeProfile), activeProfile: "" }));
    }
    setMode(null);
  };
  const doExport = async () => {
    try {
      const path = await api.exportRules();
      if (path) setMsg(t("rules.exported", { path }));
    } catch (e) {
      setError(String(e));
    }
  };
  const doImport = async () => {
    try {
      const cfg = await api.importRules();
      if (cfg) {
        setConfigFromBackend(cfg);
        setMsg(t("rules.imported"));
      }
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="card profiles">
      <div className="profiles-row">
        <Layers className="ic" />
        <div className="titles">
          <div className="n">{t("rules.profiles")}</div>
          <div className="d">{config.activeProfile || t("rules.profile.none")}</div>
        </div>
        <div className="spacer" />
        {mode === "new" || mode === "rename" ? (
          <>
            <input
              className="text"
              autoFocus
              value={name}
              placeholder={t("rules.profile.prompt")}
              aria-label={t("rules.profile.prompt")}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => (e.key === "Enter" ? commit() : e.key === "Escape" ? setMode(null) : undefined)}
              style={{ width: 160 }}
            />
            <button className="btn primary" onClick={commit} disabled={!name.trim()}>{t("ob.done.ok")}</button>
            <button className="btn ghost" onClick={() => setMode(null)}>{t("ob.cancel")}</button>
          </>
        ) : mode === "delete" ? (
          <>
            <span className="muted" style={{ fontSize: 12 }}>{t("rules.profile.confirmDelete", { name: config.activeProfile })}</span>
            <button className="btn danger" onClick={commit}>{t("rules.profile.delete")}</button>
            <button className="btn ghost" onClick={() => setMode(null)}>{t("ob.cancel")}</button>
          </>
        ) : (
          <>
            <select className="select" value={config.activeProfile} onChange={(e) => switchTo(e.target.value)} aria-label={t("rules.profiles")}>
              {!config.activeProfile && <option value="">{t("rules.profile.none")}</option>}
              {config.profiles.map((p) => (
                <option key={p.name} value={p.name}>{p.name}</option>
              ))}
            </select>
            <button className="btn" onClick={() => { setName(""); setMode("new"); }} title={t("rules.profile.new")}><Plus /> {t("rules.profile.new")}</button>
            {config.activeProfile && (
              <>
                <button className="btn ghost icon" onClick={() => { setName(config.activeProfile); setMode("rename"); }} title={t("rules.profile.rename")} aria-label={t("rules.profile.rename")}><Pencil /></button>
                <button className="btn ghost icon danger" onClick={() => setMode("delete")} title={t("rules.profile.delete")} aria-label={t("rules.profile.delete")}><Trash2 /></button>
              </>
            )}
            <span className="vsep" />
            <button className="btn ghost" onClick={doExport}><Download /> {t("rules.export")}</button>
            <button className="btn ghost" onClick={doImport}><Upload /> {t("rules.import")}</button>
          </>
        )}
      </div>
      <div className="profiles-hint">{t("rules.profile.hint")}</div>
      {(msg || error) && (
        <div className={`rule-summary ${error ? "warn" : ""}`} style={{ padding: "0 14px 10px" }}>
          {error ?? msg}
          {error && <button className="btn ghost icon" style={{ marginLeft: 6 }} onClick={() => setError(null)} aria-label={t("win.close")}><X /></button>}
        </div>
      )}
    </div>
  );
}

/* ---------- Adapter rules ---------- */

export function adapterLabel(a: string, t: T): string {
  if (a === "wifi") return t("adapter.wifi");
  if (a === "ethernet") return t("adapter.ethernet");
  if (a === "metered") return t("adapter.metered");
  return a.replace(/^name:/, "");
}

function AdapterRules({ config, units, metered }: { config: Config; units: Units; metered: boolean }) {
  const t = useT();
  const { updateConfig, tick } = useEngine();
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [choice, setChoice] = useState("wifi");
  useEffect(() => {
    api.adapters().then(setAdapters).catch(() => {});
  }, []);
  const options = useMemo(() => {
    const base = [
      { value: "wifi", label: t("adapter.wifi") },
      { value: "ethernet", label: t("adapter.ethernet") },
      { value: "metered", label: `${t("adapter.metered")}${metered ? ` (${t("adapter.metered.now")})` : ""}` },
    ];
    const named = adapters.map((a) => ({ value: `name:${a.name}`, label: `${t("adapter.name")}: ${a.name}` }));
    return [...base, ...named];
  }, [adapters, metered, t]);
  const states = tick?.states ?? {};

  const setRule = (id: string, mutate: (r: AdapterRule) => AdapterRule | null) =>
    updateConfig((c) => ({ ...c, adapters: c.adapters.flatMap((a) => (a.id === id ? (mutate(a) ? [mutate(a)!] : []) : [a])) }));
  const add = () =>
    updateConfig((c) => ({ ...c, adapters: [...c.adapters, { ...emptyRule(), id: newId(), enabled: true, adapter: choice }] }));

  return (
    <>
      <div className="section-title">{t("rules.adapters")} · {config.adapters.length}</div>
      <div className="card">
        <div className="sub-header">
          <span className="d">{t("rules.adapters.desc")} {t("adapter.metered.desc")}</span>
          <div className="spacer" />
          <select className="select" value={choice} onChange={(e) => setChoice(e.target.value)} aria-label={t("adapter.name")}>
            {options.map((o) => (
              <option key={o.value} value={o.value}>{o.label}</option>
            ))}
          </select>
          <button className="btn" onClick={add}><Plus /> {t("rules.adapters.add")}</button>
        </div>
        {config.adapters.length === 0 && <div className="empty" style={{ padding: "14px 16px 18px" }}>{t("rules.adapters.empty")}</div>}
        {config.adapters.map((a) => {
          const icon = a.adapter === "wifi" ? <Wifi /> : a.adapter === "ethernet" ? <Cable /> : a.adapter === "metered" ? <Gauge /> : <Globe />;
          return (
            <div className="rule-card inner" key={a.id}>
              <div className="who">
                <div className="app-icon lg" style={{ background: "var(--chip-bg)", color: "var(--text-2)" }}>{icon}</div>
                <div className="titles">
                  <div className="n" style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    {adapterLabel(a.adapter, t)}
                    {a.adapter === "metered" && metered && <span className="badge green">{t("adapter.metered.now")}</span>}
                  </div>
                  <div className="actions" style={{ alignItems: "center", gap: 8 }}>
                    <Switch small label={t("conn.enabled")} on={a.enabled} onChange={(enabled) => setRule(a.id, (r) => ({ ...r, enabled }))} />
                    <span className="muted" style={{ fontSize: 12 }}>{t("conn.enabled")}</span>
                    <button className="btn ghost danger" onClick={() => setRule(a.id, () => null)}>
                      <Trash2 style={{ width: 13, height: 13, verticalAlign: -2, marginRight: 4 }} />
                      {t("conn.remove")}
                    </button>
                  </div>
                </div>
              </div>
              <RuleEditor rule={a} units={units} minRate={MIN_RATE_GENERAL} state={states[`adapter:${a.id}`]} onChange={(nr) => setRule(a.id, (r) => ({ ...r, ...nr }))} />
            </div>
          );
        })}
      </div>
    </>
  );
}

/* ---------- Connection rules ---------- */

function ConnectionRules({ config, units }: { config: Config; units: Units }) {
  const t = useT();
  const { updateConfig, tick } = useEngine();
  const states = tick?.states ?? {};
  const apps = useMemo(() => {
    const seen = new Map<string, string>();
    for (const a of tick?.apps ?? []) if (!a.isDevice && a.key !== "unknown") seen.set(a.key, appName(a, t));
    for (const [k, r] of Object.entries(config.apps)) if (!seen.has(k) && !k.startsWith("hotspot:")) seen.set(k, r.name);
    for (const c of config.connections) if (c.app && !seen.has(c.app)) seen.set(c.app, c.app.split("\\").pop() ?? c.app);
    return [...seen.entries()].sort((a, b) => a[1].localeCompare(b[1], undefined, { sensitivity: "base" }));
  }, [tick, config.apps, config.connections, t]);

  const setRule = (id: string, mutate: (r: ConnRule) => ConnRule | null) =>
    updateConfig((c) => ({ ...c, connections: c.connections.flatMap((r) => (r.id === id ? (mutate(r) ? [mutate(r)!] : []) : [r])) }));
  const add = () =>
    updateConfig((c) => ({
      ...c,
      connections: [...c.connections, { ...emptyRule(), id: newId(), name: "", enabled: true, host: "", ports: "", protocol: "any", app: "" }],
    }));

  return (
    <>
      <div className="section-title">{t("rules.connections")} · {config.connections.length}</div>
      <div className="card">
        <div className="sub-header">
          <span className="d">{t("rules.connections.desc")}</span>
          <div className="spacer" />
          <button className="btn" onClick={add}><Plus /> {t("rules.connections.add")}</button>
        </div>
        {config.connections.length === 0 && <div className="empty" style={{ padding: "14px 16px 18px" }}>{t("rules.connections.empty")}</div>}
        {config.connections.map((c) => (
          <div className="rule-card inner" key={c.id}>
            <div className="who conn-form">
              <TextField label={t("conn.name")} value={c.name} placeholder={t("conn.unnamed")} onCommit={(name) => setRule(c.id, (r) => ({ ...r, name }))} />
              <TextField label={t("conn.host")} value={c.host} placeholder={t("conn.host.ph")} mono onCommit={(host) => setRule(c.id, (r) => ({ ...r, host }))} />
              <div className="conn-grid">
                <TextField label={t("conn.ports")} value={c.ports} placeholder={t("conn.ports.ph")} mono onCommit={(ports) => setRule(c.id, (r) => ({ ...r, ports }))} />
                <label className="lbl">
                  <span>{t("conn.protocol")}</span>
                  <select className="select" value={c.protocol} onChange={(e) => setRule(c.id, (r) => ({ ...r, protocol: e.target.value as ConnRule["protocol"] }))}>
                    <option value="any">{t("conn.any")}</option>
                    <option value="tcp">TCP</option>
                    <option value="udp">UDP</option>
                  </select>
                </label>
              </div>
              <label className="lbl">
                <span>{t("conn.app")}</span>
                <select className="select" value={c.app} onChange={(e) => setRule(c.id, (r) => ({ ...r, app: e.target.value }))}>
                  <option value="">{t("conn.app.any")}</option>
                  {apps.map(([k, n]) => (
                    <option key={k} value={k}>{n}</option>
                  ))}
                </select>
              </label>
              <div className="actions" style={{ alignItems: "center", gap: 8 }}>
                <Switch small label={t("conn.enabled")} on={c.enabled} onChange={(enabled) => setRule(c.id, (r) => ({ ...r, enabled }))} />
                <span className="muted" style={{ fontSize: 12 }}>{t("conn.enabled")}</span>
                <button className="btn ghost danger" onClick={() => setRule(c.id, () => null)}>
                  <Trash2 style={{ width: 13, height: 13, verticalAlign: -2, marginRight: 4 }} />
                  {t("conn.remove")}
                </button>
              </div>
            </div>
            <RuleEditor rule={c} units={units} state={states[`conn:${c.id}`]} onChange={(nr) => setRule(c.id, (r) => ({ ...r, ...nr }))} />
          </div>
        ))}
      </div>
    </>
  );
}

/** Text input that commits on blur/Enter so every keystroke does not save the config. */
function TextField({ label, value, placeholder, mono, onCommit }: { label: string; value: string; placeholder?: string; mono?: boolean; onCommit: (v: string) => void }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  return (
    <label className="lbl">
      <span>{label}</span>
      <input
        className={`text ${mono ? "mono" : ""}`}
        value={text}
        placeholder={placeholder}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => text !== value && onCommit(text.trim())}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      />
    </label>
  );
}
