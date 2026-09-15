//! Maps network 5-tuples to owning process ids.
//!
//! Primary source: WinDivert FLOW + SOCKET layer events (cheap, real time).
//! Fallback: the TCP/UDP owner-pid tables from iphlpapi, refreshed at most a
//! few times per second when an unknown flow shows up.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};

use super::packet::{map_ipv4, Addr16, PROTO_TCP, PROTO_UDP};

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug)]
pub struct FlowKey {
    pub protocol: u8,
    pub local: Addr16,
    pub local_port: u16,
    pub remote: Addr16,
    pub remote_port: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct FlowInfo {
    pub pid: u32,
    pub seen: Instant,
}

const REFRESH_INTERVAL: Duration = Duration::from_millis(300);
const MAX_EVENT_FLOWS: usize = 60_000;

pub struct FlowTable {
    /// Learned from FLOW/SOCKET events.
    events: HashMap<FlowKey, FlowInfo>,
    /// (protocol, local port) -> pid, learned from bind/listen events.
    local_ep: HashMap<(u8, u16), u32>,
    /// Rebuilt wholesale from iphlpapi on refresh.
    table: HashMap<FlowKey, u32>,
    table_ep: HashMap<(u8, u16), u32>,
    last_refresh: Instant,
}

impl Default for FlowTable {
    fn default() -> Self {
        Self {
            events: HashMap::new(),
            local_ep: HashMap::new(),
            table: HashMap::new(),
            table_ep: HashMap::new(),
            last_refresh: Instant::now() - REFRESH_INTERVAL,
        }
    }
}

impl FlowTable {
    pub fn insert_event(&mut self, key: FlowKey, pid: u32) {
        if self.events.len() > MAX_EVENT_FLOWS {
            // Defensive: if we ever miss DELETED events, do not grow forever.
            let cutoff = Instant::now() - Duration::from_secs(300);
            self.events.retain(|_, v| v.seen > cutoff);
        }
        self.events.insert(key, FlowInfo { pid, seen: Instant::now() });
    }

    pub fn remove_event(&mut self, key: &FlowKey) {
        self.events.remove(key);
    }

    pub fn insert_local_ep(&mut self, protocol: u8, port: u16, pid: u32) {
        if port != 0 {
            self.local_ep.insert((protocol, port), pid);
        }
    }

    pub fn remove_local_ep(&mut self, protocol: u8, port: u16) {
        self.local_ep.remove(&(protocol, port));
    }

    /// Returns the owning pid, or 0 when unknown.
    pub fn lookup(&mut self, key: &FlowKey) -> u32 {
        if let Some(f) = self.events.get_mut(key) {
            f.seen = Instant::now();
            return f.pid;
        }
        if let Some(&pid) = self.local_ep.get(&(key.protocol, key.local_port)) {
            return pid;
        }
        if let Some(pid) = self.lookup_table(key) {
            return pid;
        }
        if self.last_refresh.elapsed() >= REFRESH_INTERVAL {
            self.refresh_tables();
            if let Some(pid) = self.lookup_table(key) {
                return pid;
            }
        }
        0
    }

    fn lookup_table(&self, key: &FlowKey) -> Option<u32> {
        self.table
            .get(key)
            .or_else(|| self.table_ep.get(&(key.protocol, key.local_port)))
            .copied()
    }

    /// Active flows for a set of pids (used by the UI connection list).
    pub fn flows_for(&self, pids: &[u32]) -> Vec<(FlowKey, u32)> {
        self.events
            .iter()
            .filter(|(_, v)| pids.contains(&v.pid))
            .map(|(k, v)| (*k, v.pid))
            .collect()
    }

