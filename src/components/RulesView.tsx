import { Laptop, Wifi, Trash2 } from "lucide-react";
import { ruleActive, type Rule } from "../lib/api";
import { useEngine } from "../lib/engine";
import type { Units } from "../lib/format";
import { RuleEditor } from "./RuleEditor";
import { AppIcon, Switch } from "./ui";

export function RulesView({ onSelect }: { onSelect: (key: string) => void }) {
  const { config, updateConfig, tick, status } = useEngine();
  if (!config) return null;
  const units: Units = config.units;
  const appRules = Object.entries(config.apps).sort((a, b) => a[1].name.localeCompare(b[1].name));

  const setGlobal = (r: Rule) => updateConfig((c) => ({ ...c, global: r }));
  const setHotspot = (r: Rule) => updateConfig((c) => ({ ...c, hotspot: r }));

  return (
    <div className="content">
      <div className="content-scroll">
        <div className="section-title">Límites generales</div>

        <div className="card rule-card">
          <div className="who">
            <div className="app-icon lg" style={{ background: "var(--accent-soft)", color: "var(--accent)" }}>
              <Laptop />
            </div>
            <div className="titles">
              <div className="n">Todo el equipo</div>
              <div className="d">
                Techo para la suma de todo el tráfico, incluido el que reenvías a los dispositivos del hotspot. Ninguna app ni dispositivo podrá superarlo.
              </div>
              <div className="actions" style={{ alignItems: "center", gap: 8 }}>
                <Switch small on={config.globalInternetOnly} onChange={(v) => updateConfig((c) => ({ ...c, globalInternetOnly: v }))} />
                <span className="muted" style={{ fontSize: 12 }}>
                  Solo tráfico de Internet (no contar la red local)
                </span>
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
              <div className="n">Hotspot / red compartida</div>
              <div className="d">
                Límite propio para el conjunto de dispositivos conectados a tu punto de acceso. Se aplica además del límite del equipo.
              </div>
              {status && !status.forwardOk && (
                <div className="d" style={{ color: "var(--red)", marginTop: 4 }}>
                  Captura de tráfico reenviado no disponible: {status.forwardError ?? "driver no cargado"}
                </div>
              )}
            </div>
          </div>
          <RuleEditor rule={config.hotspot} units={units} onChange={setHotspot} />
        </div>

        <div className="section-title">Aplicaciones y dispositivos · {appRules.length}</div>

        {appRules.length === 0 && (
          <div className="card empty">
            No hay reglas por aplicación. Selecciona una app en <b>Actividad</b> para limitarla.
          </div>
        )}

        {appRules.map(([key, r]) => {
          const live = tick?.apps.find((a) => a.key === key);
          const isDevice = key.startsWith("hotspot:");
          return (
            <div className="card rule-card" key={key}>
              <div className="who">
                <AppIcon appKey={key} hasExe={!!r.exe} isDevice={isDevice} large />
                <div className="titles">
                  <div className="n" style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    {r.name}
                    <span className={`online ${live && (live.pids.length || live.isDevice) ? "" : "off"}`} title={live?.pids.length ? "En ejecución" : "Sin proceso activo"} />
                  </div>
                  <div className="d">{r.description}</div>
                  {r.exe && <div className="p">{r.exe}</div>}
                  <div className="actions">
                    <button className="btn ghost" onClick={() => onSelect(key)}>Ver actividad</button>
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
                      Quitar regla
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
