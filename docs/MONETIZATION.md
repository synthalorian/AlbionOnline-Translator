# Monetization Setup — Albion Translator

**Model:** $9.99 one-time purchase, 7-day free trial, license key activation via Lemon Squeezy.

Made by synth with synthclaw 🎹🦞

---

## What's already built (code)

- `src-tauri/src/license.rs` — LicenseManager: trial clock (7 days from first launch),
  Lemon Squeezy activate/validate/deactivate, 24h revalidation cadence, 7-day offline grace,
  local persistence in `<config>/albion-translator/license.json`
- `src/lib/LicenseGate.svelte` — trial banner + full paywall overlay + key entry UI
- `src/lib/license.js` — frontend glue
- Backend gates chat-message forwarding: locked = no translated messages, frontend gets
  a `license-locked` event and shows the paywall

## What synth must do (accounts — can't be automated)

### 1. Lemon Squeezy account (~15 min)
- [ ] Sign up at https://lemonsqueezy.com → create a store
- [ ] Complete payout details (bank/PayPal) — LS is merchant of record, **they handle
      global sales tax/VAT**. This is why we picked them.
- [ ] Create product: **"Albion Translator"** — one-time, **$9.99**
- [ ] In product settings: enable **license key generation** (auto-generated per order)
- [ ] Set activation limit: **3 machines** (desktop app, people have a PC + laptop)
- [ ] Copy the checkout URL

### 2. Wire the checkout URL into the app
- [ ] Replace `REPLACE_ME` in `BUY_URL` (`src-tauri/src/license.rs`) with the real
      checkout URL from step 1

### 3. Test end-to-end before release
- [ ] LS dashboard → create a **100% off discount code** for testing
- [ ] "Buy" the app with the code → receive a real license key by email
- [ ] Activate in-app → confirm paywall clears
- [ ] Delete `~/.config/albion-translator/license.json` → confirm trial/paywall returns

### 4. Launch checklist
- [ ] Landing page or itch.io-style listing (screenshots, what it does, buy button)
- [ ] Windows installer build (`cargo tauri build` — MSI/NSIS)
- [ ] Decide Linux distribution (AppImage via Tauri; you're the Linux user, buyers will be 95% Windows)
- [ ] GitHub release with binaries attached (repo is private — releases still work)

## The numbers

| Item | Value |
|---|---|
| Price | $9.99 one-time |
| LS fee | ~5% + 50¢ → **~$9 net per sale** |
| Break-even vs ads | 12 sales ≈ a year of ad revenue at this scale |
| Marginal cost per user | $0 (translation is client-side) |

## Anti-piracy posture (intentional)

Client-side check = toll booth, not vault. A determined user could patch the binary.
We accept this: 98% of Albion players can't, and the $9.99 price point is below the
hassle threshold. Do NOT add phone-home DRM beyond the 24h revalidation + 7-day grace —
being draconian to paying users costs more than piracy does.

## Dependency note — RESOLVED 2026-08-12

`albion-network-lib` (beemerwt) dependency was dropped (commit 03c47c5). Packet
decoding is now our own `src-tauri/src/photon.rs` — a Photon Protocol18
implementation (public protocol spec). The unlicensed-repo blocker is gone;
the license request issue is moot:
https://github.com/beemerwt/albion-network-lib/issues/1

Remaining ship blockers live in the checklist above: LS account, BUY_URL,
release-build verification, Windows installer.
