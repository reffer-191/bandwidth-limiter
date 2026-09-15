//! Tiny IPv4/IPv6 + TCP/UDP header parser. Only what the shaper needs.

pub const PROTO_TCP: u8 = 6;
pub const PROTO_UDP: u8 = 17;

/// 16-byte address in wire order. IPv4 is stored IPv4-mapped (::ffff:a.b.c.d)
/// so that it compares equal with what the WinDivert FLOW layer reports.
pub type Addr16 = [u8; 16];

#[derive(Clone, Copy, Debug)]
pub struct Parsed {
    pub protocol: u8,
    pub src: Addr16,
    pub dst: Addr16,
    /// 0 when the transport is not TCP/UDP.
    pub src_port: u16,
    pub dst_port: u16,
}

#[inline]
pub fn map_ipv4(b: &[u8]) -> Addr16 {
    let mut a = [0u8; 16];
    a[10] = 0xff;
    a[11] = 0xff;
    a[12..16].copy_from_slice(&b[..4]);
    a
}

pub fn parse(pkt: &[u8]) -> Option<Parsed> {
    if pkt.is_empty() {
        return None;
    }
    let version = pkt[0] >> 4;
    match version {
        4 => {
            if pkt.len() < 20 {
                return None;
            }
            let ihl = ((pkt[0] & 0x0f) as usize) * 4;
            let protocol = pkt[9];
            let src = map_ipv4(&pkt[12..16]);
            let dst = map_ipv4(&pkt[16..20]);
            let frag_off = u16::from_be_bytes([pkt[6], pkt[7]]) & 0x1fff;
            let (sp, dp) = ports(pkt, ihl, protocol, frag_off != 0);
            Some(Parsed { protocol, src, dst, src_port: sp, dst_port: dp })
        }
        6 => {
            if pkt.len() < 40 {
                return None;
            }
            let protocol = pkt[6];
            let mut src = [0u8; 16];
            let mut dst = [0u8; 16];
            src.copy_from_slice(&pkt[8..24]);
            dst.copy_from_slice(&pkt[24..40]);
            let (sp, dp) = ports(pkt, 40, protocol, false);
            Some(Parsed { protocol, src, dst, src_port: sp, dst_port: dp })
        }
        _ => None,
    }
}

#[inline]
fn ports(pkt: &[u8], off: usize, protocol: u8, fragment: bool) -> (u16, u16) {
    if fragment || !(protocol == PROTO_TCP || protocol == PROTO_UDP) || pkt.len() < off + 4 {
        return (0, 0);
    }
    (
        u16::from_be_bytes([pkt[off], pkt[off + 1]]),
        u16::from_be_bytes([pkt[off + 2], pkt[off + 3]]),
    )
}

/// True when the address belongs to a private / link-local / multicast /
/// loopback range, i.e. traffic that never leaves the local network.
pub fn is_local(a: &Addr16) -> bool {
    if a[..10] == [0u8; 10] && a[10] == 0xff && a[11] == 0xff {
        let o = &a[12..16];
        return o[0] == 10
            || (o[0] == 172 && (16..=31).contains(&o[1]))
            || (o[0] == 192 && o[1] == 168)
            || (o[0] == 169 && o[1] == 254)
            || o[0] == 127
            || o[0] >= 224 // multicast + broadcast + reserved
            || o == [0, 0, 0, 0];
    }
    // IPv6
    a[0] == 0xfe && (a[1] & 0xc0) == 0x80 // fe80::/10 link-local
        || (a[0] & 0xfe) == 0xfc // fc00::/7 unique local
        || a[0] == 0xff // multicast
        || *a == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1] // ::1
}

pub fn addr_to_string(a: &Addr16) -> String {
    if a[..10] == [0u8; 10] && a[10] == 0xff && a[11] == 0xff {
        format!("{}.{}.{}.{}", a[12], a[13], a[14], a[15])
    } else {
        std::net::Ipv6Addr::from(*a).to_string()
    }
}
