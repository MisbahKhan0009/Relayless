# Security Policy

Relayless moves people's files between their devices, so security reports are taken seriously and handled first.

## Reporting a vulnerability

Please **don't** open a public issue. Report privately through GitHub:

**[Report a vulnerability](https://github.com/MisbahKhan0009/Relayless/security/advisories/new)**

Include what you found, the affected version, steps to reproduce, and the impact you expect. A proof of concept helps.

What to expect:

- acknowledgement within 3 days
- an assessment and a plan within 10 days
- a fix released as soon as practical, with credit in the release notes unless you prefer otherwise

Please give us a reasonable time to release a fix before disclosing publicly.

## Supported versions

Security fixes go into the latest release. Both devices must run the same protocol version, so please update both.

## Scope

Especially interesting:

- reading or listing anything outside a shared folder
- connecting or downloading without an approved invitation or grant
- tampering with data in transit without detection, or a file being finalized with wrong content
- invitation or short-code weaknesses (guessing, replay, use after revocation or expiry)
- file data passing through a relay while direct-only mode is on
- crashes or resource exhaustion triggered by a remote peer
- secrets or file contents appearing in logs
- safety words that differ from what both devices should derive, or that could be forced to collide
- history or receipt data leaving the device, or a peer writing history entries it shouldn't

Out of scope: attacks that require an already compromised device, social engineering, and the unsigned-installer warnings shown by Windows and Android.

## Design

The security model is described in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): Ed25519 device identities, QUIC with TLS 1.3, hashed and expiring invitation tokens, Argon2id-stretched short codes, sender-side authorization of every request, and BLAKE3 verification of every 1 MiB block and of the final file, safety words derived from both device keys, and history kept only on the device.
