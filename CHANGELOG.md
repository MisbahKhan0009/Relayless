# Changelog

Notable changes to Relayless. Releases are published automatically from `main` as `v<version>-build<run>`.

## Unreleased

### Added
- Glass interface with a new dark theme and green accent, bundled Figtree typeface, new app icon.
- Phone layout: bottom-sheet dialogs, large two-line rows in the folder browser, tap to select, sort chips, a fixed download bar, and a save location you can pick without typing.
- Project website, contribution guide, security policy and architecture notes.

### Changed
- **Protocol v2.** Chunks are sent in 1 MiB blocks, each with its own BLAKE3 hash, and can be resumed from a block offset. Both devices need this version.
- Download progress is saved per verified megabyte, so pausing or losing the connection keeps everything already received. Downloads started on earlier versions keep their progress.

### Fixed
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
