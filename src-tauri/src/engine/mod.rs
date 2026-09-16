//! The traffic engine: WinDivert capture threads, flow→process attribution,
//! shaping scheduler and the once-per-second statistics ticker.

pub mod adapters;
pub mod flows;
pub mod packet;
pub mod procinfo;
pub mod shaper;
pub mod stats;
pub mod usage;

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::{Condvar, Mutex, MutexGuard, RwLock};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::config::{Config, Rule};
use crate::windivert::{self as wd, Address, Handle, WinDivert};

use adapters::AdapterInfo;
use flows::{FlowKey, FlowTable};
use shaper::{Bucket, Decision, Dir, Limits, QueueKey, QueuedPacket, Shaper};
use stats::{Bytes, Rate, Sample, Stats};

pub type AppId = u32;


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

    fn device(&mut self, ip: &packet::Addr16) -> (AppId, bool) {
        let ip = packet::addr_to_string(ip);
        let key = format!("hotspot:{ip}");
        if let Some(&id) = self.by_key.get(&key) {
            return (id, false);
        }
        self.register(key, ip, "Dispositivo conectado al hotspot".into(), String::new(), true)
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
    /// network, forward, flow, socket
    handles: [AtomicPtr<c_void>; 4],
    running: AtomicBool,
}

pub struct Engine {
    pub state: Arc<State>,
    pub app: AppHandle,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn limits_from(rule: &Rule) -> Limits {
    Limits {
        down: rule.dl.enabled.then(|| Bucket::new(rule.dl.rate)),
        up: rule.ul.enabled.then(|| Bucket::new(rule.ul.rate)),
        block_down: rule.block_dl,
        block_up: rule.block_ul,
    }
}

impl Engine {
    pub fn start(app: AppHandle) -> Engine {
        unsafe {
            // 1 ms timer resolution so the scheduler's short sleeps are accurate.
            windows_sys::Win32::Media::timeBeginPeriod(1);
        }
        let mut config = Config::load();
        config.sanitize();

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
            handles: std::array::from_fn(|_| AtomicPtr::new(std::ptr::null_mut())),
            running: AtomicBool::new(true),
        });
        let engine = Engine { state: state.clone(), app: app.clone() };
        engine.apply_config(&config);

        if let Some(w) = wd {
            engine.open_capture(w);
        }

