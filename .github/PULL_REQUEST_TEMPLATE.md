## What changed

<!-- A short summary. Link the issue it addresses, e.g. "Fixes #12". -->

## How it was tested

<!-- Commands you ran, devices you tried, screenshots for UI changes. -->

- [ ] `cargo test -p relayless-core --release`
- [ ] `npm run typecheck`
- [ ] Tried it on: <!-- Windows / Android / both -->

## Checklist

- [ ] Protocol changes bump `PROTOCOL_VERSION` and update `docs/ARCHITECTURE.md`
- [ ] Database changes add a migration that keeps existing downloads
- [ ] User-facing changes are noted in `CHANGELOG.md`
