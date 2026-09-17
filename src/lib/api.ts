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

/** Schedule / quota state of one rule, keyed like `Tick.states`. */
export interface RuleState {
  /** false while outside the rule's schedule */
  active: boolean;
  quotaUsed: number;
  quotaExceeded: boolean;
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
  /** "global", "hotspot", app key, "conn:<id>", "adapter:<id>" */
  states: Record<string, RuleState>;
  metered: boolean;
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

export interface Schedule {
  enabled: boolean;
  /** Monday first */
  days: boolean[];
  /** minutes since midnight; from === to means the whole day */
  from: number;
  to: number;
}

export type QuotaPeriod = "day" | "week" | "month";

export interface Quota {
  enabled: boolean;
  bytes: number;
  period: QuotaPeriod;
  action: "block" | "notify";
}

export type Priority = "high" | "normal" | "low";

export interface Rule {
  dl: Limit;
  ul: Limit;
  blockDl: boolean;
  blockUl: boolean;
  /** only meaningful for apps/devices */
  priority: Priority;
  schedule: Schedule;
  quota: Quota;
}

export interface AppRule extends Rule {
  name: string;
  description: string;
  exe: string;
}

/** Limit/block traffic to a remote host / port, optionally for one app. */
export interface ConnRule extends Rule {
  id: string;
  name: string;
  enabled: boolean;
  /** IP, CIDR or host name; empty = any */
  host: string;
  /** "443", "6881-6889", "80,443"; empty = any */
  ports: string;
  protocol: "any" | "tcp" | "udp";
  /** app key; empty = every app */
  app: string;
}

/** Replaces the whole-PC limit for traffic on a given adapter. */
export interface AdapterRule extends Rule {
  id: string;
  enabled: boolean;
  /** "wifi" | "ethernet" | "metered" | "name:<friendly name>" */
  adapter: string;
}

export interface RuleSet {
  global: Rule;
  globalInternetOnly: boolean;
  hotspot: Rule;
  apps: Record<string, AppRule>;
  connections: ConnRule[];
  adapters: AdapterRule[];
}

export interface Profile {
  name: string;
  rules: RuleSet;
}

export interface Config {
  master: boolean;
  global: Rule;
  globalInternetOnly: boolean;
  hotspot: Rule;
  apps: Record<string, AppRule>;
  connections: ConnRule[];
  adapters: AdapterRule[];
  profiles: Profile[];
  activeProfile: string;
  historyMinutes: number;
  units: "bits" | "bytes";
  theme: "system" | "light" | "dark";
  startMinimized: boolean;
  minimizeToTray: boolean;
  closeToTray: boolean;
  checkUpdates: boolean;
  language: "system" | "es" | "en";
  onboardingDone: boolean;
  notifyNewApp: boolean;
  notifyQuota: boolean;
  notifySchedule: boolean;
}

export interface Status {
  elevated: boolean;
  driverOk: boolean;
  driverError: string | null;
  forwardOk: boolean;
  forwardError: string | null;
  windivertPath: string;
  configPath: string;
  /** packets seen by the capture threads since start */
  packets: number;
  lastError: string | null;
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

export interface StatBucket {
  /** bucket start, Unix ms */
  t: number;
  dl: number;
  ul: number;
}

export interface StatApp {
  key: string;
  name: string;
  description: string;
  exe: string;
  isDevice: boolean;
  dl: number;
  ul: number;
}

export interface Stats {
  buckets: StatBucket[];
  apps: StatApp[];
  total: StatBucket;
}

export type StatsRange = "day" | "week" | "month";

export const emptySchedule = (): Schedule => ({ enabled: false, days: [true, true, true, true, true, true, true], from: 0, to: 0 });
export const emptyQuota = (): Quota => ({ enabled: false, bytes: 1024 ** 3, period: "day", action: "block" });

export const emptyRule = (): Rule => ({
  dl: { enabled: false, rate: 1_000_000 },
  ul: { enabled: false, rate: 500_000 },
  blockDl: false,
  blockUl: false,
  priority: "normal",
  schedule: emptySchedule(),
  quota: emptyQuota(),
});

export const ruleActive = (r: Rule | undefined) =>
  !!r && (r.dl.enabled || r.ul.enabled || r.blockDl || r.blockUl || r.quota.enabled || r.priority !== "normal");

/** A limit or block is configured (ignores priority/quota). */
export const ruleLimits = (r: Rule | undefined) => !!r && (r.dl.enabled || r.ul.enabled || r.blockDl || r.blockUl);

let idCounter = 0;
export const newId = () => `${Date.now().toString(16)}${(idCounter++).toString(16)}`;

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
  stats: (range: StatsRange, key?: string | null) => invoke<Stats>("get_stats", { range, key: key ?? null }),
  switchProfile: (name: string) => invoke<Config>("switch_profile", { name }),
  /** Save dialog; resolves to the path or null when cancelled. */
  exportRules: () => invoke<string | null>("export_rules"),
  /** Open dialog; resolves to the new config or null when cancelled. */
  importRules: () => invoke<Config | null>("import_rules"),
  testNotification: () => invoke<void>("test_notification"),
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
