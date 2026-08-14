//! Network protocol parsing: ethernet, IPv4, IPv6, UDP.
//! Replaces albion_network_lib::extract_udp_payload — the sniffer feeds raw
//! pcap packet bytes here and gets back (src_ip, dst_ip, udp_payload) or None.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const ETHERNET_HEADER_LEN: usize = 14;
const IPV4_HEADER_LEN: usize = 20;
const IPV6_HEADER_LEN: usize = 40;
const UDP_HEADER_LEN: usize = 8;

// ── Ethernet ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct EthernetHeader {
    dst_mac: [u8; 6],
    src_mac: [u8; 6],
    ether_type: u16,
}

fn parse_ethernet_header(data: &[u8]) -> Option<EthernetHeader> {
    if data.len() < ETHERNET_HEADER_LEN {
        return None;
    }
    let ether_type = u16::from_be_bytes([data[12], data[13]]);
    Some(EthernetHeader {
        dst_mac: [data[0], data[1], data[2], data[3], data[4], data[5]],
        src_mac: [data[6], data[7], data[8], data[9], data[10], data[11]],
        ether_type,
    })
}

const IPV4_TYPE: u16 = 0x0800;
const IPV6_TYPE: u16 = 0x86DD;

// ── IPv4 ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct Ipv4Header {
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    protocol: u8,
    header_len: usize,
}

fn parse_ipv4_header(data: &[u8]) -> Option<Ipv4Header> {
    if data.len() < IPV4_HEADER_LEN {
        return None;
    }
    let version_ihl = data[0];
    let version = version_ihl >> 4;
    if version != 4 {
        return None;
    }
    let ihl = (version_ihl & 0x0F) as usize * 4;
    if ihl < IPV4_HEADER_LEN || ihl > data.len() {
        return None;
    }
    let total_length = u16::from_be_bytes([data[2], data[3]]) as usize;
    if total_length < ihl || total_length > data.len() {
        return None;
    }
    let protocol = data[9];
    let src_ip = Ipv4Addr::from(u32::from_be_bytes([data[12], data[13], data[14], data[15]]));
    let dst_ip = Ipv4Addr::from(u32::from_be_bytes([data[16], data[17], data[18], data[19]]));
    Some(Ipv4Header {
        src_ip,
        dst_ip,
        protocol,
        header_len: ihl,
    })
}

// ── IPv6 ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct Ipv6Header {
    src_ip: Ipv6Addr,
    dst_ip: Ipv6Addr,
    next_header: u8,
}

fn parse_ipv6_header(data: &[u8]) -> Option<Ipv6Header> {
    if data.len() < IPV6_HEADER_LEN {
        return None;
    }
    let version = (data[0] >> 4) & 0x0F;
    if version != 6 {
        return None;
    }
    let _payload_len = u16::from_be_bytes([data[4], data[5]]);
    let next_header = data[6];
    let src_ip = Ipv6Addr::from([
        u16::from_be_bytes([data[8], data[9]]),
        u16::from_be_bytes([data[10], data[11]]),
        u16::from_be_bytes([data[12], data[13]]),
        u16::from_be_bytes([data[14], data[15]]),
        u16::from_be_bytes([data[16], data[17]]),
        u16::from_be_bytes([data[18], data[19]]),
        u16::from_be_bytes([data[20], data[21]]),
        u16::from_be_bytes([data[22], data[23]]),
    ]);
    let dst_ip = Ipv6Addr::from([
        u16::from_be_bytes([data[24], data[25]]),
        u16::from_be_bytes([data[26], data[27]]),
        u16::from_be_bytes([data[28], data[29]]),
        u16::from_be_bytes([data[30], data[31]]),
        u16::from_be_bytes([data[32], data[33]]),
        u16::from_be_bytes([data[34], data[35]]),
        u16::from_be_bytes([data[36], data[37]]),
        u16::from_be_bytes([data[38], data[39]]),
    ]);
    Some(Ipv6Header {
        src_ip,
        dst_ip,
        next_header,
    })
}

// ── UDP payload extraction ──────────────────────────────────────────────────

const UDP_PORT_ALBION_MAIN: u16 = 5056;
const UDP_PORT_ALBION_DTLS: u16 = 4535;

/// Extract UDP payload from an IPv4 packet. Returns (src_ip, dst_ip, payload).
fn extract_udp_payload_ipv4(data: &[u8]) -> Option<(IpAddr, IpAddr, &[u8])> {
    let ip = parse_ipv4_header(data)?;
    if ip.protocol != 17 {
        return None;
    }
    let ip_start = ETHERNET_HEADER_LEN;
    let ip_end = ip_start + ip.header_len;
    if ip_end + UDP_HEADER_LEN > data.len() {
        return None;
    }
    let src_ip = IpAddr::V4(ip.src_ip);
    let dst_ip = IpAddr::V4(ip.dst_ip);
    let udp_start = ip_end;
    let src_port = u16::from_be_bytes([data[udp_start], data[udp_start + 1]]);
    let dst_port = u16::from_be_bytes([data[udp_start + 2], data[udp_start + 3]]);
    if src_port != UDP_PORT_ALBION_MAIN && src_port != UDP_PORT_ALBION_DTLS
        && dst_port != UDP_PORT_ALBION_MAIN && dst_port != UDP_PORT_ALBION_DTLS
    {
        return None;
    }
    let payload_start = udp_start + UDP_HEADER_LEN;
    let payload = &data[payload_start..];
    Some((src_ip, dst_ip, payload))
}

