import { Laptop, Wifi, Trash2 } from "lucide-react";
import { ruleActive, type Rule } from "../lib/api";
import { useEngine } from "../lib/engine";
import type { Units } from "../lib/format";
import { useT } from "../lib/i18n";
import { RuleEditor } from "./RuleEditor";
import { AppIcon, Switch, appDescription, appName } from "./ui";

export function RulesView({ onSelect }: { onSelect: (key: string) => void }) {
  const t = useT();
  const { config, updateConfig, tick, status } = useEngine();
  if (!config) return null;
  const units: Units = config.units;
  const appRules = Object.entries(config.apps).sort((a, b) => a[1].name.localeCompare(b[1].name));

  const setGlobal = (r: Rule) => updateConfig((c) => ({ ...c, global: r }));
  const setHotspot = (r: Rule) => updateConfig((c) => ({ ...c, hotspot: r }));

  return (
    <div className="content">
      <div className="content-scroll">
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
          <RuleEditor rule={config.global} units={units} onChange={setGlobal} />
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
          <RuleEditor rule={config.hotspot} units={units} onChange={setHotspot} />
        </div>

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
