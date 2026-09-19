//! The traffic engine: WinDivert capture threads, flow→process attribution,
//! shaping scheduler and the once-per-second statistics ticker.

pub mod adapters;
pub mod effective;
pub mod flows;
pub mod packet;
pub mod procinfo;
pub mod shaper;
pub mod stats;
pub mod usage;

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex, MutexGuard, RwLock};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::clock::now_ms;
use crate::config::Config;
use crate::i18n::tr;
use crate::windivert::{self as wd, Address, Handle, WinDivert};

use adapters::AdapterInfo;
use effective::States;
use flows::{FlowKey, FlowTable, Pending};
use shaper::{ConnMatcher, Decision, Dir, Limits, QueueKey, QueuedPacket, Shaper};
use stats::{Bytes, Rate, Sample, Stats};

pub type AppId = u32;

/// WinDivert handle priority for the shaping layers (see `open_capture`).
const CAPTURE_PRIORITY: i16 = 100;


#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMeta {
    pub id: AppId,
    pub key: String,
    pub name: String,
    pub description: String,
    pub exe: String,
    pub pids: Vec<u32>,
    /// True for hotspot client pseudo-apps ("hotspot:<ip>").
    pub is_device: bool,
}

#[derive(Default)]
pub struct Apps {
    pub list: Vec<AppMeta>,
    pub by_key: HashMap<String, AppId>,
    by_pid: HashMap<u32, AppId>,
}

impl Apps {
    fn register(&mut self, key: String, name: String, description: String, exe: String, is_device: bool) -> (AppId, bool) {
        if let Some(&id) = self.by_key.get(&key) {
            return (id, false);
        }
        let id = self.list.len() as AppId;
        self.by_key.insert(key.clone(), id);
        self.list.push(AppMeta { id, key, name, description, exe, pids: Vec::new(), is_device });
        (id, true)
    }

    /// Returns the app id for a pid and whether the app was just created.
    fn app_for_pid(&mut self, pid: u32) -> (AppId, bool) {
        if let Some(&id) = self.by_pid.get(&pid) {
            return (id, false);
        }
        let info = procinfo::info(pid);
        let (id, created) = self.register(info.key, info.name, info.description, info.exe, false);
        let meta = &mut self.list[id as usize];
        if pid != 0 && !meta.pids.contains(&pid) {
            meta.pids.push(pid);
        }
        self.by_pid.insert(pid, id);
        (id, created)
    }

    /// Hotspot client row for an address, or the generic "forwarded
    /// traffic" row when the address is not a hotspot client.
    fn device(&mut self, ip: Option<&packet::Addr16>) -> (AppId, bool) {
        let ip = ip.map(packet::addr_to_string).unwrap_or_else(|| "?".to_string());
        let key = format!("hotspot:{ip}");
        if let Some(&id) = self.by_key.get(&key) {
            return (id, false);
        }
        // Description stays empty until reverse DNS gives the device a name.
        self.register(key, ip, String::new(), String::new(), true)
    }

