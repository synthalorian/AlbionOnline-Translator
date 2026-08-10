# Albion Online Translator

A cross-platform overlay application that translates Albion Online in-game chat in real-time. Built with Tauri v2, Svelte, and Rust.

## Features

- **Passive packet sniffing** — monitors UDP port 5056 for Albion chat traffic
- **No game modification** — zero injection, zero memory reading, zero ban risk
- **Real-time translation** — detects language and translates to your preferred language
- **Always-on-top overlay** — transparent, click-through window that floats above the game
- **Cross-platform** — Linux, Windows, and macOS support

## Legal / Ban Safety

This application is **explicitly allowed** by Sandbox Interactive's stated policy:

> "As long as you just look and analyze we are ok with it. The moment you modify or manipulate something or somehow interfere with our services we will react."
> — MadDave, Technical Lead for Albion Online

This app:
- ✅ Only monitors network traffic (like Wireshark)
- ✅ Does NOT modify the game client
- ✅ Does NOT read game memory
- ✅ Does NOT inject into the game process
- ✅ Does NOT send input to the game

## Architecture

```
┌─────────────────┐     UDP 5056      ┌──────────────────┐
│  Albion Online  │ ◄───────────────► │  Game Server     │
│  (Game Client)  │                   │  (SBI)           │
└────────┬────────┘                   └──────────────────┘
         │
         │ (passive sniff)
         ▼
┌─────────────────┐
│  Packet Sniffer │  libpcap / Npcap
│  (Rust)         │  Filter: UDP port 5056
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Photon Decoder │  Parse Photon header, extract chat events
│  (Rust)         │  Decode sender, channel, message text
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Translation    │  CTranslate2 (local) / Google API / HTTP
│  Engine         │  Language detection + caching
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Tauri Overlay  │  Always-on-top transparent window
│  (Svelte)       │  NOT injected into game process
└─────────────────┘
```

## Development

### Prerequisites

- **Rust** (2021 edition)
- **Node.js** 18+
- **libpcap** (Linux: `sudo apt install libpcap-dev`, macOS: `brew install libpcap`)
- **Npcap** (Windows: install with "WinPcap API-compatible Mode")

### Setup

```bash
# Clone
git clone https://github.com/synthalorian/AlbionOnline-Translator.git
cd AlbionOnline-Translator

# Install frontend deps
npm install

# Run in development
npm run tauri dev

# Build for production
npm run tauri build
```

### Linux Permissions

On Linux, grant capture capabilities to avoid running as root:

```bash
sudo setcap cap_net_raw,cap_net_admin=eip ./src-tauri/target/release/albion-translator
```

## Roadmap

- [x] Project scaffold (Tauri + Svelte + Rust)
- [x] Packet sniffer with pcap
- [x] Photon protocol decoder scaffold
- [x] Translation engine interface
- [x] Always-on-top overlay GUI
- [ ] Complete Photon chat event decoding
- [ ] CTranslate2 local translation models
- [ ] Google Translate API integration
- [ ] Language detection with lingua-rs
- [ ] SQLite translation cache
- [ ] Installer packaging (NSIS, AppImage, DMG)
- [ ] Auto-update message definitions

## References

- [Albion Online Data Project](https://www.albion-online-data.com/) — market data sniffer (proof of concept)
- [albion-online-stats](https://github.com/mazurwiktor/albion-online-stats) — DPS tracker (compliance reference)
- [albion-translator](https://github.com/beemerwt/albion-translator) — CLI translator (protocol reference)
- [albion-online-addons](https://github.com/mazurwiktor/albion-online-addons) — packet decoding library

## License

MIT

## Disclaimer

Not affiliated with Sandbox Interactive. Use at your own risk. While this app follows SBI's stated policy on passive monitoring, policies may change.
