# Contributing to Relayless

Thanks for helping. Bug reports, ideas, translations, documentation and code are all welcome.

## Reporting bugs

Open a [bug report](https://github.com/MisbahKhan0009/Relayless/issues/new?template=bug_report.yml) and include:

- the Relayless version on both devices (Settings → About)
- Windows or Android version, and device model for Android
- what you expected and what happened, with exact error messages
- whether the route was direct or relayed

Logs are in `%APPDATA%\app.relayless.desktop\logs` on Windows. They contain no file contents, but may contain file names; review before sharing.

Security issues: please follow [SECURITY.md](SECURITY.md) instead of filing a public issue.

## Development setup

You need Rust (stable) and Node.js 22. Windows needs the WebView2 runtime (preinstalled on 10 and 11). Android builds also need JDK 21 and the Android SDK and NDK.

```sh
git clone https://github.com/MisbahKhan0009/Relayless
cd Relayless
npm ci
npm run tauri dev        # desktop app with hot reload
npm run dev:ui           # interface only, with demo data, in a browser
```

`npm run dev:ui` replaces the backend with the fake data in `mock/`, which is the fastest way to work on the interface and to take screenshots.

## Making a change

1. For anything larger than a small fix, open an issue first so we can agree on the approach.
2. Create a branch from `main`.
3. Keep commits focused and write messages in the imperative ("Fix resume after source change").
4. Run the checks below.
5. Open a pull request using the template, describing what changed and how you tested it. Screenshots help for UI changes.

### Checks

```sh
cargo test -p relayless-core --release   # engine unit and end-to-end tests
cargo clippy --workspace                 # lints
npm run typecheck                        # TypeScript
npm run build                            # interface build
```

CI runs the same tests on Linux and Windows and builds and launches both apps.

## Guidelines

- **Transfer logic lives in `crates/core`.** The interface only calls commands and renders events.
- **Never trust the network.** Validate every length, offset, index and path from a peer, and keep sender-side authorization on every request.
- **Never claim progress that isn't on disk.** Data is flushed before progress is recorded; keep it that way.
- **Protocol changes** that older peers can't handle need a `PROTOCOL_VERSION` bump in `crates/core/src/proto.rs`. A new request that older peers can safely reject (like `Receipt`) doesn't. Either way, update [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
- **Privacy is the product.** Nothing about files, devices or history may leave the two devices except inside their encrypted connection. No analytics, no crash reporting to third parties, no new network calls without an issue first. If a feature records something, it goes in local history and follows the retention setting.
- **Database changes** need a new schema version and a migration in `crates/core/src/db.rs`; existing installs must keep their downloads.
- **Interface copy** is plain and specific: say what happened and what the user can do, in sentence case. Privacy claims must be checkable in the app (a receipt, a setting, a route badge), never just a slogan.
- **Design:** read [PRODUCT.md](PRODUCT.md) and [DESIGN.md](DESIGN.md) before UI work. Chart colours are fixed there (received white, sent red); text never takes a series colour, and every HUD element must show real data.
- **Tests:** engine changes should come with an end-to-end test in `crates/core/tests/e2e.rs` where practical.

## Releases

Every push to `main` that passes CI publishes a release named `v<version>-build<run>` with the Windows installer and Android APK attached. Bump `version` in `src-tauri/tauri.conf.json`, `Cargo.toml` and `package.json` for a new version, and add an entry to [CHANGELOG.md](CHANGELOG.md).

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
