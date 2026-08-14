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

/// Extract UDP payload from an IPv4 packet. Returns (src_ip, dst_ip, payload).
fn extract_udp_payload_ipv4(data: &[u8]) -> Option<(IpAddr, IpAddr, &[u8])> {
    let ip = parse_ipv4_header(data)?;
    if ip.protocol != 17 {
        return None;
    }
    let ip_end = ip.header_len;
    if ip_end + UDP_HEADER_LEN > data.len() {
        return None;
    }
    let src_ip = IpAddr::V4(ip.src_ip);
    let dst_ip = IpAddr::V4(ip.dst_ip);
    let udp_start = ip_end;
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
    let udp_start = IPV6_HEADER_LEN;
    if udp_start + UDP_HEADER_LEN > data.len() {
        return None;
    }
    let src_ip = IpAddr::V6(ip.src_ip);
    let dst_ip = IpAddr::V6(ip.dst_ip);
    let payload_start = udp_start + UDP_HEADER_LEN;
    let payload = &data[payload_start..];
    Some((src_ip, dst_ip, payload))
}

/// Extract UDP payload from a raw ethernet frame (what pcap hands us).
/// Tries IPv4 first, then IPv6. Returns None if the frame isn't IP/UDP.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a full ethernet+IPv4+UDP frame wrapping `payload` as the UDP body.
    /// Mirrors what pcap hands to extract_udp_payload on the wire.
    fn wrap_udp_frame(payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::new();
        frame.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]); // dst mac
        frame.extend_from_slice(&[0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb]); // src mac
        frame.extend_from_slice(&[0x08, 0x00]); // IPv4 ethertype
        // IPv4 header, 20 bytes, no options
        frame.push(0x45); // version 4, ihl 5
        frame.push(0x00); // dscp/ecn
        let total_len = 20 + 8 + payload.len();
        frame.extend_from_slice(&(total_len as u16).to_be_bytes());
        frame.extend_from_slice(&[0x00, 0x01]); // id
        frame.extend_from_slice(&[0x40, 0x00]); // flags/frag
        frame.push(64); // ttl
        frame.push(17); // udp
        frame.extend_from_slice(&[0x00, 0x00]); // checksum (ignored by parser)
        frame.extend_from_slice(&[5, 188, 125, 53]); // src ip
        frame.extend_from_slice(&[10, 208, 46, 229]); // dst ip
        // UDP header
        frame.extend_from_slice(&[0x13, 0xc0]); // src port 5056
        frame.extend_from_slice(&[0xa4, 0xf8]); // dst port
        frame.extend_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
        frame.extend_from_slice(&[0x00, 0x00]); // checksum
        frame.extend_from_slice(payload);
        frame
    }

    #[test]
    fn extracts_udp_payload_at_photon_offset() {
        // Real ground-truth payload from tcpdump: Photon envelope (12B) + one
        // SendUnreliable(07) command (12B) + unreliable seq (4B) + MESSAGE_EVENT.
        // command_count=3 lives at byte 3, so a +14 offset bug would read 0.
        let payload: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x03, // peer_id, flags, command_count = 3
            0xd2, 0xe7, 0x5d, 0x32, // timestamp
            0x68, 0xe4, 0xd5, 0xa1, // challenge
            0x07, 0x00, 0x00, 0x00, // SendUnreliable, channel, flags, reserved
            0x00, 0x00, 0x00, 0x39, // command length = 57
            0x00, 0x00, 0x14, 0x13, // sequence
            0x00, 0x00, 0x29, 0x81, // unreliable seq
            0xf3, 0x04, // MESSAGE_EVENT
        ];
        let frame = wrap_udp_frame(&payload);
        let (src, dst, extracted) = extract_udp_payload(&frame).expect("extract");
        assert_eq!(src, IpAddr::V4(Ipv4Addr::new(5, 188, 125, 53)));
        assert_eq!(dst, IpAddr::V4(Ipv4Addr::new(10, 208, 46, 229)));
        assert_eq!(extracted, payload.as_slice());
        assert_eq!(extracted[3], 0x03, "command_count must be at byte 3");
        assert_eq!(&extracted[28..30], &[0xf3, 0x04], "message must start at byte 28");
    }

    #[test]
    fn rejects_non_udp_frames() {
        let mut frame = wrap_udp_frame(&[1, 2, 3]);
        frame[23] = 6; // tcp instead of udp
        assert!(extract_udp_payload(&frame).is_none());
    }
}