        {
            let s = state.clone();
            thread::Builder::new().name("bwl-sched".into()).spawn(move || scheduler_loop(s)).unwrap();
        }
        {
            let s = state.clone();
            thread::Builder::new().name("bwl-tick".into()).spawn(move || ticker_loop(s, app)).unwrap();
        }
        engine
    }

    fn open_capture(&self, w: &'static WinDivert) {
        let s = &self.state;
        let lang = s.config.read().lang();
        // Main network layer: everything except loopback.
        match w.open("!loopback", wd::LAYER_NETWORK, 0, 0) {
            Ok(h) => {
                let _ = w.set_param(h, wd::PARAM_QUEUE_LENGTH, 16384);
                let _ = w.set_param(h, wd::PARAM_QUEUE_TIME, 4000);
                let _ = w.set_param(h, wd::PARAM_QUEUE_SIZE, 16 * 1024 * 1024);
                s.handles[0].store(h, Ordering::SeqCst);
                s.status.lock().driver_ok = true;
                let st = s.clone();
                let hv = h as usize;
                thread::Builder::new().name("bwl-net".into()).spawn(move || recv_loop(st, hv as Handle, false)).unwrap();
            }
            Err(e) => {
                s.status.lock().driver_error = Some(wd::explain_open_error(&e, lang));
                return;
            }
        }
        // Forwarded packets (hotspot / ICS clients).
        match w.open("true", wd::LAYER_NETWORK_FORWARD, 0, 0) {
            Ok(h) => {
                let _ = w.set_param(h, wd::PARAM_QUEUE_LENGTH, 16384);
                let _ = w.set_param(h, wd::PARAM_QUEUE_TIME, 4000);
                let _ = w.set_param(h, wd::PARAM_QUEUE_SIZE, 16 * 1024 * 1024);
                s.handles[1].store(h, Ordering::SeqCst);
                s.status.lock().forward_ok = true;
                let st = s.clone();
                let hv = h as usize;
                thread::Builder::new().name("bwl-fwd".into()).spawn(move || recv_loop(st, hv as Handle, true)).unwrap();
            }
            Err(e) => s.status.lock().forward_error = Some(wd::explain_open_error(&e, lang)),
        }
        // Flow + socket events for process attribution.
        for (layer, name, slot) in [(wd::LAYER_FLOW, "bwl-flow", 2), (wd::LAYER_SOCKET, "bwl-sock", 3)] {
            match w.open("true", layer, 0, wd::FLAG_SNIFF | wd::FLAG_RECV_ONLY) {
                Ok(h) => {
                    s.handles[slot].store(h, Ordering::SeqCst);
                    let st = s.clone();
                    let hv = h as usize;
                    thread::Builder::new().name(name.into()).spawn(move || event_loop(st, hv as Handle, layer)).unwrap();
                }
                Err(e) => log::warn!("{name}: {}", wd::explain_open_error(&e, lang)),
            }
        }
    }

    /// Pushes the configuration into the shaper. Rules for apps that have not
    /// been seen yet are attached lazily when their first packet arrives.
    pub fn apply_config(&self, cfg: &Config) {
        let s = &self.state;
        // Lock order everywhere is apps -> shaper (see build_tick), so resolve
        // the ids first and release the apps table before touching the shaper.
        let resolved: Vec<(AppId, Limits)> = {
            let apps = s.apps.lock();
            cfg.apps
                .iter()
                .filter(|(_, r)| r.rule.is_active())
                .filter_map(|(key, r)| apps.by_key.get(key).map(|&id| (id, limits_from(&r.rule))))
                .collect()
        };
        let mut sh = s.shaper.lock();
        sh.master = cfg.master;
        sh.global = limits_from(&cfg.global);
        sh.global_internet_only = cfg.global_internet_only;
        sh.hotspot = limits_from(&cfg.hotspot);
        sh.apps = resolved.into_iter().collect();
        drop(sh);
        s.sched_cv.notify_one();
    }

    pub fn set_config(&self, mut cfg: Config) -> Result<Config, String> {
        cfg.sanitize();
        self.apply_config(&cfg);
        *self.state.config.write() = cfg.clone();
        cfg.save().map_err(|e| format!("{}: {e}", crate::i18n::tr(cfg.lang(), "cfg.save")))?;
        // Keep every other surface (tray menu, UI) in sync.
        crate::tray::sync(&self.app, cfg.master, cfg.lang());
        let _ = self.app.emit("config", &cfg);
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
        if let Some(w) = s.wd {
            // Flush queued packets so nothing in flight is lost.
            let mut out = Vec::new();
            s.shaper.lock().drain_all(&mut out);
            for p in out {
                let h = s.handles[p.forward as usize].load(Ordering::SeqCst);
                if !h.is_null() {
                    let _ = w.send(h, &p.data, &p.addr);
                }
            }
            for h in &s.handles {
                let h = h.swap(std::ptr::null_mut(), Ordering::SeqCst);
                if !h.is_null() {
                    w.shutdown(h);
                    w.close(h);
                }
            }
            // WinDivert leaves its kernel driver loaded until reboot; unload it
            // if nobody else is using it so we leave the system as we found it.
            crate::elevate::stop_driver_service("WinDivert");
        }
        s.sched_cv.notify_all();
    }
}

