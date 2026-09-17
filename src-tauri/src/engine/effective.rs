//! Turns the configured rules into what must be enforced *right now*:
//! schedules decide whether a rule is in force, quotas whether it has run out.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::clock;
use crate::config::{Config, Rule};

use super::shaper::{Bucket, Limits};
use super::usage::UsageStore;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleState {
    /// False while outside the rule's schedule.
    pub active: bool,
    /// Bytes consumed in the current quota period (0 when no quota).
    pub quota_used: u64,
    pub quota_exceeded: bool,
}

/// Keys: "global", "hotspot", "<app key>", "conn:<id>", "adapter:<id>".
pub type States = BTreeMap<String, RuleState>;

fn state_of(rule: &Rule, weekday: usize, minute: u16, used: impl FnOnce(&str) -> u64) -> RuleState {
    let active = rule.schedule.is_active(weekday, minute);
    let (quota_used, quota_exceeded) = if rule.quota.enabled {
        let u = used(&rule.quota.period);
        (u, u >= rule.quota.bytes)
    } else {
        (0, false)
    };
    RuleState { active, quota_used, quota_exceeded }
}

pub fn compute(cfg: &Config, now_ms: u64, usage: &UsageStore) -> States {
    let (weekday, minute) = clock::local_weekday_minute(now_ms);
    let mut s = States::new();
    let since = |period: &str| clock::period_start(now_ms, period);
    s.insert(
        "global".into(),
        state_of(&cfg.global, weekday, minute, |p| {
            let (dl, ul) = usage.sum_all_since(since(p), |_, a| !a.is_device);
            dl + ul
        }),
    );
    s.insert(
        "hotspot".into(),
        state_of(&cfg.hotspot, weekday, minute, |p| {
            let (dl, ul) = usage.sum_all_since(since(p), |_, a| a.is_device);
            dl + ul
        }),
    );
    for (key, r) in &cfg.apps {
        s.insert(
            key.clone(),
            state_of(&r.rule, weekday, minute, |p| {
                let (dl, ul) = usage.sum_since(key, since(p));
                dl + ul
            }),
        );
    }
    for c in &cfg.connections {
        // Connection rules have no per-rule byte count; quotas are ignored.
        let mut st = state_of(&c.rule, weekday, minute, |_| 0);
        st.quota_exceeded = false;
        st.active = st.active && c.enabled;
        s.insert(format!("conn:{}", c.id), st);
    }
    for a in &cfg.adapters {
        let mut st = state_of(&a.rule, weekday, minute, |_| 0);
        st.quota_exceeded = false;
        st.active = st.active && a.enabled;
        s.insert(format!("adapter:{}", a.id), st);
    }
    s
}

/// Shaper limits for a rule in a given state.
pub fn limits_for(rule: &Rule, st: &RuleState) -> Limits {
    let priority = rule.priority_level();
    if !st.active {
        return Limits { priority, ..Default::default() };
    }
    if st.quota_exceeded && rule.quota.action == "block" {
        return Limits { block_down: true, block_up: true, priority, ..Default::default() };
    }
    Limits {
        down: rule.dl.enabled.then(|| Bucket::new(rule.dl.rate)),
        up: rule.ul.enabled.then(|| Bucket::new(rule.ul.rate)),
        block_down: rule.block_dl,
        block_up: rule.block_ul,
        priority,
    }
}

/// Parses "80", "443,8080", "6881-6889" into inclusive ranges.
pub fn parse_ports(s: &str) -> Vec<(u16, u16)> {
    let mut v = Vec::new();
    for part in s.split(|c| c == ',' || c == ';' || c == ' ') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            if let (Ok(a), Ok(b)) = (a.trim().parse::<u16>(), b.trim().parse::<u16>()) {
                v.push((a.min(b), a.max(b)));
            }
        } else if let Ok(p) = part.parse::<u16>() {
            v.push((p, p));
        }
    }
    v
}

/// "10.0.0.0/8", "1.2.3.4", "2a00::/16" → (mapped address, prefix length).
/// Host names return None (they are resolved separately).
pub fn parse_net(s: &str) -> Option<(super::packet::Addr16, u8)> {
    let (host, plen) = match s.split_once('/') {
        Some((h, p)) => (h.trim(), Some(p.trim().parse::<u8>().ok()?)),
        None => (s.trim(), None),
    };
    if let Ok(v4) = host.parse::<std::net::Ipv4Addr>() {
        return Some((super::packet::map_ipv4(&v4.octets()), plen.unwrap_or(32).min(32)));
    }
    if let Ok(v6) = host.parse::<std::net::Ipv6Addr>() {
        return Some((v6.octets(), plen.unwrap_or(128).min(128)));
    }
    None
}

pub fn is_hostname(s: &str) -> bool {
    !s.is_empty() && parse_net(s).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_and_nets() {
        assert_eq!(parse_ports("80, 443;6881-6889 22"), vec![(80, 80), (443, 443), (6881, 6889), (22, 22)]);
        assert_eq!(parse_ports("x"), vec![]);
        assert_eq!(parse_net("10.0.0.0/8").unwrap().1, 8);
        assert_eq!(parse_net("1.2.3.4").unwrap().1, 32);
        assert_eq!(parse_net("2a00::/16").unwrap().1, 16);
        assert!(parse_net("example.com").is_none());
        assert!(is_hostname("example.com"));
        assert!(!is_hostname("1.2.3.4"));
    }

    #[test]
    fn limits_follow_state() {
        let mut r = Rule::default();
        r.dl = crate::config::Limit { enabled: true, rate: 1000 };
        r.priority = "high".into();
        let l = limits_for(&r, &RuleState { active: true, quota_used: 0, quota_exceeded: false });
        assert!(l.down.is_some() && l.priority == 2);
        let l = limits_for(&r, &RuleState { active: false, quota_used: 0, quota_exceeded: false });
        assert!(l.down.is_none() && l.priority == 2);
        r.quota.enabled = true;
        let l = limits_for(&r, &RuleState { active: true, quota_used: 5, quota_exceeded: true });
        assert!(l.block_down && l.block_up);
        r.quota.action = "notify".into();
        let l = limits_for(&r, &RuleState { active: true, quota_used: 5, quota_exceeded: true });
        assert!(!l.block_down && l.down.is_some());
    }
}
