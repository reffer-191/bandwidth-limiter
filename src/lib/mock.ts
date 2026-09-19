/**
 * In-browser stand-in for the Rust backend, used automatically when the UI is
 * opened outside Tauri (plain `npm run dev`). Generates plausible traffic so
 * every view can be exercised without the driver.
 */
import type { Adapter, AppRate, Config, FlowView, Rule, Sample, Stats, StatsRange, Status, Tick } from "./api";

// Only type imports from ./api: that module awaits this one at top level, so
// a value import here would deadlock the module graph.
const emptyRule = (): Rule => ({
  dl: { enabled: false, rate: 0 },
  ul: { enabled: false, rate: 0 },
  blockDl: false,
  blockUl: false,
  priority: "normal",
  schedule: { enabled: false, days: [true, true, true, true, true, true, true], from: 0, to: 0 },
  quota: { enabled: false, bytes: 1024 ** 3, period: "day", action: "block" },
});

type Listener = (t: Tick) => void;

const APPS: Omit<AppRate, "dl" | "ul" | "totalDl" | "totalUl" | "lastSeen" | "online">[] = [
  { id: 0, key: "unknown", name: "Desconocido", description: "Tráfico sin proceso identificado", exe: "", pids: [], isDevice: false },
  { id: 1, key: "c:\\program files\\google\\chrome\\application\\chrome.exe", name: "chrome.exe", description: "Google Chrome", exe: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe", pids: [4120, 4188], isDevice: false },
  { id: 2, key: "c:\\users\\fer\\appdata\\roaming\\spotify\\spotify.exe", name: "Spotify.exe", description: "Spotify", exe: "C:\\Users\\Fer\\AppData\\Roaming\\Spotify\\Spotify.exe", pids: [9921], isDevice: false },
  { id: 3, key: "c:\\windows\\system32\\svchost.exe", name: "svchost.exe", description: "Host Process for Windows Services", exe: "C:\\Windows\\System32\\svchost.exe", pids: [1204, 1388, 2210], isDevice: false },
  { id: 4, key: "system", name: "System", description: "Núcleo de Windows (SMB, actualizaciones, etc.)", exe: "", pids: [4], isDevice: false },
  { id: 5, key: "hotspot:02:00:00:aa:bb:cc", name: "Phone-1", description: "192.168.137.23", exe: "", pids: [], isDevice: true },
  { id: 6, key: "c:\\program files\\qbittorrent\\qbittorrent.exe", name: "qbittorrent.exe", description: "qBittorrent", exe: "C:\\Program Files\\qBittorrent\\qbittorrent.exe", pids: [], isDevice: false },
  ...[
    ["Discord.exe", "Discord"], ["steam.exe", "Steam"], ["Teams.exe", "Microsoft Teams"], ["OneDrive.exe", "Microsoft OneDrive"],
    ["Zoom.exe", "Zoom"], ["msedge.exe", "Microsoft Edge"], ["slack.exe", "Slack"], ["Telegram.exe", "Telegram Desktop"],
    ["Code.exe", "Visual Studio Code"], ["Dropbox.exe", "Dropbox"], ["WhatsApp.exe", "WhatsApp"], ["obs64.exe", "OBS Studio"],
    ["EpicGamesLauncher.exe", "Epic Games Launcher"], ["firefox.exe", "Firefox"],
  ].map(([name, description], i) => ({ id: 7 + i, key: `c:\\apps\\${name.toLowerCase()}`, name, description, exe: `C:\\Apps\\${name}`, pids: [3000 + i], isDevice: false })),
];

const state = {
  config: {
    master: true,
    global: emptyRule(),
    globalInternetOnly: true,
    hotspot: emptyRule(),
    apps: {},
    connections: [],
    adapters: [],
    profiles: [],
    activeProfile: "",
    historyMinutes: 10,
    units: "bits",
    theme: "system",
    startMinimized: false,
    minimizeToTray: true,
    closeToTray: false,
    checkUpdates: true,
    language: "system",
    onboardingDone: !location.hash.includes("onboarding"),
    notifyNewApp: false,
    notifyQuota: true,
    notifySchedule: true,
    countHeaders: false,
  } as Config,
  history: [] as Sample[],
  totals: new Map<number, { dl: number; ul: number }>(),
  listeners: new Set<Listener>(),
  t: 0,
  autostart: false,
};

function rates(id: number, t: number) {
  const noise = () => Math.random();
  switch (id) {
    case 1: return { dl: 40_000 + 200_000 * Math.pow(noise(), 4) + (t % 90 < 6 ? 1_800_000 : 0), ul: 6_000 + 20_000 * noise() };
    case 2: return { dl: t % 40 < 10 ? 350_000 : 4_000, ul: 1_200 };
    case 3: return { dl: 3_000 * noise(), ul: 900 * noise() };
    case 4: return { dl: 1_500, ul: 400 };
    case 5: return { dl: 120_000 + 90_000 * noise(), ul: 15_000 };
    case 6: return { dl: 0, ul: 0 };
    default: return { dl: 800 * noise(), ul: 300 * noise() };
  }
}

function makeTick(): Tick {
  state.t += 1;
  const apps: AppRate[] = APPS.map((a) => {
    const r = rates(a.id, state.t);
    const tot = state.totals.get(a.id) ?? { dl: 0, ul: 0 };
    tot.dl += r.dl;
    tot.ul += r.ul;
    state.totals.set(a.id, tot);
    return { ...a, online: a.isDevice ? a.id === 5 : a.pids.length > 0, dl: r.dl, ul: r.ul, totalDl: tot.dl, totalUl: tot.ul, lastSeen: r.dl + r.ul > 0 ? Date.now() : Date.now() - 86_400_000 * (a.id + 1) };
  });
  const sum = (f: (a: AppRate) => boolean) =>
    apps.filter(f).reduce((acc, a) => ({ dl: acc.dl + a.dl, ul: acc.ul + a.ul }), { dl: 0, ul: 0 });
  const hotspot = sum((a) => a.isDevice);
  const total = sum(() => true);
  const local = sum((a) => a.id === 4);
  const internet = { dl: total.dl - hotspot.dl - local.dl, ul: total.ul - hotspot.ul - local.ul };
  return {
    ts: Date.now(),
    total,
    internet,
    local,
    hotspot,
    apps,
    queuedBytes: state.config.global.dl.enabled ? 48_000 : 0,
    dropped: 0,
    limiting: state.config.global.dl.enabled || Object.keys(state.config.apps).length > 0,
    states: ruleStates(),
    metered: false,
  };
}

/** Schedules evaluated against the browser clock; quotas against 30-day totals. */
function ruleStates(): Record<string, import("./api").RuleState> {
  const now = new Date();
  const weekday = (now.getDay() + 6) % 7;
  const minute = now.getHours() * 60 + now.getMinutes();
  const st = (r: import("./api").Rule, used: number) => {
    const s = r.schedule;
    let active = true;
    if (s.enabled) {
      if (s.from === s.to) active = s.days[weekday];
      else if (s.from < s.to) active = s.days[weekday] && minute >= s.from && minute < s.to;
      else active = (s.days[weekday] && minute >= s.from) || (s.days[(weekday + 6) % 7] && minute < s.to);
    }
    return { active, quotaUsed: r.quota.enabled ? used : 0, quotaExceeded: r.quota.enabled && used >= r.quota.bytes };
  };
  const out: Record<string, import("./api").RuleState> = {};
  const totalAll = [...state.totals.values()].reduce((a, t) => a + t.dl + t.ul, 0);
  out.global = st(state.config.global, totalAll);
  out.hotspot = st(state.config.hotspot, state.totals.get(5) ? state.totals.get(5)!.dl + state.totals.get(5)!.ul : 0);
  for (const [key, r] of Object.entries(state.config.apps)) {
    const app = APPS.find((a) => a.key === key);
    const t = app ? state.totals.get(app.id) : undefined;
    out[key] = st(r, t ? t.dl + t.ul : 0);
  }
  for (const c of state.config.connections) out[`conn:${c.id}`] = { ...st(c, 0), active: c.enabled && st(c, 0).active };
  for (const a of state.config.adapters) out[`adapter:${a.id}`] = { ...st(a, 0), active: a.enabled && st(a, 0).active };
  return out;
}

function mockStats(range: StatsRange, key?: string | null): Stats {
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const days = range === "day" ? 1 : range === "week" ? 7 : 30;
  const bucketMs = range === "day" ? 3_600_000 : 86_400_000;
  const from = range === "day" ? today : today - (days - 1) * 86_400_000;
  const n = range === "day" ? 24 : days;
  const seed = (i: number, k: number) => Math.abs(Math.sin(i * 12.9898 + k * 78.233) * 43758.5453) % 1;
  const apps = APPS.filter((a) => a.id !== 6).map((a, k) => {
    const share = a.id === 1 ? 0.45 : a.id === 2 ? 0.2 : a.id === 5 ? 0.15 : 0.02;
    const perBucket = (i: number) => {
      const active = range === "day" ? (i >= 8 && i <= 23 ? 1 : 0.1) : 1;
      // Buckets in the future (later today) stay empty.
      if (from + i * bucketMs > Date.now()) return { dl: 0, ul: 0 };
      const base = (range === "day" ? 180e6 : 3.5e9) * share * active * (0.5 + seed(i, k));
      return { dl: base, ul: base * 0.12 };
    };
    return { a, perBucket };
  });
  const buckets = Array.from({ length: n }, (_, i) => {
    const t = from + i * bucketMs;
    const sel = key ? apps.filter((x) => x.a.key === key) : apps;
    const sum = sel.reduce((acc, x) => { const b = x.perBucket(i); return { dl: acc.dl + b.dl, ul: acc.ul + b.ul }; }, { dl: 0, ul: 0 });
    return { t, dl: Math.round(sum.dl), ul: Math.round(sum.ul) };
  });
  const appTotals = apps
    .map((x) => {
      let dl = 0, ul = 0;
      for (let i = 0; i < n; i++) { const b = x.perBucket(i); dl += b.dl; ul += b.ul; }
      return { key: x.a.key, name: x.a.name, description: x.a.description, exe: x.a.exe, isDevice: x.a.isDevice, dl: Math.round(dl), ul: Math.round(ul) };
    })
    .filter((x) => x.dl + x.ul > 0)
    .sort((a, b) => b.dl + b.ul - (a.dl + a.ul));
  const total = appTotals.reduce((acc, x) => ({ dl: acc.dl + x.dl, ul: acc.ul + x.ul }), { dl: 0, ul: 0 });
  return { buckets, apps: appTotals, total: { t: from, ...total } };
}

// Pre-fill 15 minutes of history so the chart has something to show.
for (let i = 0; i < 900; i++) {
  const t = makeTick();
  t.ts = Date.now() - (900 - i) * 1000;
  state.history.push({ ts: t.ts, total: t.total, internet: t.internet, local: t.local, hotspot: t.hotspot, top: t.apps.filter((a) => a.dl + a.ul > 0).sort((a, b) => b.dl + b.ul - (a.dl + a.ul)).slice(0, 24).map((a) => [a.id, { dl: a.dl, ul: a.ul }]) });
}
setInterval(() => {
  const t = makeTick();
  state.listeners.forEach((l) => l(t));
}, 1000);

export const mockApi = {
  status: async (): Promise<Status> => ({ elevated: true, driverOk: true, driverError: null, forwardOk: true, forwardError: null, windivertPath: "(mock) WinDivert.dll", configPath: "(mock) config.json", packets: 123456, lastError: null, restarts: 0, sendErrors: 0, threads: 4, reattributed: 48213 }),
  config: async () => structuredClone(state.config),
  setConfig: async (c: Config) => {
    state.config = structuredClone(c);
    return structuredClone(c);
  },
  snapshot: async () => makeTick(),
  history: async () => state.history.slice(),
  adapters: async (): Promise<Adapter[]> => [
    { if_index: 12, ipv6_if_index: 12, name: "Wi-Fi", description: "Intel(R) Wi-Fi 6 AX201 160MHz", kind: "wifi", up: true, is_hotspot: false, addresses: ["192.168.1.34/24", "fe80::1c2a:9f0e:aa12:7b3c/64"] },
    { if_index: 20, ipv6_if_index: 20, name: "Conexión de área local* 2", description: "Microsoft Wi-Fi Direct Virtual Adapter #2", kind: "wifi", up: true, is_hotspot: true, addresses: ["192.168.137.1/24"] },
    { if_index: 5, ipv6_if_index: 5, name: "Ethernet", description: "Realtek PCIe GbE Family Controller", kind: "ethernet", up: false, is_hotspot: false, addresses: [] },
  ],
  icon: async () => null,
  flows: async (): Promise<FlowView[]> => [
    { protocol: "TCP", local: "192.168.1.34:51234", remote: "142.250.184.14:443", pid: 4120 },
    { protocol: "UDP", local: "192.168.1.34:60001", remote: "142.250.184.14:443", pid: 4188 },
  ],
  autostart: async () => state.autostart,
  setAutostart: async (enabled: boolean) => (state.autostart = enabled),
  minimizeWindow: async () => {},
  checkUpdate: async () => ({ version: "9.9.9", current: "0.5.0", notes: "Versión de prueba del mock", date: null }),
  installUpdate: async () => {},
  stats: async (range: StatsRange, key?: string | null) => mockStats(range, key),
  switchProfile: async (name: string) => {
    const c = state.config;
    const target = c.profiles.find((p) => p.name === name);
    if (target && name !== c.activeProfile) {
      const live = { global: c.global, globalInternetOnly: c.globalInternetOnly, hotspot: c.hotspot, apps: c.apps, connections: c.connections, adapters: c.adapters };
      const prev = c.profiles.find((p) => p.name === c.activeProfile);
      if (prev) prev.rules = structuredClone(live);
      Object.assign(c, structuredClone(target.rules), { activeProfile: name });
    }
    return structuredClone(c);
  },
  exportRules: async () => {
    const c = state.config;
    const blob = new Blob([JSON.stringify({ app: "Bandwidth Limiter", kind: "rules", version: 1, rules: { global: c.global, globalInternetOnly: c.globalInternetOnly, hotspot: c.hotspot, apps: c.apps, connections: c.connections, adapters: c.adapters }, profiles: c.profiles }, null, 2)], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "bandwidth-limiter-rules.json";
    a.click();
    return "bandwidth-limiter-rules.json";
  },
  importRules: async () => null,
  testNotification: async () => alert("Notificación de prueba (mock)"),
  diagnostics: async () => ({
    version: "0.8.0", windows: "10.0.26200", uptimeSecs: 4242, queuedBytes: 0, dropped: 12, limiting: false, apps: APPS.length, flowsPending: false, adapters: 3, metered: false,
    status: await mockApi.status(), configPath: "(mock) config.json", usagePath: "(mock) usage.db", logPath: "(mock) logs/app.log",
    problems: ["2026-09-18 10:12:03.120 WARN  engine: recv (forward): Error 6 (mock)", "2026-09-18 10:12:18.004 WARN  notify: could not register AUMID (mock)"],
    report: "Bandwidth Limiter 0.8.0 (mock report)\n--- log tail ---\n…",
  }),
  openLogFolder: async () => {},
  onTick: (l: Listener) => {
    state.listeners.add(l);
    return () => state.listeners.delete(l);
  },
};