impl State {
    fn handle_packet(&self, handle: Handle, forward: bool, pkt: &[u8], addr: Address) {
        let w = self.wd.unwrap();
        let Some(p) = packet::parse(pkt) else {
            let _ = w.send(handle, pkt, &addr);
            return;
        };
        let (outbound, local, local_port, remote, remote_port) = if forward {
            // Forwarded traffic: "download" means heading to a hotspot client.
            let to_client = self.adapters.read().is_on_link(&p.dst);
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

        let (app, created) = if forward {
            self.apps.lock().device(&local)
        } else {
            let pid = self.flows.lock().lookup(&FlowKey {
                protocol: p.protocol,
                local,
                local_port,
                remote,
                remote_port,
            });
            self.apps.lock().app_for_pid(pid)
        };
        if created {
            self.attach_rule(app);
        }

        let len = pkt.len();
        self.stats.lock().counters.record(app, outbound, len, internet, forward);

        let key = QueueKey { app, dir: if outbound { Dir::Up } else { Dir::Down }, forward, internet };
        let decision = self.shaper.lock().admit(key, len, || QueuedPacket { data: pkt.to_vec(), addr, forward });
        match decision {
            Decision::Pass => {
                let _ = w.send(handle, pkt, &addr);
            }
            Decision::Drop => {}
            Decision::Queued => {
                self.sched_cv.notify_one();
            }
        }
    }

    /// A newly discovered app may already have a persisted rule.
    fn attach_rule(&self, app: AppId) {
        let key = self.apps.lock().list[app as usize].key.clone();
        let rule = self.config.read().apps.get(&key).map(|r| r.rule.clone());
        if let Some(rule) = rule {
            if rule.is_active() {
                self.shaper.lock().apps.insert(app, limits_from(&rule));
            }
        }
    }

    pub fn app_exe(&self, key: &str) -> Option<String> {
        let apps = self.apps.lock();
        apps.by_key.get(key).map(|&id| apps.list[id as usize].exe.clone()).filter(|e| !e.is_empty())
            .or_else(|| self.config.read().apps.get(key).map(|r| r.exe.clone()).filter(|e| !e.is_empty()))
    }
}

fn recv_loop(state: Arc<State>, handle: Handle, forward: bool) {
    let w = state.wd.unwrap();
    let mut buf = vec![0u8; wd::MTU_MAX];
    let mut addr = Address::default();
    while state.running.load(Ordering::Relaxed) {
        match w.recv(handle, &mut buf, &mut addr) {
            Ok(len) => state.handle_packet(handle, forward, &buf[..len], addr),
            Err(e) => match e.raw_os_error() {
                // ERROR_NO_DATA (shutdown) / ERROR_OPERATION_ABORTED / INVALID_HANDLE
                Some(232) | Some(995) | Some(6) => break,
                _ => {
                    log::warn!("recv: {e}");
                    thread::sleep(Duration::from_millis(5));
                }
            },
        }
    }
}

fn event_loop(state: Arc<State>, handle: Handle, layer: u32) {
    let w = state.wd.unwrap();
    let mut addr = Address::default();
    let mut none = [0u8; 0];
    while state.running.load(Ordering::Relaxed) {
        if let Err(e) = w.recv(handle, &mut none, &mut addr) {
            match e.raw_os_error() {
                Some(232) | Some(995) | Some(6) => break,
                _ => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
            }
        }
        let f = addr.flow();
        let key = FlowKey {
            protocol: f.protocol,
            local: w.hton_ipv6(&f.local_addr),
            local_port: f.local_port,
            remote: w.hton_ipv6(&f.remote_addr),
            remote_port: f.remote_port,
        };
        let mut flows = state.flows.lock();
        if layer == wd::LAYER_FLOW {
            match addr.event() {
                wd::EVENT_FLOW_ESTABLISHED => flows.insert_event(key, f.process_id),
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
                }
                wd::EVENT_SOCKET_CLOSE => {
                    flows.remove_event(&key);
                    flows.remove_local_ep(f.protocol, f.local_port);
                }
                _ => {}
            }
        }
    }
}

fn scheduler_loop(state: Arc<State>) {
    let Some(w) = state.wd else { return };
    let mut out: Vec<QueuedPacket> = Vec::with_capacity(512);
    let mut guard = state.shaper.lock();
    while state.running.load(Ordering::Relaxed) {
        let wait = guard.schedule(&mut out);
        if out.is_empty() {
            state.sched_cv.wait_for(&mut guard, wait);
        } else {
            MutexGuard::unlocked(&mut guard, || {
                for p in out.drain(..) {
                    let h = state.handles[p.forward as usize].load(Ordering::Relaxed);
                    if !h.is_null() {
                        let _ = w.send(h, &p.data, &p.addr);
                    }
                }
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
        }
        if n % 15 == 0 {
            let info = adapters::enumerate();
            *state.adapters.write() = info;
        }

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
            if n % 30 == 0 {
                if n % 3600 == 0 {
                    usage.prune(ts);
                }
                if let Err(e) = usage.save() {
                    log::warn!("usage save: {e}");
                }
            }
        }
        let tick = build_tick(&state, &sample, &per_app);
        *state.latest.lock() = Some(tick.clone());
        let _ = app.emit("tick", &tick);
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
    }
}
