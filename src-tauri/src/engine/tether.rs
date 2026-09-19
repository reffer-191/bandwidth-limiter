//! Mobile-hotspot client list from Windows (the same data the Settings
//! page shows): MAC, addresses and, when the client announced one, its
//! name. Used to give devices a stable identity (the MAC) across DHCP
//! leases and to know which ones are connected right now.

use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct TetherClient {
    pub mac: String,
    pub ips: Vec<String>,
    pub name: Option<String>,
}

#[derive(Default, Debug)]
pub struct TetherMap {
    /// Hotspot on and the API answered.
    pub available: bool,
    pub clients: Vec<TetherClient>,
    pub ip_to_mac: HashMap<String, String>,
    pub mac_to_name: HashMap<String, String>,
}

impl TetherMap {
    pub fn from_clients(clients: Vec<TetherClient>, available: bool) -> Self {
        let mut m = TetherMap { available, ..Default::default() };
        for c in &clients {
            for ip in &c.ips {
                m.ip_to_mac.insert(ip.clone(), c.mac.clone());
            }
            if let Some(n) = &c.name {
                m.mac_to_name.insert(c.mac.clone(), n.clone());
            }
        }
        m.clients = clients;
        m
    }

    pub fn is_online(&self, ip_or_mac: &str) -> bool {
        self.ip_to_mac.contains_key(ip_or_mac) || self.clients.iter().any(|c| c.mac == ip_or_mac)
    }
}

/// Queries the tethering manager of the Internet connection profile.
/// Returns None when the hotspot is off or the API is unavailable.
pub fn query() -> Option<Vec<TetherClient>> {
    use windows::Networking::Connectivity::NetworkInformation;
    use windows::Networking::HostNameType;
    use windows::Networking::NetworkOperators::{NetworkOperatorTetheringManager, TetheringOperationalState};

    let profile = NetworkInformation::GetInternetConnectionProfile().ok()?;
    let manager = NetworkOperatorTetheringManager::CreateFromConnectionProfile(&profile).ok()?;
    if manager.TetheringOperationalState().ok()? != TetheringOperationalState::On {
        return Some(Vec::new());
    }
    let list = manager.GetTetheringClients().ok()?;
    let mut out = Vec::new();
    for c in list {
        let mac = c.MacAddress().map(|s| s.to_string().to_lowercase()).unwrap_or_default();
        if mac.is_empty() {
            continue;
        }
        let mut ips = Vec::new();
        let mut name = None;
        if let Ok(names) = c.HostNames() {
            for h in names {
                let (Ok(display), Ok(kind)) = (h.DisplayName(), h.Type()) else { continue };
                let display = display.to_string();
                match kind {
                    HostNameType::Ipv4 | HostNameType::Ipv6 => ips.push(display),
                    _ => {
                        if name.is_none() && !display.is_empty() {
                            name = Some(display.split('.').next().unwrap_or(&display).to_string());
                        }
                    }
                }
            }
        }
        out.push(TetherClient { mac, ips, name });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_does_not_panic_and_maps_clients() {
        let _ = query();
        let m = TetherMap::from_clients(vec![TetherClient { mac: "aa:bb".into(), ips: vec!["192.168.137.5".into()], name: Some("Phone".into()) }], true);
        assert!(m.is_online("192.168.137.5") && m.is_online("aa:bb") && !m.is_online("192.168.137.6"));
        assert_eq!(m.mac_to_name.get("aa:bb").map(String::as_str), Some("Phone"));
    }
}
