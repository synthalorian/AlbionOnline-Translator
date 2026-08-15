//! CIDR-based host filtering for Albion Online server IPs.
//! Ported from albion-network-lib-ref src/capture/hosts.rs.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

#[derive(Clone, Debug)]
enum CidrRange {
    V4 { network: u32, mask: u32 },
    V6 { network: u128, mask: u128 },
}

impl CidrRange {
    fn contains(&self, ip: IpAddr) -> bool {
        match (self, ip) {
            (Self::V4 { network, mask }, IpAddr::V4(ip)) => (u32::from(ip) & mask) == *network,
            (Self::V6 { network, mask }, IpAddr::V6(ip)) => (u128::from(ip) & mask) == *network,
            _ => false,
        }
    }
}

impl FromStr for CidrRange {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let (address, prefix) = value
            .split_once('/')
            .ok_or_else(|| "missing /prefix".to_string())?;
        let ip = address.parse::<IpAddr>().map_err(|e| format!("invalid IP address: {e}"))?;
        let prefix = prefix.parse::<u8>().map_err(|e| format!("invalid prefix length: {e}"))?;

        match ip {
            IpAddr::V4(ip) => {
                if prefix > 32 {
                    return Err("IPv4 prefix length must be <= 32".to_string());
                }
                let mask = prefix_mask_u32(prefix);
                Ok(Self::V4 {
                    network: u32::from(ip) & mask,
                    mask,
                })
            }
            IpAddr::V6(ip) => {
                if prefix > 128 {
                    return Err("IPv6 prefix length must be <= 128".to_string());
                }
                let mask = prefix_mask_u128(prefix);
                Ok(Self::V6 {
                    network: u128::from(ip) & mask,
                    mask,
                })
            }
        }
    }
}

/// Filters UDP packets by source/destination IP CIDR ranges.
/// Load from a hosts.txt file (one CIDR per line, # comments allowed)
/// or construct programmatically with `from_cidrs`.
#[derive(Clone, Debug, Default)]
pub struct HostFilter {
    ranges: Vec<CidrRange>,
}

impl HostFilter {
    pub fn from_cidrs<I, S>(cidrs: I) -> std::result::Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut ranges = Vec::new();
        for cidr in cidrs {
            let cidr = cidr.as_ref().trim();
            if cidr.is_empty() {
                continue;
            }
            ranges.push(CidrRange::from_str(cidr)?);
        }
        Ok(Self { ranges })
    }

    /// Load CIDR ranges from a file. One CIDR per line; `#` starts a comment.
    pub fn from_file(path: &std::path::Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read hosts file {:?}: {e}", path))?;
        let mut ranges = Vec::new();
        for (line_number, line) in content.lines().enumerate() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            ranges.push(
                CidrRange::from_str(line)
                    .map_err(|msg| format!("{}:{}: invalid CIDR entry {:?}: {msg}", path.display(), line_number + 1, line))?,
            );
        }
        Ok(Self { ranges })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        self.ranges.iter().any(|range| range.contains(ip))
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
}

fn prefix_mask_u32(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    }
}

fn prefix_mask_u128(prefix: u8) -> u128 {
    if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn v4_range_contains_member_and_rejects_outsider() {
        let filter = HostFilter::from_cidrs(["5.188.125.0/24"]).unwrap();
        assert!(filter.contains("5.188.125.47".parse().unwrap()));
        assert!(!filter.contains("5.188.126.1".parse().unwrap()));
    }

    #[test]
    fn v6_range_contains_member_and_rejects_outsider() {
        let filter = HostFilter::from_cidrs(["2001:db8::/32"]).unwrap();
        assert!(filter.contains("2001:db8::dead:beef".parse().unwrap()));
        assert!(!filter.contains("2001:db9::1".parse().unwrap()));
    }

    #[test]
    fn v4_range_does_not_match_v6_address() {
        let filter = HostFilter::from_cidrs(["5.188.125.0/24"]).unwrap();
        assert!(!filter.contains("::1".parse().unwrap()));
    }

    #[test]
    fn rejects_invalid_cidr() {
        assert!(HostFilter::from_cidrs(["5.188.125.0/33"]).is_err());
        assert!(HostFilter::from_cidrs(["not-an-ip/24"]).is_err());
        assert!(HostFilter::from_cidrs(["5.188.125.0"]).is_err());
    }

    #[test]
    fn from_file_skips_comments_and_blank_lines() {
        let mut tmp = std::env::temp_dir();
        tmp.push(format!("hosts-test-{}.txt", std::process::id()));
        let mut f = std::fs::File::create(&tmp).unwrap();
        writeln!(f, "# Albion server ranges").unwrap();
        writeln!(f).unwrap();
        writeln!(f, "5.188.125.0/24   # primary fleet").unwrap();
        writeln!(f, "  85.234.70.0/24").unwrap();
        drop(f);

        let filter = HostFilter::from_file(&tmp).unwrap();
        std::fs::remove_file(&tmp).ok();
        assert_eq!(filter.len(), 2);
        assert!(filter.contains("5.188.125.9".parse().unwrap()));
        assert!(filter.contains("85.234.70.200".parse().unwrap()));
    }
}