/// Extract UDP payload from an IPv6 packet. Returns (src_ip, dst_ip, udp_payload).
fn extract_udp_payload_ipv6(data: &[u8]) -> Option<(IpAddr, IpAddr, &[u8])> {
    let ip = parse_ipv6_header(data)?;
    if ip.next_header != 17 {
        return None;
    }
    let ip_start = ETHERNET_HEADER_LEN;
    let udp_start = ip_start + IPV6_HEADER_LEN;
    if udp_start + UDP_HEADER_LEN > data.len() {
        return None;
    }
    let src_ip = IpAddr::V6(ip.src_ip);
    let dst_ip = IpAddr::V6(ip.dst_ip);
    let src_port = u16::from_be_bytes([data[udp_start], data[udp_start + 1]]);
    let dst_port = u16::from_be_bytes([data[udp_start + 2], data[udp_start + 3]]);
    if src_port != UDP_PORT_ALBION_MAIN && src_port != UDP_PORT_ALBION_DTLS
        && dst_port != UDP_PORT_ALBION_MAIN && dst_port != UDP_PORT_ALBION_DTLS
    {
        return None;
    }
    let payload_start = udp_start + UDP_HEADER_LEN;
    let payload = &data[payload_start..];
    Some((src_ip, dst_ip, payload))
}

/// Extract UDP payload from a raw ethernet frame (what pcap hands us).
/// Tries IPv4 first, then IPv6. Returns None if the frame isn't IP/UDP or
/// isn't on an Albion UDP port.
pub fn extract_udp_payload(data: &[u8]) -> Option<(IpAddr, IpAddr, &[u8])> {
    let eth = parse_ethernet_header(data)?;
    match eth.ether_type {
        h if h == IPV4_TYPE => extract_udp_payload_ipv4(&data[ETHERNET_HEADER_LEN..]),
        h if h == IPV6_TYPE => extract_udp_payload_ipv6(&data[ETHERNET_HEADER_LEN..]),
        _ => None,
    }
}

// ── CIDR matching ────────────────────────────────────────────────────────────

/// Does `ip` fall inside `cidr` ("5.188.125.0/24")?
pub fn ip_in_cidr(ip: IpAddr, cidr: &str) -> bool {
    match ip {
        IpAddr::V4(addr) => ipv4_in_cidr(addr, cidr),
        IpAddr::V6(addr) => ipv6_in_cidr(addr, cidr),
    }
}

fn ipv4_in_cidr(ip: Ipv4Addr, cidr: &str) -> bool {
    let bytes = cidr.as_bytes();
    let slash = match bytes.iter().position(|&b| b == b'/') {
        Some(p) => p,
        None => return false,
    };
    let net_str = &cidr[..slash];
    let prefix_str = &cidr[slash + 1..];
    let prefix_len: u32 = match prefix_str.parse() {
        Ok(n) if n <= 32 => n,
        _ => return false,
    };
    let parts: Vec<&str> = match net_str.split('.').collect::<Vec<_>>() {
        v if v.len() == 4 => v,
        _ => return false,
    };
    let net: u32 = match (0..4).try_fold(0u32, |acc, i| {
        let octet: u8 = match parts[i].parse() {
            Ok(v) => v,
            _ => return None,
        };
        Some(acc | ((octet as u32) << (24 - i * 8)))
    }) {
        Some(v) => v,
        None => return false,
    };
    let mask: u32 = if prefix_len == 0 {
        0
    } else {
        !0u32 << (32 - prefix_len)
    };
    (u32::from(ip) & mask) == (net & mask)
}

fn ipv6_in_cidr(ip: Ipv6Addr, cidr: &str) -> bool {
    let bytes = cidr.as_bytes();
    let slash = match bytes.iter().position(|&b| b == b'/') {
        Some(p) => p,
        None => return false,
    };
    let net_str = &cidr[..slash];
    let prefix_str = &cidr[slash + 1..];
    let prefix_len: u32 = match prefix_str.parse() {
        Ok(n) if n <= 128 => n,
        _ => return false,
    };
    let parts: Vec<&str> = match net_str.split(':').collect::<Vec<_>>() {
        v if v.len() == 8 => v,
        _ => return false,
    };
    let net_u128: u128 = match (0..8).try_fold(0u128, |acc, i| {
        let part: u16 = match parts[i].parse() {
            Ok(v) => v,
            _ => return None,
        };
        Some(acc | ((part as u128) << (112 - i * 16)))
    }) {
        Some(v) => v,
        None => return false,
    };
    let mask: u128 = if prefix_len == 0 {
        0
    } else {
        !0u128 << (128 - prefix_len)
    };
    let ip_u128 = u128::from(ip);
    (ip_u128 & mask) == (net_u128 & mask)
}
