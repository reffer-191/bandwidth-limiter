import { invoke } from "@tauri-apps/api/core";

export interface Rate {
  dl: number; // bytes/s
  ul: number;
}

export interface AppRate {
  id: number;
  key: string;
  name: string;
  description: string;
  exe: string;
  pids: number[];
  isDevice: boolean;
  dl: number;
  ul: number;
  /** bytes in the last 30 days (persisted) */
  totalDl: number;
  totalUl: number;
  lastSeen: number;
}

export interface Tick {
  ts: number;
  total: Rate;
  internet: Rate;
  local: Rate;
  hotspot: Rate;
  apps: AppRate[];
  queuedBytes: number;
  dropped: number;
  limiting: boolean;
}

export interface Sample {
  ts: number;
  total: Rate;
  internet: Rate;
  local: Rate;
  hotspot: Rate;
  top: [number, Rate][];
}

export interface Limit {
  enabled: boolean;
  rate: number; // bytes/s
}

export interface Rule {
  dl: Limit;
  ul: Limit;
  blockDl: boolean;
  blockUl: boolean;
}

export interface AppRule extends Rule {
  name: string;
  description: string;
  exe: string;
}

export interface Config {
  master: boolean;
  global: Rule;
  globalInternetOnly: boolean;
  hotspot: Rule;
  apps: Record<string, AppRule>;
  historyMinutes: number;
  units: "bits" | "bytes";
  theme: "system" | "light" | "dark";
  startMinimized: boolean;
  minimizeToTray: boolean;
  closeToTray: boolean;
  checkUpdates: boolean;
  language: "system" | "es" | "en";
  onboardingDone: boolean;
}

export interface Status {
  elevated: boolean;
  driverOk: boolean;
  driverError: string | null;
  forwardOk: boolean;
  forwardError: string | null;
  windivertPath: string;
  configPath: string;
}

export interface Adapter {
  if_index: number;
  ipv6_if_index: number;
  name: string;
  description: string;
  kind: "ethernet" | "wifi" | "ppp" | "tunnel" | "other";
  up: boolean;
  is_hotspot: boolean;
  addresses: string[];
}

export interface UpdateInfo {
  version: string;
  current: string;
  notes: string | null;
  date: string | null;
}

export interface FlowView {
  protocol: string;
  local: string;
  remote: string;
  pid: number;
}

export const emptyRule = (): Rule => ({
  dl: { enabled: false, rate: 1_000_000 },
  ul: { enabled: false, rate: 500_000 },
  blockDl: false,
  blockUl: false,
});

export const ruleActive = (r: Rule | undefined) =>
  !!r && (r.dl.enabled || r.ul.enabled || r.blockDl || r.blockUl);

export const isTauri = "__TAURI_INTERNALS__" in window;

const tauriApi = {
  status: () => invoke<Status>("get_status"),
  config: () => invoke<Config>("get_config"),
  setConfig: (config: Config) => invoke<Config>("set_config", { config }),
  snapshot: () => invoke<Tick | null>("get_snapshot"),
  history: () => invoke<Sample[]>("get_history"),
  adapters: () => invoke<Adapter[]>("get_adapters"),
  icon: (key: string) => invoke<string | null>("get_app_icon", { key }),
  flows: (key: string) => invoke<FlowView[]>("get_app_flows", { key }),
  autostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<boolean>("set_autostart", { enabled }),
  minimizeWindow: () => invoke<void>("minimize_window"),
  checkUpdate: () => invoke<UpdateInfo | null>("check_update"),
  installUpdate: () => invoke<void>("install_update"),
};

/** Outside Tauri (plain `vite` in a browser) fall back to the in-memory mock. */
export const api: typeof tauriApi = isTauri ? tauriApi : (await import("./mock")).mockApi;

export async function onConfig(cb: (c: Config) => void): Promise<() => void> {
  if (!isTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<Config>("config", (e) => cb(e.payload));
}

export async function onEvent<T>(name: string, cb: (payload: T) => void): Promise<() => void> {
  if (!isTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(name, (e) => cb(e.payload));
}

export async function onTick(cb: (t: Tick) => void): Promise<() => void> {
  if (isTauri) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<Tick>("tick", (e) => cb(e.payload));
  }
  return (await import("./mock")).mockApi.onTick(cb);
}
