use pcap::{Capture, Device};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::{network::extract_udp_payload, photon::PhotonDecoder, translator::TranslationEngine, hosts::HostFilter};
use crate::photon::{self, ChatChannel};

// Albion's game UDP ports are stable across the entire server fleet, while
// the server IPs rotate across many ranges (5.188.125.x, 5.45.187.x,
// 193.169.238.x, 85.234.70.x all observed live), so an IP whitelist silently
// drops chat from any range not listed. Filter on ports at the BPF level
// (fast, kernel-side) and on IPs after extraction (user-side, based on
// hosts.txt CIDR ranges).
const BPF_FILTER: &str = "udp port 5055 or udp port 5056 or udp port 4535";

const DEFAULT_HOSTS_PATH: &str = "hosts.txt";

/// Channel mappings persist across app restarts (same game session = same ids).
/// Saved to ~/.config/albion-translator/channels.json.
fn channel_map_path() -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("albion-translator");
    std::fs::create_dir_all(&dir).ok();
    dir.join("channels.json")
}

pub fn save_channel_map(map: &HashMap<i64, ChatChannel>) {
    let path = channel_map_path();
    // Only save non-Unknown mappings — no point persisting noise
    let filtered: HashMap<String, String> = map
        .iter()
        .filter(|(_, ch)| **ch != ChatChannel::Unknown && **ch != ChatChannel::Language)
        .map(|(id, ch)| (id.to_string(), ch.to_string()))
        .collect();
    if let Ok(json) = serde_json::to_string_pretty(&filtered) {
        std::fs::write(&path, json).ok();
    }
}

pub fn load_channel_map() -> HashMap<i64, ChatChannel> {
    let path = channel_map_path();
    let Ok(json) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    let Ok(raw) = serde_json::from_str::<HashMap<String, String>>(&json) else {
        return HashMap::new();
    };
    raw.iter()
        .filter_map(|(id, ch)| {
            let id = id.parse::<i64>().ok()?;
            let channel = match ch.as_str() {
                "Local" => ChatChannel::Say,
                "Whisper" => ChatChannel::Whisper,
                "Party" => ChatChannel::Party,
                "Guild" => ChatChannel::Guild,
                "Alliance" => ChatChannel::Alliance,
                "Global" => ChatChannel::Global,
                "Trade" => ChatChannel::Trade,
                "LFG" => ChatChannel::LFG,
                "Recruitment" => ChatChannel::Recruitment,
                "Faction" => ChatChannel::Faction,
                _ => return None,
            };
            Some((id, channel))
        })
        .collect()
}

/// The machine's primary outbound IPv4, found by "connecting" a UDP socket —
/// no traffic is actually sent, the kernel just resolves the route and tells
/// us which local address it would use. Cross-platform replacement for
/// parsing `ip route` (which doesn't exist on Windows).
fn primary_outbound_ip() -> Option<IpAddr> {
    let sock = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    sock.connect(("8.8.8.8", 80)).ok()?;
    sock.local_addr().ok().map(|a| a.ip())
}

/// Human-readable label for a pcap device: description (friendly on Windows,
/// e.g. "Realtek PCIe GbE Family Controller") falling back to the raw name.
fn device_label(dev: &Device) -> String {
    match &dev.desc {
        Some(d) if !d.is_empty() => format!("{} ({})", d, dev.name),
        _ => dev.name.clone(),
    }
}

