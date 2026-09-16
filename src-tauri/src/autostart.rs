//! "Start with Windows" through a logon Scheduled Task.
//!
//! The app needs administrator rights, and Windows refuses to auto-start
//! elevated programs from the `Run` registry key, so the reliable way (used by
//! NetLimiter and friends) is a task with "run with highest privileges".

use std::os::windows::process::CommandExt;
use std::process::Command;

const TASK_NAME: &str = "Bandwidth Limiter";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn schtasks(args: &[&str], lang: crate::i18n::Lang) -> Result<(bool, String), String> {
    let out = Command::new("schtasks.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("{}: {e}", crate::i18n::tr(lang, "auto.run")))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string()
        + &String::from_utf8_lossy(&out.stderr);
    Ok((out.status.success(), text))
}

pub fn is_enabled() -> bool {
    schtasks(&["/Query", "/TN", TASK_NAME], crate::i18n::Lang::En).map(|(ok, _)| ok).unwrap_or(false)
}

pub fn set_enabled(enabled: bool, lang: crate::i18n::Lang) -> Result<bool, String> {
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let tr = format!("\"{}\" --autostart", exe.display());
        let (ok, text) = schtasks(&[
            "/Create", "/F", "/TN", TASK_NAME, "/TR", &tr, "/SC", "ONLOGON", "/RL", "HIGHEST", "/IT",
        ], lang)?;
        if !ok {
            return Err(format!("{}: {}", crate::i18n::tr(lang, "auto.create"), text.trim()));
        }
    } else {
        let (ok, text) = schtasks(&["/Delete", "/F", "/TN", TASK_NAME], lang)?;
        if !ok && !text.contains("ERROR: The system cannot find") && is_enabled() {
            return Err(format!("{}: {}", crate::i18n::tr(lang, "auto.delete"), text.trim()));
        }
    }
    Ok(is_enabled())
}

/// True when launched by the logon task.
pub fn launched_by_task() -> bool {
    std::env::args().any(|a| a == "--autostart")
}
