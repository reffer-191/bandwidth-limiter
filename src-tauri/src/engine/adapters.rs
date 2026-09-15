//! Network adapter enumeration (GetAdaptersAddresses).


use serde::Serialize;
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
    IP_ADAPTER_ADDRESSES_LH,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC};

use super::packet::{map_ipv4, Addr16};

#[derive(Clone, Debug, Serialize)]
pub struct Adapter {
    pub if_index: u32,
    pub ipv6_if_index: u32,
    pub name: String,
    pub description: String,
    pub kind: &'static str,
    pub up: bool,
    pub is_hotspot: bool,
    pub addresses: Vec<String>,
}

#[derive(Default, Clone, Debug)]
pub struct AdapterInfo {
    pub adapters: Vec<Adapter>,
    /// Every on-link prefix of every adapter that is up.
    pub on_link: Vec<(Addr16, u8)>,
}

impl AdapterInfo {
    /// True when `addr` is directly reachable through one of our adapters.
    pub fn is_on_link(&self, addr: &Addr16) -> bool {
        self.on_link.iter().any(|(net, plen)| prefix_matches(net, addr, *plen))
    }
}

fn prefix_matches(net: &Addr16, addr: &Addr16, plen: u8) -> bool {
    let is_v4 = net[..10] == [0u8; 10] && net[10] == 0xff && net[11] == 0xff;
    let (a, b, bits) = if is_v4 {
        if !(addr[..10] == [0u8; 10] && addr[10] == 0xff && addr[11] == 0xff) {
            return false;
        }
        (&net[12..], &addr[12..], plen.min(32) as usize)
    } else {
        (&net[..], &addr[..], plen.min(128) as usize)
    };
    let full = bits / 8;
    if a[..full] != b[..full] {
        return false;
    }
    let rem = bits % 8;
    if rem == 0 {
        return true;
    }
    let mask = 0xffu8 << (8 - rem);
    (a[full] & mask) == (b[full] & mask)
}

unsafe fn pwstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    while *p.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

fn kind_of(if_type: u32) -> &'static str {
    match if_type {
        6 => "ethernet",
        23 => "ppp",
        24 => "loopback",
        71 => "wifi",
        131 => "tunnel",
        _ => "other",
    }
}

pub fn enumerate() -> AdapterInfo {
    let mut info = AdapterInfo::default();
    unsafe {
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let mut size: u32 = 32 * 1024;
        let mut buf: Vec<u8> = vec![0; size as usize];
        let mut r;
        loop {
            r = GetAdaptersAddresses(
                AF_UNSPEC as u32,
                flags,
                std::ptr::null(),
                buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                &mut size,
            );
            if r == 111 {
                // ERROR_BUFFER_OVERFLOW
                buf = vec![0; size as usize];
                continue;
            }
            break;
        }
        if r != 0 {
            return info;
        }
        let mut p = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !p.is_null() {
            let a = &*p;
            let if_index = a.Anonymous1.Anonymous.IfIndex;
            let description = pwstr(a.Description);
            let name = pwstr(a.FriendlyName);
            let up = a.OperStatus == 1;
            let mut addresses = Vec::new();
            let mut prefixes = Vec::new();
            let mut u = a.FirstUnicastAddress;
            while !u.is_null() {
                let ua = &*u;
                let sa = ua.Address.lpSockaddr as *const u8;
                if !sa.is_null() {
                    let family = u16::from_ne_bytes([*sa, *sa.add(1)]);
                    let plen = ua.OnLinkPrefixLength;
                    if family == AF_INET {
                        let b = std::slice::from_raw_parts(sa.add(4), 4);
                        let addr = map_ipv4(b);
                        addresses.push(format!("{}.{}.{}.{}/{}", b[0], b[1], b[2], b[3], plen));
                        prefixes.push((addr, plen));
                    } else if family == AF_INET6 {
                        let b = std::slice::from_raw_parts(sa.add(8), 16);
                        let addr: Addr16 = b.try_into().unwrap();
                        addresses.push(format!("{}/{}", std::net::Ipv6Addr::from(addr), plen));
                        prefixes.push((addr, plen));
                    }
                }
                u = ua.Next;
            }
            let is_hotspot = description.contains("Wi-Fi Direct Virtual Adapter")
                || addresses.iter().any(|s| s.starts_with("192.168.137.1/"));
            let kind = kind_of(a.IfType);
            if kind != "loopback" {
                if up {
                    info.on_link.extend(prefixes.iter().copied());
                }
                info.adapters.push(Adapter {
                    if_index,
                    ipv6_if_index: a.Ipv6IfIndex,
                    name,
                    description,
                    kind,
                    up,
                    is_hotspot,
                    addresses,
                });
            }
            p = a.Next as *const _;
        }
    }
    info
}