/// VPN/tunnel adapters: WARP, WireGuard, OpenVPN TAP, Tailscale, etc.
/// Capturing one of these is doubly broken for Albion: the game traffic is
/// encapsulated+encrypted inside the tunnel, so the Albion-port BPF filter
/// can never match, and binding the tunnel means we ignore the physical NIC
/// where split-tunneled (or VPN-disabled) traffic actually flows.
/// Token-based so "Fortinet" doesn't trip on "tun".
/// Token-based VPN/tunnel name match, split out for testing — "Fortinet"
/// must not trip on "tun", but "Cloudflare WARP Interface Tunnel" must flag.
fn looks_like_tunnel(name: &str, desc: &str) -> bool {
    let text = format!("{} {}", name.to_lowercase(), desc.to_lowercase());
    const TOKENS: &[&str] = &[
        "warp", "wireguard", "wg", "nordlynx", "tailscale", "zerotier",
        "openvpn", "tap", "tun", "utun", "tunnel", "vpn", "ppp", "wgcf",
    ];
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| {
            // "warp" is VPN-specific enough to match inside compounds
            // (Cloudflare's Linux device is literally "CloudflareWARP").
            if tok.contains("warp") {
                return true;
            }
            TOKENS.iter().any(|kw| {
                // Exact ("tun") or keyword + digit suffix ("tun0", "tailscale0", "wg0").
                tok == *kw
                    || (tok.starts_with(kw)
                        && tok.len() > kw.len()
                        && tok[kw.len()..].chars().all(|c| c.is_ascii_digit()))
            })
        })
}

fn is_tunnel_device(dev: &Device) -> bool {
    looks_like_tunnel(&dev.name, dev.desc.as_deref().unwrap_or(""))
}

/// Is this address usable as evidence of a real network? Excludes loopback
/// AND link-local (169.254.x.x = "unplugged/unconfigured") — a Bluetooth PAN
/// with a link-local address beat the real Wi-Fi adapter in fallback
/// selection (burned 2026-08-28, Ola's machine).
fn usable_ipv4(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified(),
        _ => false,
    }
}

/// First non-loopback, non-link-local IPv4 address of a device, if any.
fn first_ipv4(dev: &Device) -> Option<IpAddr> {
    dev.addresses.iter().map(|a| a.addr).find(usable_ipv4)
}

/// Structured device info for the UI picker.
#[derive(serde::Serialize, Clone, Debug)]
pub struct CaptureDeviceInfo {
    /// Raw pcap name — the stable identifier passed back to start_capture.
    pub name: String,
    /// Human-readable label for display.
    pub label: String,
    /// Heuristic: VPN/tunnel adapter (bad choice for Albion capture).
    pub is_tunnel: bool,
    /// First non-loopback IPv4, empty string if none.
    pub ipv4: String,
}

