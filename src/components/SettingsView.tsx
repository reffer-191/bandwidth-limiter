import { useEffect, useState } from "react";
import { api, type UpdateInfo } from "../lib/api";
import { useEngine } from "../lib/engine";
import { Segmented, Switch } from "./ui";

const APP_VERSION = __APP_VERSION__;

export function SettingsView() {
  const { config, updateConfig, status } = useEngine();
  const [autostart, setAutostart] = useState<boolean | null>(null);
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
  const [autostartError, setAutostartError] = useState<string | null>(null);
  useEffect(() => {
    api.autostart().then(setAutostart).catch(() => setAutostart(false));
  }, []);
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
        <div className="section-title">Apariencia</div>
        <div className="card">
          <div className="settings-row">
            <div className="l">
              <span className="t">Tema</span>
              <span className="s">Sigue al sistema o fuerza claro / oscuro</span>
            </div>
            <Segmented
              value={config.theme}
              onChange={(theme) => updateConfig((c) => ({ ...c, theme }))}
              options={[
                { value: "system", label: "Sistema" },
                { value: "light", label: "Claro" },
                { value: "dark", label: "Oscuro" },
              ]}
            />
          </div>
          <div className="settings-row">
            <div className="l">
              <span className="t">Unidades</span>
              <span className="s">Cómo mostrar velocidades y límites</span>
            </div>
            <Segmented
              value={config.units}
              onChange={(units) => updateConfig((c) => ({ ...c, units }))}
              options={[
                { value: "bits", label: "Mbit/s" },
                { value: "bytes", label: "MB/s" },
              ]}
            />
          </div>
          <div className="settings-row">
            <div className="l">
              <span className="t">Ventana del historial</span>
              <span className="s">Minutos visibles en la gráfica de Actividad</span>
            </div>
            <Segmented
              value={config.historyMinutes}
              onChange={(historyMinutes) => updateConfig((c) => ({ ...c, historyMinutes }))}
              options={[1, 5, 10, 15, 30, 60].map((m) => ({ value: m, label: `${m}m` }))}
            />
          </div>
        </div>

        <div className="section-title">Inicio</div>
        <div className="card">
          <div className="settings-row">
            <div className="l">
              <span className="t">Iniciar con Windows</span>
              <span className="s">
                Crea una tarea programada al iniciar sesión con privilegios elevados (el driver los necesita).
                {autostartError && <span style={{ color: "var(--red)" }}> {autostartError}</span>}
              </span>
            </div>
            <Switch on={!!autostart} onChange={toggleAutostart} disabled={autostart === null} />
          </div>
          <div className="settings-row">
            <div className="l">
              <span className="t">Iniciar minimizado</span>
              <span className="s">La ventana arranca minimizada en la barra de tareas (siempre al iniciar con Windows)</span>
            </div>
            <Switch on={config.startMinimized} onChange={(startMinimized) => updateConfig((c) => ({ ...c, startMinimized }))} />
          </div>
        </div>

        <div className="section-title">Actualizaciones</div>
        <div className="card">
          <div className="settings-row">
            <div className="l">
              <span className="t">Buscar actualizaciones al iniciar</span>
              <span className="s">Consulta las versiones publicadas en GitHub unos segundos después de arrancar</span>
            </div>
            <Switch on={config.checkUpdates} onChange={(checkUpdates) => updateConfig((c) => ({ ...c, checkUpdates }))} />
          </div>
          <div className="settings-row">
            <div className="l">
              <span className="t">Versión instalada: {APP_VERSION}</span>
              <span className="s">
                {updateState.status === "idle" && "Las actualizaciones se descargan firmadas desde las releases del proyecto"}
                {updateState.status === "checking" && "Comprobando…"}
                {updateState.status === "none" && "Tienes la última versión"}
                {updateState.status === "available" && `Disponible la versión ${updateState.info?.version}`}
                {updateState.status === "installing" && "Descargando e instalando… la aplicación se reiniciará"}
                {updateState.status === "error" && <span style={{ color: "var(--red)" }}>{updateState.error}</span>}
              </span>
            </div>
            {updateState.status === "available" ? (
              <button className="btn primary" onClick={installNow}>Instalar y reiniciar</button>
            ) : (
              <button className="btn" onClick={checkNow} disabled={updateState.status === "checking" || updateState.status === "installing"}>Buscar ahora</button>
            )}
          </div>
        </div>

        <div className="section-title">Bandeja del sistema</div>
        <div className="card">
          <div className="settings-row">
            <div className="l">
              <span className="t">Minimizar a la bandeja</span>
              <span className="s">El botón de minimizar oculta la ventana; clic en el icono de la bandeja para volver a mostrarla</span>
            </div>
            <Switch on={config.minimizeToTray} onChange={(minimizeToTray) => updateConfig((c) => ({ ...c, minimizeToTray }))} />
          </div>
          <div className="settings-row">
            <div className="l">
              <span className="t">Cerrar a la bandeja</span>
              <span className="s">La X oculta la ventana en vez de salir; el limitador sigue activo. Salir desde el menú del icono</span>
            </div>
            <Switch on={config.closeToTray} onChange={(closeToTray) => updateConfig((c) => ({ ...c, closeToTray }))} />
          </div>
        </div>

        <div className="section-title">Motor</div>
        <div className="card">
          <div className="settings-row">
            <div className="l">
              <span className="t">Permisos de administrador</span>
              <span className="s">Necesarios para cargar el driver de captura</span>
            </div>
            <span className={`badge ${status?.elevated ? "green" : "block"}`}>{status?.elevated ? "Elevado" : "No elevado"}</span>
          </div>
          <div className="settings-row">
            <div className="l">
              <span className="t">Driver WinDivert</span>
              <span className="s mono" style={{ fontSize: 11 }}>{status?.windivertPath || "no cargado"}</span>
            </div>
            <span className={`badge ${status?.driverOk ? "green" : "block"}`}>{status?.driverOk ? "Activo" : status?.driverError ?? "Inactivo"}</span>
          </div>
          <div className="settings-row">
            <div className="l">
              <span className="t">Tráfico reenviado (hotspot)</span>
              <span className="s">Captura en la capa NETWORK_FORWARD</span>
            </div>
            <span className={`badge ${status?.forwardOk ? "green" : ""}`}>{status?.forwardOk ? "Activo" : status?.forwardError ?? "Inactivo"}</span>
          </div>
        </div>

        <div className="section-title">Datos</div>
        <div className="card">
          <div className="settings-row">
            <div className="l">
              <span className="t">Configuración</span>
              <span className="s mono" style={{ fontSize: 11 }}>{status?.configPath || "—"}</span>
            </div>
            <span className="badge">{status?.configPath?.toLowerCase().includes("appdata") ? "Instalada" : "Portable"}</span>
          </div>
        </div>

        <div className="section-title">Acerca de</div>
        <div className="card">
          <div className="settings-row">
            <div className="l">
              <span className="t">Bandwidth Limiter {APP_VERSION}</span>
              <span className="s">Limitador de ancho de banda y monitor de tráfico para Windows. Construido con Tauri, Rust y WinDivert (LGPL).</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
