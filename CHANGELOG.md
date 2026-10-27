# Changelog

Notable changes to Relayless. Releases are published automatically from `main` as `v<version>-build<run>`.

## Unreleased

### Added
- **Privacy receipts.** Every finished transfer keeps a receipt on the device: route (direct or relayed bytes), encryption, blocks verified, a file fingerprint shared by both devices, and what servers stored (nothing). Copy it as text.
- **History.** A month calendar with a dot per day sized by data moved (hover for the files), totals per device pair, and a breakdown by file type. Stored only on the device; keep it for 1 year (default), 90 days, 30 days, forever or not at all, with or without file names.
- **Safety words.** Five words derived from both devices' keys, shown when a device asks for access, while connecting, and on device lists.
- **Live speed graph** on Overview and Transfers: receive and send speed for the last two minutes, with markers for pauses, reconnects and route changes.
- **Sending is visible.** Files this device sends now appear in Transfers (new Sending tab), on Overview and on the phone Home screen, each with the receiver's progress, a percentage, speed, time left and a "Delivered and verified" state once the receiver has checked the file.
- Downloads show a percentage too.
- Privacy section in Settings.
- New phone Home screen: big Send and Receive buttons, live speed, transfers in progress, this week's activity as dots, devices and your shared folders.
- Phone navigation trimmed to Home, Devices, Transfers and More; Shared folders, History, Activity and Settings open from More.
- Glass interface with a new dark theme and green accent, bundled Figtree typeface, new app icon.
- Phone layout: bottom-sheet dialogs, large two-line rows in the folder browser, tap to select, sort chips, a fixed download bar, and a save location you can pick without typing.
- Project website, contribution guide, security policy and architecture notes.

### Changed
- The receiver now sends the sender a short receipt after a file is verified, so both keep a history entry. Older versions ignore it.
- **Protocol v2.** Chunks are sent in 1 MiB blocks, each with its own BLAKE3 hash, and can be resumed from a block offset.
- Download progress is saved per verified megabyte, so pausing or losing the connection keeps everything already received. Downloads started on earlier versions keep their progress.

### Fixed
- **Transfers between different networks no longer sit at 0.** Relayless now tries a direct path first and, if routers block it (common on mobile data), continues through an end-to-end encrypted relay after about 15 seconds, switching back to direct when possible. Existing installs move to this mode once; Direct only is still in Settings.
- Slow links no longer time out mid-block: a stream is only treated as stalled after 90 seconds with no data, and progress and speed now count bytes as they arrive.
- On phones, no page can be scrolled sideways any more (Overview, Transfers and Settings were wider than the screen on small phones).
- Updated the `rand` dependency to 0.10.
- Pausing early in a download no longer starts it over on resume.
- On Android, the keyboard no longer opens and closes repeatedly in dialogs.

## 0.1.0

First public version.

- Direct, end-to-end encrypted transfers between Windows and Android over QUIC with NAT hole punching.
- Shared folders, invitations with expiry and use limits, 12-character short codes, device approval and revocation.
- Remote folder browsing, file and folder downloads, pause, resume, retry and cancel.
- Crash-safe progress, whole-file verification, source change detection.
- Direct-only mode by default, with optional relay fallback.
- Windows tray and autostart; Android foreground service for screen-off transfers.