    fn prune_dead(&mut self) {
        let dead: Vec<u32> = self
            .by_pid
            .keys()
            .copied()
            .filter(|&pid| pid != 0 && pid != 4 && !procinfo::is_alive(pid))
            .collect();
        for pid in dead {
            if let Some(id) = self.by_pid.remove(&pid) {
                self.list[id as usize].pids.retain(|&p| p != pid);
            }
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub elevated: bool,
    pub driver_ok: bool,
    pub driver_error: Option<String>,
    pub forward_ok: bool,
    pub forward_error: Option<String>,
    pub windivert_path: String,
    pub config_path: String,
    /// Packets seen by the capture threads since start (diagnostics).
    pub packets: u64,
    pub last_error: Option<String>,
    /// Capture re-opens performed by the watchdog.
    pub restarts: u32,
    /// Packets WinDivert refused to re-inject.
    pub send_errors: u64,
    /// Capture threads currently running (network, forward, flow, socket).
    pub threads: u32,
    /// Bytes first booked as "Unknown" and later moved to their app.
    pub reattributed: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRate {
    pub id: AppId,
    pub key: String,
    pub name: String,
    pub description: String,
    pub exe: String,
    pub pids: Vec<u32>,
    pub is_device: bool,
    pub dl: f64,
    pub ul: f64,
    /// Bytes over the last 30 days (persisted across restarts).
    pub total_dl: u64,
    pub total_ul: u64,
    pub last_seen: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tick {
    pub ts: u64,
    pub total: Rate,
    pub internet: Rate,
    pub local: Rate,
    pub hotspot: Rate,
    pub apps: Vec<AppRate>,
    pub queued_bytes: usize,
    pub dropped: u64,
    pub limiting: bool,
    /// Schedule / quota state per rule (see `effective::States`).
    pub states: States,
    pub metered: bool,
}

pub struct State {
    pub wd: Option<&'static WinDivert>,
    pub shaper: Mutex<Shaper>,
    pub sched_cv: Condvar,
    pub flows: Mutex<FlowTable>,
    pub apps: Mutex<Apps>,
    pub stats: Mutex<Stats>,
    pub adapters: RwLock<AdapterInfo>,
    pub config: RwLock<Config>,
    pub status: Mutex<Status>,
    pub latest: Mutex<Option<Tick>>,
    pub icons: Mutex<HashMap<String, Option<String>>>,
    pub usage: Mutex<usage::UsageStore>,
    /// Last rule states pushed into the shaper, with the config generation
    /// they were computed from.
    applied: Mutex<(u64, States)>,
    config_gen: AtomicU64,
    /// Set when something other than the config changed the effective rules
    /// (adapter list, DNS answers, a rule's app showing up).
    effective_dirty: AtomicBool,
    /// Host name → resolved addresses for connection rules.
    dns: Mutex<HashMap<String, Vec<packet::Addr16>>>,
    dns_dirty: AtomicBool,
    /// Apps first seen in this session, waiting for a notification.
    new_apps: Mutex<Vec<AppId>>,
    /// Bytes to move from "Unknown" to an app in the usage store (ticker).
    reattributed: Mutex<Vec<(AppId, u64, u64)>>,
    /// Charge IP/TCP headers to the buckets (config.count_headers).
    count_headers: AtomicBool,
    /// network, forward, flow, socket
    handles: [AtomicPtr<c_void>; 4],
    /// Bumped on every (re)open so stale threads know they are obsolete.
    open_gen: AtomicU32,
    threads: AtomicU32,
    /// A capture thread died with an error; the watchdog re-opens.
    capture_failed: AtomicBool,
    last_open: Mutex<Instant>,
    pub started: Instant,
    running: AtomicBool,
}

/// Packets per `WinDivertRecvEx` call and the buffer that holds them.
const RECV_BATCH: usize = 64;
const RECV_BUF: usize = 512 * 1024;

pub struct Engine {
    pub state: Arc<State>,
    pub app: AppHandle,
}

impl Engine {
    pub fn start(app: AppHandle) -> Engine {
        unsafe {
            // 1 ms timer resolution so the scheduler's short sleeps are accurate.
            windows_sys::Win32::Media::timeBeginPeriod(1);
        }
        let mut config = Config::load();
        config.sanitize();
        crate::clock::refresh_offset();

        let wd_result = WinDivert::get();
        let mut status = Status {
            elevated: crate::elevate::is_elevated(),
            config_path: Config::path().display().to_string(),
            ..Default::default()
        };
        let wd = match wd_result {
            Ok(w) => {
                status.windivert_path = w.path.display().to_string();
                Some(w)
            }
            Err(e) => {
                status.driver_error = Some(format!("{}: {e}", crate::i18n::tr(config.lang(), "err.dll")));
                None
            }
        };

        let mut apps = Apps::default();
        apps.register("unknown".into(), "Desconocido".into(), "Tráfico sin proceso identificado".into(), String::new(), false);
        // Apps seen in the last 30 days stay in the list even before they
        // generate traffic in this session.
        let mut usage = usage::UsageStore::load();
        usage.prune(now_ms());
        let mut known: Vec<(&String, &usage::AppUsage)> = usage.apps.iter().collect();
        known.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()));
        for (key, u) in known {
            apps.register(key.clone(), u.name.clone(), u.description.clone(), u.exe.clone(), u.is_device);
        }

        let state = Arc::new(State {
            wd,
            shaper: Mutex::new(Shaper::default()),
            sched_cv: Condvar::new(),
            flows: Mutex::new(FlowTable::default()),
            apps: Mutex::new(apps),
            stats: Mutex::new(Stats::default()),
            adapters: RwLock::new(adapters::enumerate()),
            config: RwLock::new(config.clone()),
            status: Mutex::new(status),
            latest: Mutex::new(None),
            icons: Mutex::new(HashMap::new()),
            usage: Mutex::new(usage),
            applied: Mutex::new((u64::MAX, States::new())),
            config_gen: AtomicU64::new(0),
            effective_dirty: AtomicBool::new(false),
            dns: Mutex::new(HashMap::new()),
            dns_dirty: AtomicBool::new(true),
            new_apps: Mutex::new(Vec::new()),
            reattributed: Mutex::new(Vec::new()),
            count_headers: AtomicBool::new(config.count_headers),
            handles: std::array::from_fn(|_| AtomicPtr::new(std::ptr::null_mut())),
            open_gen: AtomicU32::new(0),
            threads: AtomicU32::new(0),
            capture_failed: AtomicBool::new(false),
            last_open: Mutex::new(Instant::now()),
            started: Instant::now(),
            running: AtomicBool::new(true),
        });
        let engine = Engine { state: state.clone(), app: app.clone() };
        state.refresh_effective(&config, now_ms(), None);

        if wd.is_some() {
            state.open_capture();
        }

        {
            let s = state.clone();
            thread::Builder::new().name("bwl-sched".into()).spawn(move || scheduler_loop(s)).unwrap();
        }
        {
            let s = state.clone();
            thread::Builder::new().name("bwl-tick".into()).spawn(move || ticker_loop(s, app)).unwrap();
        }
        {
            let s = state.clone();
            thread::Builder::new().name("bwl-dns".into()).spawn(move || resolver_loop(s)).unwrap();
        }
        engine
    }

    pub fn set_config(&self, mut cfg: Config) -> Result<Config, String> {
        cfg.sanitize();
        let s = &self.state;
        s.config_gen.fetch_add(1, Ordering::SeqCst);
        s.dns_dirty.store(true, Ordering::SeqCst);
        *s.config.write() = cfg.clone();
        s.count_headers.store(cfg.count_headers, Ordering::Relaxed);
        s.refresh_effective(&cfg, now_ms(), None);
        cfg.save().map_err(|e| format!("{}: {e}", tr(cfg.lang(), "cfg.save")))?;
        // Keep every other surface (tray menu, UI) in sync.
        crate::tray::sync(&self.app, &cfg);
        if let Err(e) = self.app.emit("config", &cfg) {
            log::debug!("emit config: {e}");
        }
        Ok(cfg)
    }

    pub fn stop(&self) {
        let s = &self.state;
        if !s.running.swap(false, Ordering::SeqCst) {
            return;
        }
        if let Err(e) = s.usage.lock().save() {
            log::warn!("usage save: {e}");
        }
        if s.wd.is_some() {
            s.close_capture(true);
            // WinDivert leaves its kernel driver loaded until reboot; unload it
            // if nobody else is using it so we leave the system as we found it.
            crate::elevate::stop_driver_service("WinDivert");
        }
        s.sched_cv.notify_all();
        log::info!("engine stopped");
    }
}

impl State {
    /// Opens the WinDivert handles and starts the capture threads. Safe to
    /// call again after `close_capture` (the watchdog does).
    fn open_capture(self: &Arc<Self>) {
        let Some(w) = self.wd else { return };
        let lang = self.config.read().lang();
        let gen = self.open_gen.fetch_add(1, Ordering::SeqCst) + 1;
        *self.last_open.lock() = Instant::now();
        self.capture_failed.store(false, Ordering::SeqCst);
        let tune = |h: Handle| {
            for (param, value) in [(wd::PARAM_QUEUE_LENGTH, 16384), (wd::PARAM_QUEUE_TIME, 4000), (wd::PARAM_QUEUE_SIZE, 16 * 1024 * 1024)] {
                if let Err(e) = w.set_param(h, param, value) {
                    log::warn!("set_param {param}: {e}");
                }
            }
        };
        // Main network layer: everything except loopback. A priority above
        // the default 0 means we see packets before other WinDivert users
        // (VPN clients, an older copy of this app) and they only get what we
        // let through.
        match w.open("!loopback", wd::LAYER_NETWORK, CAPTURE_PRIORITY, 0) {
            Ok(h) => {
                tune(h);
                self.handles[0].store(h, Ordering::SeqCst);
                {
                    let mut st = self.status.lock();
                    st.driver_ok = true;
                    st.driver_error = None;
                }
                let st = self.clone();
                let hv = h as usize;
                self.threads.fetch_add(1, Ordering::SeqCst);
                thread::Builder::new().name("bwl-net".into()).spawn(move || recv_loop(st, hv as Handle, false, gen)).unwrap();
            }
            Err(e) => {
                let msg = wd::explain_open_error(&e, lang);
                log::error!("open NETWORK: {msg}");
                let mut st = self.status.lock();
                st.driver_ok = false;
                st.driver_error = Some(msg);
                return;
            }
        }
        // Forwarded packets (hotspot / ICS clients).
        match w.open("true", wd::LAYER_NETWORK_FORWARD, CAPTURE_PRIORITY, 0) {
            Ok(h) => {
                tune(h);
                self.handles[1].store(h, Ordering::SeqCst);
                {
                    let mut st = self.status.lock();
                    st.forward_ok = true;
                    st.forward_error = None;
                }
                let st = self.clone();
                let hv = h as usize;
                self.threads.fetch_add(1, Ordering::SeqCst);
                thread::Builder::new().name("bwl-fwd".into()).spawn(move || recv_loop(st, hv as Handle, true, gen)).unwrap();
            }
            Err(e) => {
                let msg = wd::explain_open_error(&e, lang);
                log::warn!("open NETWORK_FORWARD: {msg}");
                self.status.lock().forward_error = Some(msg);
            }
        }
        // Flow + socket events for process attribution.
        for (layer, name, slot) in [(wd::LAYER_FLOW, "bwl-flow", 2), (wd::LAYER_SOCKET, "bwl-sock", 3)] {
            match w.open("true", layer, 0, wd::FLAG_SNIFF | wd::FLAG_RECV_ONLY) {
                Ok(h) => {
                    self.handles[slot].store(h, Ordering::SeqCst);
                    let st = self.clone();
                    let hv = h as usize;
                    self.threads.fetch_add(1, Ordering::SeqCst);
                    thread::Builder::new().name(name.into()).spawn(move || event_loop(st, hv as Handle, layer, gen)).unwrap();
                }
                Err(e) => log::warn!("{name}: {}", wd::explain_open_error(&e, lang)),
            }
        }
        log::info!("capture open (generation {gen})");
    }

    /// Shuts the handles; the capture threads notice and exit. With
    /// `flush`, queued packets are sent first so nothing in flight is lost.
    fn close_capture(&self, flush: bool) {
        let Some(w) = self.wd else { return };
        if flush {
            let mut out = Vec::new();
            self.shaper.lock().drain_all(&mut out);
            for p in out {
                let h = self.handles[p.forward as usize].load(Ordering::SeqCst);
                if !h.is_null() && w.send(h, &p.data, &p.addr).is_err() {
                    self.status.lock().send_errors += 1;
                }
            }
        }
        for h in &self.handles {
            let h = h.swap(std::ptr::null_mut(), Ordering::SeqCst);
            if !h.is_null() {
                w.shutdown(h);
                w.close(h);
            }
        }
        {
            let mut st = self.status.lock();
            st.driver_ok = false;
            st.forward_ok = false;
        }
    }

    /// Watchdog: called by the ticker. Re-opens the capture when a thread
    /// died or the initial open failed, at most every 15 s.
    fn watchdog(self: &Arc<Self>) {
        if self.wd.is_none() {
            return;
        }
        let failed = self.capture_failed.load(Ordering::SeqCst);
        let not_open = !self.status.lock().driver_ok;
        if !(failed || not_open) {
            return;
        }
        if self.last_open.lock().elapsed() < Duration::from_secs(15) {
            return;
        }
        log::warn!("watchdog: capture {} — reopening", if failed { "thread failed" } else { "not open" });
        self.close_capture(false);
        // Give the old threads a moment to see the closed handles.
        thread::sleep(Duration::from_millis(200));
        self.status.lock().restarts += 1;
        self.open_capture();
    }

    /// A flow that was counted as "Unknown" turned out to belong to `pid`.
    fn reattribute(&self, pid: u32, pending: Pending) {
        let (app, _) = self.apps.lock().app_for_pid(pid);
        if app == 0 {
            return;
        }
        self.stats.lock().reattribute(0, app, pending.dl, pending.ul);
        self.reattributed.lock().push((app, pending.dl, pending.ul));
        self.status.lock().reattributed += pending.dl + pending.ul;
    }

    fn handle_packet(&self, handle: Handle, forward: bool, pkt: &[u8], addr: Address) {
        let w = self.wd.unwrap();
        let Some(p) = packet::parse(pkt) else {
            if w.send(handle, pkt, &addr).is_err() {
                self.status.lock().send_errors += 1;
            }
            return;
        };
        // Forwarded traffic: "download" means heading to a hotspot client.
        // Only addresses on the hotspot subnet are devices; anything else
        // the PC forwards (WSL / Hyper-V / VPN networks) is accounted as
        // ordinary traffic under one generic row and is not subject to the
        // hotspot limit.
        let mut hotspot_client = false;
        let (outbound, local, local_port, remote, remote_port) = if forward {
            let (src_dev, dst_dev) = {
                let a = self.adapters.read();
                (a.is_hotspot_client(&p.src), a.is_hotspot_client(&p.dst))
            };
            hotspot_client = src_dev || dst_dev;
            let to_client = if dst_dev && !src_dev {
                true
            } else if src_dev && !dst_dev {
                false
            } else {
                // Neither side is a hotspot client: the private/on-link side
                // is "ours".
                let a = self.adapters.read();
                let (sl, dl) = (packet::is_local(&p.src) || a.is_on_link(&p.src), packet::is_local(&p.dst) || a.is_on_link(&p.dst));
                dl && !sl
            };
            if to_client {
                (false, p.dst, p.dst_port, p.src, p.src_port)
            } else {
                (true, p.src, p.src_port, p.dst, p.dst_port)
            }
        } else if addr.outbound() {
            (true, p.src, p.src_port, p.dst, p.dst_port)
        } else {
            (false, p.dst, p.dst_port, p.src, p.src_port)
        };
        let internet = !packet::is_local(&remote);

        let len = pkt.len();
        // What the buckets and the counters see: the payload by default, the
        // whole packet when the user asked to count headers.
        let charge = if self.count_headers.load(Ordering::Relaxed) { len } else { p.payload_len(len) };
        let (app, created) = if forward {
            self.apps.lock().device(if hotspot_client { Some(&local) } else { None })
        } else {
            let key = FlowKey { protocol: p.protocol, local, local_port, remote, remote_port };
            let (pid, pending) = {
                let mut flows = self.flows.lock();
                let pid = flows.lookup(&key);
                if pid == 0 {
                    // Owner not known yet: park the bytes so they can follow
                    // the process once the socket event arrives.
                    flows.note_unknown(key, outbound, charge);
                    (0, None)
                } else {
                    (pid, flows.take_pending(&key))
                }
            };
            if let Some(pending) = pending {
                self.reattribute(pid, pending);
            }
            self.apps.lock().app_for_pid(pid)
        };
        if created {
            // Rules for this app (and connection rules restricted to it) get
            // attached by the ticker; until then it is unshaped for < 1 s.
            self.effective_dirty.store(true, Ordering::Relaxed);
            self.new_apps.lock().push(app);
        }

        let decision = {
            let mut sh = self.shaper.lock();
            let conn = if sh.matchers.is_empty() { 0 } else { sh.match_conn(app, &remote, remote_port, p.protocol) };
            let adapter = if forward || sh.if_map.is_empty() { 0 } else { sh.adapter_for(addr.if_idx()) };
            let key = QueueKey { app, dir: if outbound { Dir::Up } else { Dir::Down }, forward: hotspot_client, internet, conn, adapter };
            sh.admit(key, len, charge, || QueuedPacket { data: pkt.to_vec(), addr, forward, app, outbound, internet, charge, hotspot: hotspot_client })
        };
        // Traffic is accounted when it is delivered (here, or by the
        // scheduler for queued packets), never when it is dropped, so the
        // chart and the quotas reflect what the limiter lets through.
        match decision {
            Decision::Pass => {
                self.stats.lock().counters.record(app, outbound, charge, internet, hotspot_client);
                if w.send(handle, pkt, &addr).is_err() {
                    self.status.lock().send_errors += 1;
                }
            }
            Decision::Drop => {}
            Decision::Queued => {
                self.sched_cv.notify_one();
            }
        }
    }

    /// Recomputes what the shaper must enforce (schedules, quotas, adapter
    /// and connection rules) and pushes it when it differs from what is
    /// applied. `app` is the handle used for notifications (None at startup).
    pub fn refresh_effective(&self, cfg: &Config, now: u64, app: Option<&AppHandle>) {
        let gen = self.config_gen.load(Ordering::SeqCst);
        let forced = self.effective_dirty.swap(false, Ordering::SeqCst);
        let states = effective::compute(cfg, now, &self.usage.lock());
        {
            let applied = self.applied.lock();
            if !forced && applied.0 == gen && applied.1 == states {
                return;
            }
        }
        // Lock order everywhere is apps -> usage -> shaper (see build_tick).
        let (app_ids, conn_apps): (Vec<(AppId, Limits)>, Vec<Option<AppId>>) = {
            let apps = self.apps.lock();
            let ids = cfg
                .apps
                .iter()
                .filter_map(|(key, r)| {
                    let id = *apps.by_key.get(key)?;
                    Some((id, effective::limits_for(&r.rule, &states[key])))
                })
                .collect();
            let conn_apps = cfg
                .connections
                .iter()
                .map(|c| if c.app.is_empty() { None } else { Some(apps.by_key.get(&c.app).copied().unwrap_or(AppId::MAX)) })
                .collect();
            (ids, conn_apps)
        };
        // Connection rules: skip disabled ones; a rule scoped to an app that
        // has not shown up yet gets AppId::MAX, which never matches.
        let dns = self.dns.lock();
        let mut conns = Vec::new();
        let mut matchers = Vec::new();
        for (c, app_id) in cfg.connections.iter().zip(conn_apps) {
            if !c.enabled || conns.len() >= 250 {
                continue;
            }
            let st = &states[&format!("conn:{}", c.id)];
            let mut nets = Vec::new();
            let any_host = c.host.is_empty();
            if let Some(n) = effective::parse_net(&c.host) {
                nets.push(n);
            } else if !any_host {
                if let Some(ips) = dns.get(&c.host.to_lowercase()) {
                    nets.extend(ips.iter().map(|ip| (*ip, if packet::is_v4(ip) { 32 } else { 128 })));
                }
            }
            conns.push(effective::limits_for(&c.rule, st));
            matchers.push(ConnMatcher {
                app: app_id,
                any_host,
                nets,
                ports: effective::parse_ports(&c.ports),
                protocol: match c.protocol.as_str() {
                    "tcp" => packet::PROTO_TCP,
                    "udp" => packet::PROTO_UDP,
                    _ => 0,
                },
            });
        }
        drop(dns);
        // Adapter rules: interface index → first enabled rule that matches.
        let info = self.adapters.read();
        let mut adapter_limits = Vec::new();
        let mut if_map = HashMap::new();
        for a in cfg.adapters.iter().filter(|a| a.enabled) {
            let st = &states[&format!("adapter:{}", a.id)];
            adapter_limits.push(effective::limits_for(&a.rule, st));
            let idx = adapter_limits.len() as u8;
            for ad in &info.adapters {
                let hit = match a.adapter.as_str() {
                    "wifi" | "ethernet" => ad.kind == a.adapter,
                    "metered" => info.metered,
                    other => other.strip_prefix("name:").map(|n| n == ad.name).unwrap_or(false),
                };
                if hit {
                    if_map.entry(ad.if_index).or_insert(idx);
                    if ad.ipv6_if_index != 0 {
                        if_map.entry(ad.ipv6_if_index).or_insert(idx);
                    }
                }
            }
            if adapter_limits.len() >= 250 {
                break;
            }
        }
        drop(info);

        let mut sh = self.shaper.lock();
        sh.master = cfg.master;
        sh.global = effective::limits_for(&cfg.global, &states["global"]);
        sh.global_internet_only = cfg.global_internet_only;
        sh.hotspot = effective::limits_for(&cfg.hotspot, &states["hotspot"]);
        sh.apps = app_ids.into_iter().collect();
        sh.conns = conns;
        sh.matchers = matchers;
        sh.adapters = adapter_limits;
        sh.if_map = if_map;
        drop(sh);
        self.sched_cv.notify_one();

        // Notifications for what just changed.
        let mut applied = self.applied.lock();
        if let Some(app) = app {
            let lang = cfg.lang();
            for (key, st) in &states {
                let Some(prev) = applied.1.get(key) else { continue };
                let (label, rule) = rule_label(cfg, key, lang);
                let Some(rule) = rule else { continue };
                if cfg.notify_schedule && rule.schedule.enabled && prev.active != st.active {
                    let body = tr(lang, if st.active { "notif.schedule.on" } else { "notif.schedule.off" });
                    crate::notify::show(app, &label, body);
                }
                if cfg.notify_quota && rule.quota.enabled && !prev.quota_exceeded && st.quota_exceeded {
                    let body = tr(lang, if rule.quota.action == "block" { "notif.quota.block" } else { "notif.quota.notify" })
                        .replace("{used}", &fmt_bytes(st.quota_used))
                        .replace("{quota}", &fmt_bytes(rule.quota.bytes));
                    crate::notify::show(app, &format!("{} · {label}", tr(lang, "notif.quota")), &body);
                }
            }
        }
        *applied = (gen, states);
    }

    pub fn app_exe(&self, key: &str) -> Option<String> {
        let apps = self.apps.lock();
        apps.by_key.get(key).map(|&id| apps.list[id as usize].exe.clone()).filter(|e| !e.is_empty())
            .or_else(|| self.config.read().apps.get(key).map(|r| r.exe.clone()).filter(|e| !e.is_empty()))
    }
}

fn recv_loop(state: Arc<State>, handle: Handle, forward: bool, gen: u32) {
    let w = state.wd.unwrap();
    let mut buf = vec![0u8; RECV_BUF];
    let mut addrs = vec![Address::default(); RECV_BATCH];
    let mut n: u64 = 0;
    let mut errors: u32 = 0;
    let name = if forward { "forward" } else { "network" };
    while state.running.load(Ordering::Relaxed) && state.open_gen.load(Ordering::Relaxed) == gen {
        match w.recv_ex(handle, &mut buf, &mut addrs) {
            Ok((len, count)) => {
                errors = 0;
                // Packets sit back to back; each one's length comes from its
                // IP header (the last one takes whatever is left).
                let mut off = 0;
                for addr in addrs.iter().take(count) {
                    if off >= len {
                        break;
                    }
                    let plen = packet::parse(&buf[off..len]).map(|p| p.total_len).filter(|&l| l > 0 && off + l <= len).unwrap_or(len - off);
                    // Copy out so the parsed slice never aliases the batch
                    // buffer while the next iteration reads it.
                    let pkt = &buf[off..off + plen];
                    state.handle_packet(handle, forward, pkt, *addr);
                    off += plen;
                    n += 1;
                    if n % 64 == 0 {
                        state.status.lock().packets += 64;
                    }
                }
            }
            Err(e) => match e.raw_os_error() {
                // ERROR_NO_DATA (shutdown) / ERROR_OPERATION_ABORTED / INVALID_HANDLE
                Some(232) | Some(995) | Some(6) => break,
                _ => {
                    errors += 1;
                    if errors == 1 {
                        log::warn!("recv ({name}): {e}");
                        state.status.lock().last_error = Some(format!("recv ({name}): {e}"));
                    }
                    if errors > 200 {
                        // A second of consecutive failures: the handle is
                        // dead (BFE restarted, driver unloaded…). Let the
                        // watchdog rebuild everything.
                        log::error!("recv ({name}): giving up after {errors} errors");
                        state.capture_failed.store(true, Ordering::SeqCst);
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
            },
        }
    }
    state.threads.fetch_sub(1, Ordering::SeqCst);
    log::info!("capture thread ({name}, generation {gen}) ended");
}

fn event_loop(state: Arc<State>, handle: Handle, layer: u32, gen: u32) {
    let w = state.wd.unwrap();
    let mut addr = Address::default();
    let mut none = [0u8; 0];
    let mut errors: u32 = 0;
    let name = if layer == wd::LAYER_FLOW { "flow" } else { "socket" };
    while state.running.load(Ordering::Relaxed) && state.open_gen.load(Ordering::Relaxed) == gen {
        if let Err(e) = w.recv(handle, &mut none, &mut addr) {
            match e.raw_os_error() {
                Some(232) | Some(995) | Some(6) => break,
                _ => {
                    errors += 1;
                    if errors == 1 {
                        log::warn!("recv ({name}): {e}");
                    }
                    if errors > 200 {
                        log::error!("recv ({name}): giving up after {errors} errors");
                        state.capture_failed.store(true, Ordering::SeqCst);
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
            }
        }
        errors = 0;
        let f = addr.flow();
        let key = FlowKey {
            protocol: f.protocol,
            local: w.hton_ipv6(&f.local_addr),
            local_port: f.local_port,
            remote: w.hton_ipv6(&f.remote_addr),
            remote_port: f.remote_port,
        };
        let pending = {
            let mut flows = state.flows.lock();
            let mut learnt = false;
            if layer == wd::LAYER_FLOW {
                match addr.event() {
                    wd::EVENT_FLOW_ESTABLISHED => {
                        flows.insert_event(key, f.process_id);
                        learnt = true;
                    }
                    wd::EVENT_FLOW_DELETED => flows.remove_event(&key),
                    _ => {}
                }
            } else {
                match addr.event() {
                    wd::EVENT_SOCKET_BIND | wd::EVENT_SOCKET_LISTEN => {
                        flows.insert_local_ep(f.protocol, f.local_port, f.process_id)
                    }
                    wd::EVENT_SOCKET_CONNECT | wd::EVENT_SOCKET_ACCEPT => {
                        flows.insert_event(key, f.process_id);
                        flows.insert_local_ep(f.protocol, f.local_port, f.process_id);
                        learnt = true;
                    }
                    wd::EVENT_SOCKET_CLOSE => {
                        flows.remove_event(&key);
                        flows.remove_local_ep(f.protocol, f.local_port);
                    }
                    _ => {}
                }
            }
            if learnt && f.process_id != 0 { flows.take_pending(&key) } else { None }
        };
        if let Some(p) = pending {
            state.reattribute(f.process_id, p);
        }
    }
    state.threads.fetch_sub(1, Ordering::SeqCst);
    log::info!("event thread ({name}, generation {gen}) ended");
}

fn scheduler_loop(state: Arc<State>) {
    let Some(w) = state.wd else { return };
    let mut out: Vec<QueuedPacket> = Vec::with_capacity(512);
    let mut batch: Vec<u8> = Vec::with_capacity(RECV_BUF);
    let mut batch_addrs: Vec<Address> = Vec::with_capacity(512);
    let mut guard = state.shaper.lock();
    while state.running.load(Ordering::Relaxed) {
        let wait = guard.schedule(&mut out);
        if out.is_empty() {
            state.sched_cv.wait_for(&mut guard, wait);
        } else {
            MutexGuard::unlocked(&mut guard, || {
                {
                    let mut stats = state.stats.lock();
                    for p in out.iter() {
                        stats.counters.record(p.app, p.outbound, p.charge, p.internet, p.hotspot);
                    }
                }
                // One WinDivertSendEx per handle: the packets are copied back
                // to back into the batch buffer with one address each.
                for fwd in [false, true] {
                    let h = state.handles[fwd as usize].load(Ordering::Relaxed);
                    if h.is_null() {
                        continue;
                    }
                    batch.clear();
                    batch_addrs.clear();
                    for p in out.iter().filter(|p| p.forward == fwd) {
                        batch.extend_from_slice(&p.data);
                        batch_addrs.push(p.addr);
                    }
                    if batch_addrs.is_empty() {
                        continue;
                    }
                    let r = if batch_addrs.len() == 1 { w.send(h, &batch, &batch_addrs[0]) } else { w.send_ex(h, &batch, &batch_addrs) };
                    if let Err(e) = r {
                        let mut st = state.status.lock();
                        st.send_errors += batch_addrs.len() as u64;
                        if st.send_errors % 1000 == 1 {
                            log::warn!("send: {e}");
                        }
                    }
                }
                out.clear();
            });
        }
    }
}

fn ticker_loop(state: Arc<State>, app: AppHandle) {
    let mut last = Instant::now();
    let mut n: u64 = 0;
    let mut last_limiting = false;
    while state.running.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(1000));
        n += 1;
        let now = Instant::now();
        let secs = now.duration_since(last).as_secs_f64().max(0.001);
        last = now;

        if n % 5 == 0 {
            state.apps.lock().prune_dead();
            let mut flows = state.flows.lock();
            if flows.has_pending() {
                flows.expire_pending(now);
            }
        }
        state.watchdog();
        if n % 15 == 0 {
            let info = adapters::enumerate();
            *state.adapters.write() = info;
            state.effective_dirty.store(true, Ordering::Relaxed);
        }
        crate::clock::refresh_offset();

        let ts = now_ms();
        let (sample, per_app) = state.stats.lock().tick(ts, secs);
        {
            // Persisted 30-day usage.
            let apps = state.apps.lock();
            let mut usage = state.usage.lock();
            for (id, (_, delta)) in &per_app {
                if delta.dl + delta.ul > 0 {
                    if let Some(m) = apps.list.get(*id as usize) {
                        usage.add(&m.key, m, delta.dl, delta.ul, ts);
                    }
                }
            }
            // Bytes first booked as "Unknown" whose owner showed up.
            let moved: Vec<(AppId, u64, u64)> = std::mem::take(&mut *state.reattributed.lock());
            for (id, dl, ul) in moved {
                if let Some(m) = apps.list.get(id as usize) {
                    usage.transfer("unknown", m, dl, ul, ts);
                }
            }
            if n % 30 == 0 {
                if n % 3600 == 0 {
                    usage.prune(ts);
                }
                if let Err(e) = usage.save() {
                    log::warn!("usage save: {e}");
                }
            }
        }
        // Schedules, quotas and freshly seen apps.
        let cfg = state.config.read().clone();
        state.refresh_effective(&cfg, ts, Some(&app));
        let fresh: Vec<AppId> = std::mem::take(&mut *state.new_apps.lock());
        if cfg.notify_new_app && !fresh.is_empty() {
            let apps = state.apps.lock();
            let lang = cfg.lang();
            for id in fresh {
                if let Some(m) = apps.list.get(id as usize) {
                    if m.key == "unknown" || m.key == "system" {
                        continue;
                    }
                    let title = tr(lang, if m.is_device { "notif.newDevice" } else { "notif.newApp" });
                    let body = if m.description.is_empty() || m.is_device { m.name.clone() } else { format!("{} — {}", m.name, m.description) };
                    crate::notify::show(&app, title, &body);
                }
            }
        }

        let tick = build_tick(&state, &sample, &per_app);
        *state.latest.lock() = Some(tick.clone());
        if let Err(e) = app.emit("tick", &tick) {
            log::debug!("emit tick: {e}");
        }
        state.status.lock().threads = state.threads.load(Ordering::Relaxed);
        if tick.limiting != last_limiting {
            last_limiting = tick.limiting;
            crate::tray::set_active(&app, tick.limiting);
        }
        if n % 2 == 0 {
            let (units, lang) = {
                let c = state.config.read();
                (c.units.clone(), c.lang())
            };
            let suffix = if tick.limiting { format!(" ({})", crate::i18n::tr(lang, "tray.limiting")) } else { String::new() };
            crate::tray::set_tooltip(&app, &format!(
                "Bandwidth Limiter{suffix}\n\u{2193} {}   \u{2191} {}",
                fmt_rate(tick.total.dl, &units),
                fmt_rate(tick.total.ul, &units)
            ));
        }
    }
}

/// Human label + rule for a state key ("global", app key, "conn:id"…).
fn rule_label<'a>(cfg: &'a Config, key: &str, lang: crate::i18n::Lang) -> (String, Option<&'a crate::config::Rule>) {
    if key == "global" {
        return (tr(lang, "rule.pc").into(), Some(&cfg.global));
    }
    if key == "hotspot" {
        return (tr(lang, "rule.hotspot").into(), Some(&cfg.hotspot));
    }
    if let Some(id) = key.strip_prefix("conn:") {
        return cfg
            .connections
            .iter()
            .find(|c| c.id == id)
            .map(|c| (if c.name.is_empty() { c.host.clone() } else { c.name.clone() }, Some(&c.rule)))
            .unwrap_or((key.into(), None));
    }
    if let Some(id) = key.strip_prefix("adapter:") {
        return cfg
            .adapters
            .iter()
            .find(|a| a.id == id)
            .map(|a| (a.adapter.trim_start_matches("name:").to_string(), Some(&a.rule)))
            .unwrap_or((key.into(), None));
    }
    cfg.apps.get(key).map(|a| (a.name.clone(), Some(&a.rule))).unwrap_or((key.into(), None))
}

pub fn fmt_bytes(b: u64) -> String {
    let v = b as f64;
    if v < 1024.0 { format!("{b} B") }
    else if v < 1024.0 * 1024.0 { format!("{:.1} KB", v / 1024.0) }
    else if v < 1024.0 * 1024.0 * 1024.0 { format!("{:.1} MB", v / 1024.0 / 1024.0) }
    else { format!("{:.2} GB", v / 1024.0 / 1024.0 / 1024.0) }
}

fn fmt_rate(bytes_per_sec: f64, units: &str) -> String {
    if units == "bytes" {
        let v = bytes_per_sec;
        if v < 1024.0 { format!("{v:.0} B/s") }
        else if v < 1024.0 * 1024.0 { format!("{:.1} KB/s", v / 1024.0) }
        else { format!("{:.2} MB/s", v / 1024.0 / 1024.0) }
    } else {
        let b = bytes_per_sec * 8.0;
        if b < 1000.0 { format!("{b:.0} bit/s") }
        else if b < 1e6 { format!("{:.1} kbit/s", b / 1e3) }
        else { format!("{:.2} Mbit/s", b / 1e6) }
    }
}

fn build_tick(state: &State, sample: &Sample, per_app: &HashMap<AppId, (Rate, Bytes)>) -> Tick {
    let apps = state.apps.lock();
    let usage = state.usage.lock();
    let now = sample.ts;
    let (queued_bytes, dropped, limiting) = {
        let sh = state.shaper.lock();
        (sh.queued_bytes, sh.dropped, sh.has_any_limit())
    };
    let states = state.applied.lock().1.clone();
    let metered = state.adapters.read().metered;
    let list = apps
        .list
        .iter()
        .map(|m| {
            let r = per_app.get(&m.id).map(|(r, _)| *r).unwrap_or_default();
            let (total_dl, total_ul) = usage.totals(&m.key, now);
            AppRate {
                id: m.id,
                key: m.key.clone(),
                name: m.name.clone(),
                description: m.description.clone(),
                exe: m.exe.clone(),
                pids: m.pids.clone(),
                is_device: m.is_device,
                dl: r.dl,
                ul: r.ul,
                total_dl,
                total_ul,
                last_seen: usage.last_seen(&m.key),
            }
        })
        .collect();
    Tick {
        ts: sample.ts,
        total: sample.total,
        internet: sample.internet,
        local: sample.local,
        hotspot: sample.hotspot,
        apps: list,
        queued_bytes,
        dropped,
        limiting,
        states,
        metered,
    }
}

/// Reverse-DNS the hotspot clients seen recently so the list shows
/// "Galaxy-S23 · 192.168.137.71" instead of a bare address (the ICS DNS
/// proxy knows the names the phones sent with their DHCP request).
fn resolve_device_names(state: &Arc<State>) {
    let targets: Vec<(AppId, String)> = {
        let apps = state.apps.lock();
        apps.list
            .iter()
            .filter(|m| m.is_device && m.description.is_empty() && m.name != "?" && !m.pids.contains(&u32::MAX))
            .map(|m| (m.id, m.name.clone()))
            .collect()
    };
    for (id, ip) in targets {
        let Ok(addr) = ip.parse::<std::net::IpAddr>() else { continue };
        let name = adapters::reverse_lookup(addr);
        let mut apps = state.apps.lock();
        if let Some(m) = apps.list.get_mut(id as usize) {
            match name {
                Some(n) if n != ip => m.description = n,
                // Mark as tried (pids are meaningless for devices) so we do
                // not hammer the resolver every 5 minutes.
                _ => m.pids.push(u32::MAX),
            }
        }
    }
}

/// Resolves the host names used by connection rules (every 5 minutes and
/// whenever the rules change) so the matchers can compare addresses.
fn resolver_loop(state: Arc<State>) {
    use std::net::ToSocketAddrs;
    let mut last = Instant::now() - Duration::from_secs(3600);
    while state.running.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(1000));
        let due = last.elapsed() > Duration::from_secs(300);
        if !due && !state.dns_dirty.swap(false, Ordering::SeqCst) {
            continue;
        }
        last = Instant::now();
        resolve_device_names(&state);
        let hosts: Vec<String> = state
            .config
            .read()
            .connections
            .iter()
            .filter(|c| c.enabled && effective::is_hostname(&c.host))
            .map(|c| c.host.to_lowercase())
            .collect();
        if hosts.is_empty() {
            let mut dns = state.dns.lock();
            if !dns.is_empty() {
                dns.clear();
            }
            continue;
        }
        let mut changed = false;
        for host in hosts {
            let ips: Vec<packet::Addr16> = (host.as_str(), 0)
                .to_socket_addrs()
                .map(|it| {
                    it.map(|sa| match sa.ip() {
                        std::net::IpAddr::V4(v4) => packet::map_ipv4(&v4.octets()),
                        std::net::IpAddr::V6(v6) => v6.octets(),
                    })
                    .collect()
                })
                .unwrap_or_default();
            let mut dns = state.dns.lock();
            if ips.is_empty() {
                // Keep the previous answer on a transient failure.
                dns.entry(host).or_default();
                continue;
            }
            if dns.get(&host).map(|old| *old != ips).unwrap_or(true) {
                dns.insert(host, ips);
                changed = true;
            }
        }
        if changed {
            state.effective_dirty.store(true, Ordering::SeqCst);
        }
    }
}

