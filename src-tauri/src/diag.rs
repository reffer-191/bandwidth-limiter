//! Rotating log file + in-memory tail for the diagnostics panel.
//!
//! `app.log` lives in `%LOCALAPPDATA%\Bandwidth Limiter\logs` (next to the
//! executable in portable mode) and rotates at 1 MB, keeping three old
//! generations. The last 300 lines are also kept in memory so Settings →
//! Diagnostics can show them and build a report without touching the disk.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use log::{Level, LevelFilter, Log, Metadata, Record};
use parking_lot::Mutex;

const MAX_BYTES: u64 = 1024 * 1024;
const GENERATIONS: usize = 3;
const TAIL_LINES: usize = 300;

struct Logger {
    file: Mutex<Option<(File, u64)>>,
    tail: Mutex<VecDeque<String>>,
    path: PathBuf,
    stderr: bool,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

pub fn log_dir() -> PathBuf {
    if let Some(dir) = crate::config::Config::portable_dir() {
        return dir.join("logs");
    }
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Bandwidth Limiter")
        .join("logs")
}

pub fn log_path() -> PathBuf {
    log_dir().join("app.log")
}

fn open(path: &PathBuf) -> Option<(File, u64)> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok()?;
    }
    let f = OpenOptions::new().create(true).append(true).open(path).ok()?;
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    Some((f, len))
}

fn rotate(path: &PathBuf) {
    // app.log.3 ← app.log.2 ← app.log.1 ← app.log
    let gen = |n: usize| path.with_extension(format!("log.{n}"));
    let _ = std::fs::remove_file(gen(GENERATIONS));
    for n in (1..GENERATIONS).rev() {
        let _ = std::fs::rename(gen(n), gen(n + 1));
    }
    let _ = std::fs::rename(path, gen(1));
}

fn timestamp() -> String {
    let ms = crate::clock::now_ms();
    let local = crate::clock::to_local(ms);
    let days = (local / crate::clock::DAY_MS) as i64;
    let (y, m, d) = crate::clock::civil_from_days(days);
    let rem = local % crate::clock::DAY_MS;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}.{:03}", rem / 3_600_000, (rem / 60_000) % 60, (rem / 1000) % 60, rem % 1000)
}

impl Log for Logger {
    fn enabled(&self, m: &Metadata) -> bool {
        m.level() <= Level::Info && (m.target().starts_with("bandwidth_limiter") || m.level() <= Level::Warn)
    }

    fn log(&self, r: &Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line = format!("{} {:<5} {}: {}", timestamp(), r.level(), r.target().rsplit("::").next().unwrap_or(""), r.args());
        if self.stderr {
            eprintln!("{line}");
        }
        {
            let mut tail = self.tail.lock();
            if tail.len() >= TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(line.clone());
        }
        let mut guard = self.file.lock();
        if guard.is_none() {
            *guard = open(&self.path);
        }
        if let Some((file, len)) = guard.as_mut() {
            if *len > MAX_BYTES {
                drop(guard.take());
                rotate(&self.path);
                *guard = open(&self.path);
                if let Some((file, len)) = guard.as_mut() {
                    let _ = writeln!(file, "{line}");
                    *len += line.len() as u64 + 2;
                }
                return;
            }
            if writeln!(file, "{line}").is_ok() {
                *len += line.len() as u64 + 2;
            }
        }
    }

    fn flush(&self) {
        if let Some((file, _)) = self.file.lock().as_mut() {
            let _ = file.flush();
        }
    }
}

/// Installs the logger (once). Debug builds also echo to stderr.
pub fn init() {
    crate::clock::refresh_offset();
    let logger = LOGGER.get_or_init(|| Logger {
        file: Mutex::new(None),
        tail: Mutex::new(VecDeque::with_capacity(TAIL_LINES)),
        path: log_path(),
        stderr: cfg!(debug_assertions),
    });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
}

/// Most recent log lines, oldest first.
pub fn tail() -> Vec<String> {
    LOGGER.get().map(|l| l.tail.lock().iter().cloned().collect()).unwrap_or_default()
}

/// Only the warnings and errors from the tail.
pub fn recent_problems(max: usize) -> Vec<String> {
    let mut v: Vec<String> = tail().into_iter().filter(|l| l.contains(" WARN ") || l.contains(" ERROR ")).collect();
    if v.len() > max {
        v.drain(..v.len() - max);
    }
    v
}