    pub fn refresh_tables(&mut self) {
        self.last_refresh = Instant::now();
        self.table.clear();
        self.table_ep.clear();
        unsafe {
            if let Some(buf) = get_table(true, AF_INET as u32) {
                // MIB_TCPROW_OWNER_PID: state, laddr, lport, raddr, rport, pid (6 x u32)
                for row in rows(&buf, 24) {
                    let u = |o: usize| u32::from_ne_bytes(row[o..o + 4].try_into().unwrap());
                    let state = u(0);
                    let key = FlowKey {
                        protocol: PROTO_TCP,
                        local: map_ipv4(&row[4..8]),
                        local_port: port_of(u(8)),
                        remote: map_ipv4(&row[12..16]),
                        remote_port: port_of(u(16)),
                    };
                    let pid = u(20);
                    if state == 2 {
                        self.table_ep.insert((PROTO_TCP, key.local_port), pid);
                    } else {
                        self.table.insert(key, pid);
                    }
                }
            }
            if let Some(buf) = get_table(true, AF_INET6 as u32) {
                // MIB_TCP6ROW_OWNER_PID: laddr[16], lscope, lport, raddr[16], rscope, rport, state, pid
                for row in rows(&buf, 56) {
                    let u = |o: usize| u32::from_ne_bytes(row[o..o + 4].try_into().unwrap());
                    let key = FlowKey {
                        protocol: PROTO_TCP,
                        local: row[0..16].try_into().unwrap(),
                        local_port: port_of(u(20)),
                        remote: row[24..40].try_into().unwrap(),
                        remote_port: port_of(u(44)),
                    };
                    let state = u(48);
                    let pid = u(52);
                    if state == 2 {
                        self.table_ep.insert((PROTO_TCP, key.local_port), pid);
                    } else {
                        self.table.insert(key, pid);
                    }
                }
            }
            if let Some(buf) = get_table(false, AF_INET as u32) {
                // MIB_UDPROW_OWNER_PID: laddr, lport, pid
                for row in rows(&buf, 12) {
                    let u = |o: usize| u32::from_ne_bytes(row[o..o + 4].try_into().unwrap());
                    self.table_ep.insert((PROTO_UDP, port_of(u(4))), u(8));
                }
            }
            if let Some(buf) = get_table(false, AF_INET6 as u32) {
                // MIB_UDP6ROW_OWNER_PID: laddr[16], lscope, lport, pid
                for row in rows(&buf, 28) {
                    let u = |o: usize| u32::from_ne_bytes(row[o..o + 4].try_into().unwrap());
                    self.table_ep.insert((PROTO_UDP, port_of(u(20))), u(24));
                }
            }
        }
    }
}

/// iphlpapi stores the port in network byte order inside the low 16 bits.
#[inline]
fn port_of(v: u32) -> u16 {
    (((v & 0xff) << 8) | ((v >> 8) & 0xff)) as u16
}

fn rows(buf: &[u8], row_size: usize) -> impl Iterator<Item = &[u8]> {
    let n = u32::from_ne_bytes(buf[0..4].try_into().unwrap()) as usize;
    // Rows start after the count, aligned to the row's natural alignment (4 or 8).
    let start = 4;
    (0..n)
        .map(move |i| start + i * row_size)
        .take_while(move |&off| off + row_size <= buf.len())
        .map(move |off| &buf[off..off + row_size])
}

unsafe fn get_table(tcp: bool, family: u32) -> Option<Vec<u8>> {
    let mut size: u32 = 0;
    let mut buf: Vec<u8> = Vec::new();
    for _ in 0..3 {
        let r = if tcp {
            GetExtendedTcpTable(
                buf.as_mut_ptr() as *mut _,
                &mut size,
                0,
                family,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        } else {
            GetExtendedUdpTable(
                buf.as_mut_ptr() as *mut _,
                &mut size,
                0,
                family,
                UDP_TABLE_OWNER_PID,
                0,
            )
        };
        match r {
            0 => return if buf.len() >= 4 { Some(buf) } else { None },
            122 => buf = vec![0u8; size as usize], // ERROR_INSUFFICIENT_BUFFER
            _ => return None,
        }
    }
    None
}
