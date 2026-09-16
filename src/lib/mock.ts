/**
 * In-browser stand-in for the Rust backend, used automatically when the UI is
 * opened outside Tauri (plain `npm run dev`). Generates plausible traffic so
 * every view can be exercised without the driver.
 */
import type { Adapter, AppRate, Config, FlowView, Sample, Status, Tick } from "./api";

type Listener = (t: Tick) => void;

const APPS: Omit<AppRate, "dl" | "ul" | "totalDl" | "totalUl" | "lastSeen">[] = [
  { id: 0, key: "unknown", name: "Desconocido", description: "Tráfico sin proceso identificado", exe: "", pids: [], isDevice: false },
  { id: 1, key: "c:\\program files\\google\\chrome\\application\\chrome.exe", name: "chrome.exe", description: "Google Chrome", exe: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe", pids: [4120, 4188], isDevice: false },
  { id: 2, key: "c:\\users\\fer\\appdata\\roaming\\spotify\\spotify.exe", name: "Spotify.exe", description: "Spotify", exe: "C:\\Users\\Fer\\AppData\\Roaming\\Spotify\\Spotify.exe", pids: [9921], isDevice: false },
  { id: 3, key: "c:\\windows\\system32\\svchost.exe", name: "svchost.exe", description: "Host Process for Windows Services", exe: "C:\\Windows\\System32\\svchost.exe", pids: [1204, 1388, 2210], isDevice: false },
  { id: 4, key: "system", name: "System", description: "Núcleo de Windows (SMB, actualizaciones, etc.)", exe: "", pids: [4], isDevice: false },
  { id: 5, key: "hotspot:192.168.137.23", name: "192.168.137.23", description: "Dispositivo conectado al hotspot", exe: "", pids: [], isDevice: true },
  { id: 6, key: "c:\\program files\\qbittorrent\\qbittorrent.exe", name: "qbittorrent.exe", description: "qBittorrent", exe: "C:\\Program Files\\qBittorrent\\qbittorrent.exe", pids: [], isDevice: false },
  ...[
    ["Discord.exe", "Discord"], ["steam.exe", "Steam"], ["Teams.exe", "Microsoft Teams"], ["OneDrive.exe", "Microsoft OneDrive"],
    ["Zoom.exe", "Zoom"], ["msedge.exe", "Microsoft Edge"], ["slack.exe", "Slack"], ["Telegram.exe", "Telegram Desktop"],
    ["Code.exe", "Visual Studio Code"], ["Dropbox.exe", "Dropbox"], ["WhatsApp.exe", "WhatsApp"], ["obs64.exe", "OBS Studio"],
    ["EpicGamesLauncher.exe", "Epic Games Launcher"], ["firefox.exe", "Firefox"],
  ].map(([name, description], i) => ({ id: 7 + i, key: `c:\apps\${name.toLowerCase()}`, name, description, exe: `C:\Apps\${name}`, pids: [3000 + i], isDevice: false })),
];

const state = {
  config: {
    master: true,
    global: { dl: { enabled: false, rate: 0 }, ul: { enabled: false, rate: 0 }, blockDl: false, blockUl: false },
    globalInternetOnly: true,
    hotspot: { dl: { enabled: false, rate: 0 }, ul: { enabled: false, rate: 0 }, blockDl: false, blockUl: false },
    apps: {},
    historyMinutes: 10,
    units: "bits",
    theme: "system",
    startMinimized: false,
    minimizeToTray: true,
    closeToTray: false,
    checkUpdates: true,
    language: "system",
    onboardingDone: !location.hash.includes("onboarding"),
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
    return { ...a, dl: r.dl, ul: r.ul, totalDl: tot.dl, totalUl: tot.ul, lastSeen: r.dl + r.ul > 0 ? Date.now() : Date.now() - 86_400_000 * (a.id + 1) };
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
  };
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
  status: async (): Promise<Status> => ({ elevated: true, driverOk: true, driverError: null, forwardOk: true, forwardError: null, windivertPath: "(mock) WinDivert.dll", configPath: "(mock) config.json" }),
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
  onTick: (l: Listener) => {
    state.listeners.add(l);
    return () => state.listeners.delete(l);
  },
};
