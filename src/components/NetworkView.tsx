import { useEffect, useState } from "react";
import { Cable, Wifi, Network, Smartphone, Globe, RefreshCw } from "lucide-react";
import { api, type Adapter } from "../lib/api";
import { useEngine } from "../lib/engine";
import { formatBytes, formatRate } from "../lib/format";
import { useT } from "../lib/i18n";
import { RuleBadges } from "./ActivityView";
import { appDescription } from "./ui";

export function NetworkView({ onSelect }: { onSelect: (key: string) => void }) {
  const t = useT();
  const { tick, config } = useEngine();
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const units = config?.units ?? "bits";

  const load = () => api.adapters().then(setAdapters).catch(() => {});
  useEffect(() => {
    load();
    const id = setInterval(load, 10000);
    return () => clearInterval(id);
  }, []);

  const devices = (tick?.apps ?? []).filter((a) => a.isDevice).sort((a, b) => b.dl + b.ul - (a.dl + a.ul));
  const sorted = [...adapters].sort((a, b) => Number(b.up) - Number(a.up) || Number(b.is_hotspot) - Number(a.is_hotspot));

  return (
    <div className="content">
      <div className="content-scroll">
        <div className="card">
          <div className="card-header">
            <h2>{t("net.devices")}</h2>
            <span className="faint">{devices.length}</span>
            <div className="spacer" />
            <span className="muted" style={{ fontSize: 12 }}>
              ↓ {formatRate(tick?.hotspot.dl ?? 0, units)} · ↑ {formatRate(tick?.hotspot.ul ?? 0, units)}
            </span>
          </div>
          {devices.length === 0 ? (
            <div className="empty">{t("net.noDevices")}</div>
          ) : (
            <table className="table" style={{ marginTop: 8 }}>
              <thead>
                <tr>
                  <th>{t("net.device")}</th>
                  <th className="num">{t("download")}</th>
                  <th className="num">{t("upload")}</th>
                  <th className="num">{t("net.total")}</th>
                  <th className="num">{t("col.rules")}</th>
                </tr>
              </thead>
              <tbody>
                {devices.map((d) => (
                  <tr key={d.key} tabIndex={0} onClick={() => onSelect(d.key)} onKeyDown={(e) => e.key === "Enter" && onSelect(d.key)}>
                    <td>
                      <div className="app-cell">
                        <div className="app-icon"><Smartphone /></div>
                        <div className="app-name">
                          <span className="n">{d.name}</span>
                          <span className="d">{appDescription(d, t)}</span>
                        </div>
                      </div>
                    </td>
                    <td className="num rate dl">{formatRate(d.dl, units)}</td>
                    <td className="num rate ul">{formatRate(d.ul, units)}</td>
                    <td className="num faint">{formatBytes(d.totalDl + d.totalUl)}</td>
                    <td className="num"><RuleBadges rule={config?.apps[d.key]} units={units} state={tick?.states[d.key]} /></td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <div className="card">
          <div className="card-header" style={{ paddingBottom: 4 }}>
            <h2>{t("net.adapters")}</h2>
            <div className="spacer" />
            <button className="btn ghost icon" onClick={load} title={t("net.refresh")} aria-label={t("net.refresh")}><RefreshCw /></button>
          </div>
          {sorted.map((a) => (
            <div className="adapter" key={a.if_index}>
              <div className={`ic ${a.up ? "" : "off"}`}>
                {a.is_hotspot ? <Wifi /> : a.kind === "wifi" ? <Wifi /> : a.kind === "ethernet" ? <Cable /> : a.kind === "tunnel" || a.kind === "ppp" ? <Globe /> : <Network />}
              </div>
              <div className="body">
                <div className="n">
                  {a.name}
                  {a.is_hotspot && <span className="badge green">{t("hotspot")}</span>}
                  {!a.up && <span className="badge">{t("net.disconnected")}</span>}
                </div>
                <div className="d">{a.description}</div>
                {a.addresses.length > 0 && (
                  <div className="addrs">
                    {a.addresses.map((ad) => (
                      <span className="badge mono" key={ad} style={{ fontSize: 10.5 }}>{ad}</span>
                    ))}
                  </div>
                )}
              </div>
            </div>
          ))}
          {sorted.length === 0 && <div className="empty">{t("net.noAdapters")}</div>}
        </div>
      </div>
    </div>
  );
}