/// All capturable devices, structured — feeds the settings interface picker.
pub fn list_devices_detailed() -> Vec<CaptureDeviceInfo> {
    Device::list()
        .map(|devs| {
            devs.iter()
                .map(|d| CaptureDeviceInfo {
                    name: d.name.clone(),
                    label: device_label(d),
                    is_tunnel: is_tunnel_device(d),
                    ipv4: first_ipv4(d)
                        .map(|a| a.to_string())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

const ALBION_PORTS: [u16; 3] = [5055, 5056, 4535];

/// Result of the 10-second network diagnostic. The three counters triangulate
/// every "CAPTURING but no messages" report:
///   total == 0            → Npcap/driver/adapter capture is broken outright
///   total > 0, albion == 0 → capture works; game traffic isn't on this adapter
///                            (VPN tunnel, or the game isn't chatting)
///   albion > 0            → traffic arrives; any silence is the decoder
/// top_udp_ports reveals WHERE traffic actually goes (2408 = WireGuard/WARP,
/// 443 = QUIC, 53 = DNS) — the smoking gun for "the VPN is secretly still on".
#[derive(serde::Serialize, Clone, Debug)]
pub struct DiagReport {
    pub device: String,
    pub duration_secs: u64,
    pub total_packets: u64,
    pub udp_packets: u64,
    pub albion_packets: u64,
    pub top_udp_ports: Vec<(u16, u64)>,
}

/// Blocking packet survey on the selected (or auto) device with NO filter.
/// Call from spawn_blocking. Fails if the main capture is running.
pub fn run_diagnostic(
    preferred_device: Option<&str>,
    duration_secs: u64,
    capture_busy: bool,
) -> Result<DiagReport, SnifferError> {
    if capture_busy {
        return Err(SnifferError::AlreadyRunning);
    }
    let device = select_device(preferred_device)?;
    let label = device_label(&device);
    info!("Diagnostic capture on {} for {}s", label, duration_secs);

    let mut cap = Capture::from_device(device)
        .map_err(|e| SnifferError::CaptureOpen(e.to_string()))?
        .promisc(true)
        .snaplen(65535)
        .timeout(1000)
        .open()
        .map_err(|e| SnifferError::CaptureOpen(e.to_string()))?;
    // Deliberately NO BPF filter: we want to see everything the adapter sees.

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(duration_secs);
    let mut total = 0u64;
    let mut udp = 0u64;
    let mut albion = 0u64;
    let mut port_counts: HashMap<u16, u64> = HashMap::new();

    while std::time::Instant::now() < deadline {
        match cap.next_packet() {
            Ok(packet) => {
                total += 1;
                if let Some((_, _, sp, dp, _)) = crate::network::extract_udp_packet(packet.data) {
                    udp += 1;
                    if ALBION_PORTS.contains(&sp) || ALBION_PORTS.contains(&dp) {
                        albion += 1;
                    }
                    // The lower port is usually the service port (53/443/2408…).
                    *port_counts.entry(sp.min(dp)).or_insert(0) += 1;
                }
            }
            Err(pcap::Error::TimeoutExpired) => continue,
            Err(_) => break,
        }
    }

    let mut top: Vec<(u16, u64)> = port_counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    top.truncate(5);

    let report = DiagReport {
        device: label,
        duration_secs,
        total_packets: total,
        udp_packets: udp,
        albion_packets: albion,
        top_udp_ports: top,
    };
    info!("Diagnostic result: {:?}", report);
    Ok(report)
}

/// Pick the capture device. `preferred` = raw pcap name from the UI picker.
///
/// Auto order: device owning the primary outbound IP → if that's a tunnel,
/// the first non-tunnel device with a real IPv4 (split-tunnel / VPN-paused
/// setups) → Device::lookup() as last resort.
fn select_device(preferred: Option<&str>) -> Result<Device, SnifferError> {
    let devices = Device::list().map_err(|e| SnifferError::DeviceLookup(e.to_string()))?;

    if let Some(name) = preferred.filter(|n| !n.is_empty()) {
        let dev = devices
            .iter()
            .find(|d| d.name == name)
            .cloned()
            .ok_or_else(|| {
                SnifferError::DeviceLookup(format!(
                    "selected interface not found: {} (available: {})",
                    name,
                    devices.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", ")
                ))
            })?;
        if is_tunnel_device(&dev) {
            info!("Note: manually selected interface looks like a VPN/tunnel adapter");
        }
        return Ok(dev);
    }

    let primary_ip = primary_outbound_ip();
    if let Some(ip) = primary_ip {
        info!("Primary outbound IP: {}", ip);
    }

    let primary_dev = primary_ip.and_then(|ip| {
        devices
            .iter()
            .find(|d| d.addresses.iter().any(|a| a.addr == ip))
            .cloned()
    });

    match primary_dev {
        Some(dev) if !is_tunnel_device(&dev) => Ok(dev),
        Some(tunnel) => {
            // The default route lives on a VPN tunnel (Cloudflare WARP etc.).
            // Albion traffic encapsulated in the tunnel is invisible to our
            // port filter, so prefer the physical NIC — correct for
            // split-tunneled games and for a VPN paused after route setup.
            info!(
                "Primary outbound device is a VPN/tunnel ({}); preferring physical adapter",
                device_label(&tunnel)
            );
            devices
                .iter()
                .find(|d| !is_tunnel_device(d) && first_ipv4(d).is_some())
                .cloned()
                .or_else(|| Device::lookup().ok().flatten())
                .ok_or(SnifferError::NoDevice)
        }
        None => Device::lookup()
            .ok()
            .flatten()
            .ok_or(SnifferError::NoDevice),
    }
}

/// List all capturable devices with their addresses — for diagnostics and a
/// future manual picker.
pub fn list_devices() -> Vec<String> {
    Device::list()
        .map(|devs| {
            devs.iter()
                .map(|d| {
                    let addrs: Vec<String> =
                        d.addresses.iter().map(|a| a.addr.to_string()).collect();
                    format!("{} [{}]", device_label(d), addrs.join(", "))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub struct PacketSniffer {
    running: Arc<AtomicBool>,
    packets: Arc<AtomicU64>,
    tx: mpsc::Sender<photon::ChatMessage>,
    host_filter: Option<HostFilter>,
    /// Shared channel map — the decoder reads/writes through this, and the
    /// Tauri command handler can inject manual mappings from the UI.
    channel_map: Arc<StdMutex<HashMap<i64, ChatChannel>>>,
}

impl PacketSniffer {
    pub fn new(tx: mpsc::Sender<photon::ChatMessage>) -> Self {
        // Try to load hosts.txt from the current working directory; if it
        // doesn't exist or is empty, run unfiltered (backward compat).
        let host_filter = if std::path::Path::new(DEFAULT_HOSTS_PATH).exists() {
            HostFilter::from_file(std::path::Path::new(DEFAULT_HOSTS_PATH)).ok()
        } else {
            None
        };

        // Load persisted channel mappings from previous sessions
        let saved = load_channel_map();
        if !saved.is_empty() {
            info!("Loaded {} saved channel mappings", saved.len());
        }

        Self {
            running: Arc::new(AtomicBool::new(false)),
            packets: Arc::new(AtomicU64::new(0)),
            tx,
            host_filter,
            channel_map: Arc::new(StdMutex::new(saved)),
        }
    }

    /// Start capture. Returns a human-readable label of the device being
    /// listened on, so the UI can show exactly where packets come from.
    /// `preferred_device` = raw pcap name from the settings picker (None = auto).
    pub fn start(&mut self, preferred_device: Option<&str>) -> Result<String, SnifferError> {
        if self.running.load(Ordering::SeqCst) {
            return Err(SnifferError::AlreadyRunning);
        }

        // Auto: the interface owning the primary outbound IP — Device::lookup()
        // happily returns a VPN/loopback/virtual adapter and silently captures
        // nothing, and Windows has no `ip route` to parse. The UDP-connect
        // trick works identically on Linux and Windows. If the primary device
        // is a VPN tunnel (Cloudflare WARP…), prefer the physical NIC instead —
        // tunnel-encapsulated Albion traffic can never match our port filter.
        let device = select_device(preferred_device)?;

        let label = device_label(&device);
        info!("Using device: {}", label);

        if let Some(ref hf) = self.host_filter {
            info!(
                "IP filtering active: {} CIDR range(s)",
                hf.len()
            );
        } else {
            info!("IP filtering disabled — all UDP on Albion ports will be processed");
        }

        let mut cap = None;
        // cargo rebuilds replace the binary and wipe its setcap caps, so a
        // freshly relaunched app can hit a brief permission window. Retry
        // instead of failing the capture outright.
        for attempt in 1..=5 {
            let inactive = match Capture::from_device(device.clone()) {
                Ok(d) => d,
                Err(e) => return Err(SnifferError::CaptureOpen(e.to_string())),
            };
            match inactive
                .promisc(true)
                .snaplen(65535)
                .timeout(1000)
                .open()
            {
                Ok(c) => {
                    cap = Some(c);
                    break;
                }
                Err(e) => {
                    if attempt == 5 {
                        return Err(SnifferError::CaptureOpen(e.to_string()));
                    }
                    error!("Capture open failed (attempt {}): {} — retrying", attempt, e);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }
        let mut cap = cap.expect("capture open should succeed after retries");

        cap.filter(BPF_FILTER, true)
            .map_err(|e| SnifferError::Filter(e.to_string()))?;

        self.running.store(true, Ordering::SeqCst);
        self.packets.store(0, Ordering::SeqCst);
        let running = self.running.clone();
        let packets = self.packets.clone();
        let tx = self.tx.clone();
        let host_filter = self.host_filter.clone();
        let channel_map = self.channel_map.clone();

        // Raw decoded (untranslated) messages flow into a bounded channel;
        // a dedicated worker translates them off the capture loop so a slow
        // network call can never stall packet sniffing.
        let (raw_tx, mut raw_rx) = mpsc::channel::<photon::ChatMessage>(64);

        tokio::spawn(async move {
            debug!("translation worker: spawned");
            let mut translator = TranslationEngine::new();
            debug!("translation worker: engine ready, entering recv loop");
            while let Some(msg) = raw_rx.recv().await {
                debug!("translation worker: received msg from {}", msg.sender);
                let ui_msg = Self::convert_message(&msg, &mut translator).await;
                if tx.send(ui_msg).await.is_err() {
                    break;
                }
            }
            debug!("translation worker: recv loop exited");
        });

        // CRITICAL: this loop MUST live on the blocking thread pool, not in a
        // tokio::spawn task. pcap's next_packet() blocks the thread
        // synchronously, so the capture task's poll() never returns Pending —
        // it pins a runtime worker forever. The mpsc receiver woken by send()
        // lands in that pinned worker's local queue and is never polled:
        // translation silently starves while decoding logs look healthy.
        // spawn_blocking + blocking_send is the correct shape for a
        // synchronous capture source.
        tokio::task::spawn_blocking(move || {
            info!("Packet capture started");

            let mut decoder = PhotonDecoder::with_channel_map(channel_map);
            let mut packet_number = 0usize;
            let mut filtered_count = 0usize;
            let mut ip_filtered_count = 0usize;

            while running.load(Ordering::SeqCst) {
                match cap.next_packet() {
                    Ok(packet) => {
                        packet_number += 1;
                        packets.store(packet_number as u64, Ordering::SeqCst);

                        // Extract UDP payload from raw ethernet frame. The BPF
                        // port filter already gates on Albion's ports; the
                        // decoder below validates structure, so non-Albion
                        // traffic never survives to the channel.
                        if let Some((src_ip, dst_ip, payload)) = extract_udp_payload(packet.data) {
                            // Apply IP-based host filtering (hosts.txt CIDR ranges).
                            // Match either endpoint: inbound chat has the server as
                            // src, but outbound whispers have it as dst — checking
                            // only src would silently drop everything you send.
                            if let Some(ref hf) = host_filter {
                                if !hf.contains(src_ip) && !hf.contains(dst_ip) {
                                    ip_filtered_count += 1;
                                    continue;
                                }
                            }

                            if let Some(msg) = decoder.decode(payload) {
                                // blocking_send waits for capacity — natural
                                // backpressure straight to the capture loop.
                                if raw_tx.blocking_send(msg).is_err() {
                                    error!("Failed to send chat message");
                                    break;
                                }
                            }
                        } else {
                            // Packet passed the port filter but isn't extractable
                            // IP/UDP — count it so the stop log stays diagnostic.
                            filtered_count += 1;
                        }
                    }
                    Err(e) => {
                        if running.load(Ordering::SeqCst) {
                            debug!("Capture timeout: {}", e);
                        }
                    }
                }
            }

            info!(
                "Packet capture stopped. Total: {}, Filtered (non-IP): {}, IP-filtered: {}",
                packet_number, filtered_count, ip_filtered_count
            );
        });

        Ok(label)
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Packets captured since the current capture session started.
    pub fn packet_count(&self) -> u64 {
        self.packets.load(Ordering::SeqCst)
    }

    /// Inject a manual channel mapping from the UI (e.g. user tags Unknown
    /// channel 25813 as Guild). Takes effect immediately for future messages.
    /// Persisted to disk so it survives app restarts within the same game session.
    pub fn set_channel_mapping(&self, channel_id: i64, channel: ChatChannel) {
        if let Ok(mut map) = self.channel_map.lock() {
            info!("Manual channel mapping: {} -> {}", channel_id, channel);
            map.insert(channel_id, channel);
            save_channel_map(&map);
        }
    }

    /// Get the shared channel map for the decoder.
    pub fn shared_channel_map(&self) -> Arc<StdMutex<HashMap<i64, ChatChannel>>> {
        self.channel_map.clone()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    async fn convert_message(msg: &photon::ChatMessage, translator: &mut TranslationEngine) -> photon::ChatMessage {
        let source_lang = translator.detect_language(&msg.text);
        debug!(
            "convert_message: sender={} detect={:?} text_len={}",
            msg.sender,
            source_lang,
            msg.text.len()
        );

        // translate() self-gates: it returns None only when lingua is
        // CONFIDENT the text is already the target language. Uncertain,
        // mixed, or unsupported-language chat goes to Google sl=auto, so
        // nothing gets silently dropped here anymore.
        let translated_text = translator.translate(&msg.text, source_lang.as_deref()).await;

        photon::ChatMessage {
            timestamp: msg.timestamp.clone(),
            channel: msg.channel.clone(),
            channel_id: msg.channel_id,
            sender: msg.sender.clone(),
            text: msg.text.clone(),
            source_lang,
            translated_text,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SnifferError {
    #[error("Capture already running")]
    AlreadyRunning,
    #[error("No network device found")]
    NoDevice,
    #[error("Failed to lookup device: {0}")]
    DeviceLookup(String),
    #[error("Failed to open capture: {0}")]
    CaptureOpen(String),
    #[error("Failed to set filter: {0}")]
    Filter(String),
}

#[cfg(test)]
mod device_selection_tests {
    use super::*;

    #[test]
    fn tunnel_heuristic_flags_vpn_adapters() {
        // Ola's actual adapter (2026-08): must flag.
        assert!(looks_like_tunnel(
            "\\Device\\NPF_{D8484304-D804-6AA0-A33D-72368368364D}",
            "Cloudflare WARP Interface Tunnel"
        ));
        assert!(looks_like_tunnel("wg0", "WireGuard Tunnel"));
        assert!(looks_like_tunnel("tailscale0", ""));
        assert!(looks_like_tunnel("", "TAP-Windows Adapter V9"));
        assert!(looks_like_tunnel("tun0", ""));
        assert!(looks_like_tunnel("CloudflareWARP", ""));
    }

    #[test]
    fn tunnel_heuristic_ignores_physical_adapters() {
        assert!(!looks_like_tunnel(
            "\\Device\\NPF_{88FA266C-1A3A-4044-BEE5-B452A5C4A23F}",
            "Intel(R) Wi-Fi 6 AX201 160MHz"
        ));
        assert!(!looks_like_tunnel("enp0s31f6", ""));
        assert!(!looks_like_tunnel("eth0", "Realtek PCIe GbE Family Controller"));
        // Substring traps: "tun" in Fortinet, "tap" in… nothing common, but
        // tokenization must protect us regardless.
        assert!(!looks_like_tunnel("", "Fortinet Ethernet Adapter"));
    }

    #[test]
    fn usable_ipv4_rejects_loopback_and_linklocal() {
        use std::net::Ipv4Addr;
        // Ola's actual adapter table (2026-08): Bluetooth PAN 169.254.44.80
        // and unplugged Realtek 169.254.193.51 must NOT qualify; the Intel
        // Wi-Fi at 192.168.10.25 must.
        assert!(!usable_ipv4(&IpAddr::V4(Ipv4Addr::new(169, 254, 44, 80))));
        assert!(!usable_ipv4(&IpAddr::V4(Ipv4Addr::new(169, 254, 193, 51))));
        assert!(!usable_ipv4(&IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(!usable_ipv4(&IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
        assert!(usable_ipv4(&IpAddr::V4(Ipv4Addr::new(192, 168, 10, 25))));
        // IPv6 never qualifies as the primary evidence address.
        assert!(!usable_ipv4(&"fe80::1".parse::<IpAddr>().unwrap()));
    }

    /// The UDP-connect trick must yield a real local IPv4, and that IP
    /// must belong to exactly one capturable device — the selection logic
    /// start() relies on. Device::list() works unprivileged on Linux/Windows.
    #[test]
    fn primary_ip_matches_a_capture_device() {
        let ip =
            primary_outbound_ip().expect("UDP-connect trick must resolve an outbound IP");
        assert!(ip.is_ipv4(), "expected IPv4 primary address, got {}", ip);
        assert!(!ip.is_loopback(), "primary IP must not be loopback");

        let devices = Device::list().expect("Device::list must work unprivileged");
        let matches: Vec<_> = devices
            .iter()
            .filter(|d| d.addresses.iter().any(|a| a.addr == ip))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "primary IP {} must match exactly one device; found {:?} among {}",
            ip,
            matches.iter().map(|d| &d.name).collect::<Vec<_>>(),
            list_devices().join(" | ")
        );
    }
}
