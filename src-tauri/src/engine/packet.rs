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
    /// Length of the whole packet according to the IP header (used to split
    /// batched receives).
    pub total_len: usize,
    /// IP + TCP/UDP header bytes; what remains is payload.
    pub header_len: usize,
}

impl Parsed {
    /// Bytes above the transport layer.
    #[inline]
    pub fn payload_len(&self, wire_len: usize) -> usize {
        wire_len.saturating_sub(self.header_len)
    }
}

#[inline]
pub fn map_ipv4(b: &[u8]) -> Addr16 {
    let mut a = [0u8; 16];
    a[10] = 0xff;
    a[11] = 0xff;
    a[12..16].copy_from_slice(&b[..4]);
    a
}

#[inline]
pub fn is_v4(a: &Addr16) -> bool {
    a[..10] == [0u8; 10] && a[10] == 0xff && a[11] == 0xff
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
            let total_len = u16::from_be_bytes([pkt[2], pkt[3]]) as usize;
            let header_len = ihl + transport_header(pkt, ihl, protocol, frag_off != 0);
            Some(Parsed { protocol, src, dst, src_port: sp, dst_port: dp, total_len, header_len })
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
            let total_len = 40 + u16::from_be_bytes([pkt[4], pkt[5]]) as usize;
            let header_len = 40 + transport_header(pkt, 40, protocol, false);
            Some(Parsed { protocol, src, dst, src_port: sp, dst_port: dp, total_len, header_len })
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

/// TCP header (data offset) or UDP header size; 0 for anything else.
#[inline]
fn transport_header(pkt: &[u8], off: usize, protocol: u8, fragment: bool) -> usize {
    if fragment {
        return 0;
    }
    match protocol {
        PROTO_TCP if pkt.len() >= off + 13 => (((pkt[off + 12] >> 4) as usize) * 4).clamp(20, 60),
        PROTO_UDP => 8,
        _ => 0,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ipv4(proto: u8, payload: usize) -> Vec<u8> {
        let thdr = if proto == PROTO_TCP { 20 } else { 8 };
        let total = 20 + thdr + payload;
        let mut p = vec![0u8; total];
        p[0] = 0x45;
        p[2..4].copy_from_slice(&(total as u16).to_be_bytes());
        p[9] = proto;
        p[12..16].copy_from_slice(&[192, 168, 1, 2]);
        p[16..20].copy_from_slice(&[93, 184, 216, 34]);
        p[20..22].copy_from_slice(&443u16.to_be_bytes());
        p[22..24].copy_from_slice(&51000u16.to_be_bytes());
        if proto == PROTO_TCP {
            p[32] = 5 << 4; // data offset: 5 words
        }
        p
    }

    #[test]
    fn parses_ipv4_tcp_and_udp() {
        let p = parse(&ipv4(PROTO_TCP, 100)).unwrap();
        assert_eq!((p.src_port, p.dst_port), (443, 51000));
        assert_eq!(p.total_len, 140);
        assert_eq!(p.header_len, 40);
        assert_eq!(p.payload_len(140), 100);
        assert!(is_local(&p.src) && !is_local(&p.dst));
        assert_eq!(addr_to_string(&p.dst), "93.184.216.34");
        let u = parse(&ipv4(PROTO_UDP, 10)).unwrap();
        assert_eq!(u.header_len, 28);
        assert_eq!(u.total_len, 38);
    }

    #[test]
    fn parses_ipv6_and_rejects_garbage() {
        let mut p = vec![0u8; 40 + 20 + 5];
        p[0] = 0x60;
        p[4..6].copy_from_slice(&25u16.to_be_bytes());
        p[6] = PROTO_TCP;
        p[8] = 0xfe;
        p[9] = 0x80;
        p[24] = 0x2a;
        p[40..42].copy_from_slice(&80u16.to_be_bytes());
        p[42..44].copy_from_slice(&40000u16.to_be_bytes());
        p[52] = 5 << 4;
        let v6 = parse(&p).unwrap();
        assert_eq!(v6.total_len, 65);
        assert_eq!(v6.header_len, 60);
        assert!(is_local(&v6.src) && !is_local(&v6.dst));
        assert!(parse(&[]).is_none());
        assert!(parse(&[0x45; 10]).is_none());
        assert!(parse(&[0x75; 60]).is_none(), "unknown IP version");
        // Fragments carry no ports and no transport header.
        let mut f = ipv4(PROTO_TCP, 10);
        f[6] = 0x00;
        f[7] = 0x10;
        let frag = parse(&f).unwrap();
        assert_eq!((frag.src_port, frag.dst_port), (0, 0));
        assert_eq!(frag.header_len, 20);
    }
}
