//! Local-time helpers (the engine works in Unix milliseconds, UTC).

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MIN_MS: u64 = 60_000;
pub const HOUR_MS: u64 = 3_600_000;
pub const DAY_MS: u64 = 86_400_000;

static OFFSET_MS: AtomicI64 = AtomicI64::new(0);

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Re-reads the UTC → local offset from Windows (cheap; call once per tick so
/// DST changes are picked up).
pub fn refresh_offset() {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    unsafe {
        let mut tz: TIME_ZONE_INFORMATION = std::mem::zeroed();
        let r = GetTimeZoneInformation(&mut tz);
        if r == u32::MAX {
            return;
        }
        // UTC = local + Bias (minutes); 2 = daylight saving active.
        let bias = tz.Bias + if r == 2 { tz.DaylightBias } else { tz.StandardBias };
        OFFSET_MS.store(-(bias as i64) * MIN_MS as i64, Ordering::Relaxed);
    }
}

pub fn offset_ms() -> i64 {
    OFFSET_MS.load(Ordering::Relaxed)
}

/// Milliseconds of "local wall clock" for a UTC instant.
pub fn to_local(utc_ms: u64) -> u64 {
    (utc_ms as i64 + offset_ms()).max(0) as u64
}

pub fn to_utc(local_ms: u64) -> u64 {
    (local_ms as i64 - offset_ms()).max(0) as u64
}

/// Monday-first weekday (0..7) and minutes since midnight, local time.
pub fn local_weekday_minute(utc_ms: u64) -> (usize, u16) {
    let local = to_local(utc_ms);
    let day = local / DAY_MS;
    // 1970-01-01 was a Thursday (3 in Monday-first numbering).
    let weekday = ((day + 3) % 7) as usize;
    let minute = ((local % DAY_MS) / MIN_MS) as u16;
    (weekday, minute)
}

/// Civil date from days since the Unix epoch (Howard Hinnant's algorithm).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = (if m > 2 { m - 3 } else { m + 9 }) as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// UTC instant at which the local day / week (Monday) / month containing
/// `utc_ms` began.
pub fn period_start(utc_ms: u64, period: &str) -> u64 {
    let local = to_local(utc_ms);
    let day = (local / DAY_MS) as i64;
    let start_day = match period {
        "week" => day - ((day + 3) % 7),
        "month" => {
            let (y, m, _) = civil_from_days(day);
            days_from_civil(y, m, 1)
        }
        _ => day,
    };
    to_utc(start_day as u64 * DAY_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(days_from_civil(2026, 9, 17)), (2026, 9, 17));
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
    }

    #[test]
    fn periods_in_utc() {
        // 2026-09-17 is a Thursday. With offset 0 the week starts on Monday 14.
        let day = days_from_civil(2026, 9, 17) as u64;
        let now = day * DAY_MS + 5 * HOUR_MS;
        assert_eq!(period_start(now, "day"), day * DAY_MS);
        assert_eq!(period_start(now, "week"), (day - 3) * DAY_MS);
        assert_eq!(period_start(now, "month"), days_from_civil(2026, 9, 1) as u64 * DAY_MS);
        assert_eq!(local_weekday_minute(now), (3, 300));
    }
}
