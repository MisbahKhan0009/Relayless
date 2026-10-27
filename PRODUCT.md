# Product

<!-- impeccable:product-schema 1 -->

## Platform

adaptive

Windows desktop (Tauri 2) and Android (sideloaded APK) share one React interface that adapts its layout per device class; the project website is plain static HTML/CSS/JS in `website/`.

## Users

Primary: small teams and labs (research groups, studios) moving large files such as datasets, builds and media masters between their own machines and collaborators' machines. They pair devices once with an invitation, then pull files from a shared folder, often files of many gigabytes over home or campus connections.

## Product Purpose

Relayless sends large files directly from one device to another, end-to-end encrypted, resumable after pauses, crashes or reboots, and verified block by block. It aims to use the full speed of the connection without making the computer or phone feel busy. Success: a large transfer finishes intact, the user can prove it stayed private, and they come back to use it again.

## Positioning

No account, no cloud copy, no upload step. Files go device to device over QUIC with NAT hole punching; a direct path is always tried first, and when routers block it a relay carries only end-to-end encrypted bytes it cannot read or keep (direct-only mode is one setting away). Every transfer leaves a privacy receipt on the device (route, encryption, verification, what servers stored: nothing) and paired devices can compare safety words to rule out anyone in the middle.

## Operating Context

- Sender shares a folder, creates an invitation (link, QR or 12-character short code), approves the device.
- Receiver browses the shared folder remotely and downloads files or folders.
- Transfers run in the background: Windows tray, Android foreground service.
- History, statistics and receipts are kept only on the device, with user-set retention.

## Capabilities and Constraints

- Protocol v2: chunks sent in 1 MiB blocks, each with a BLAKE3 hash; resume from the last saved block; final whole-file check.
- Direct first by default, with encrypted relay fallback after about 15 seconds; optional direct-only mode.
- Bandwidth limits, parallel streams, low CPU priority option.
- Safety words: five words derived from both devices' public keys.
- Local history: per-transfer receipts, calendar of activity, totals per device pair and per file category. Default retention 1 year; options forever, 90 days, 30 days, off; option to keep history without file names.
- No telemetry and no server-side storage of files or history.

## Brand Commitments

- Name: Relayless. Independent open-source project (MIT), package `app.relayless`; not tied to any company brand.
- Privacy is the main promise, said by showing proof: calm, specific statements backed by receipts, safety words and what never leaves the device. No hype.
- Copy never uses em dashes.

## Evidence on Hand

- Real app screenshots via the mock preview mode (`npm run dev:ui`), demo data clearly fictional.
- Website at `website/`, live on GitHub Pages.
- No customers, testimonials, benchmarks or download counts exist; never invent them.

## Product Principles

1. Prove privacy, don't claim it: every privacy statement maps to something the user can check.
2. Nothing leaves the device unless the user moves a file.
3. Large files are the normal case: progress, resume and verification must stay legible at 100 GB.
4. Calm under load: the app stays quiet and light while transfers run at full speed.
5. Earn return visits through insight into one's own transfers, not through engagement tricks.
