# Changelog

All notable changes to this project should be documented in this file.

## [Unreleased]

- Merge the daemon into the desktop app: `tunnelmuxd` now builds as a library plus a thin CLI binary, and the GUI links the library and hosts the daemon in its own process. One process owns the control port, the state files, the api token, and the provider child processes.
- Stop shipping a bundled `tunnelmuxd` sidecar in the GUI bundle. Remove the `externalBin` Tauri config, the staging script, and the CI staging steps that produced it.
- Remove the GUI's "spawn a daemon" path entirely, along with bundled/`PATH` binary resolution, PID tracking, and readiness polling. The daemon library returns once its listeners are bound, so there is nothing to poll for.
- Keep daemon ownership explicit in the GUI: an already-answering daemon is adopted as `external` (ownership value `managed` is replaced by `embedded`) and never stopped; otherwise the app starts the embedded daemon and stops it on exit.
- Stop the embedded daemon's tunnels and provider processes on app exit, and prevent the runtime monitor from restarting them mid-shutdown.
- Fix a start-failure bug that rotated the shared api token: the daemon now binds its listeners before writing anything to the data directory, and reuses an existing token instead of minting a new one on every start. A failed start can no longer lock out already-running clients.
- Default daemon logging to `info` when `RUST_LOG` is unset, so listener and provider lifecycle lines are visible without extra setup.
- Pick the rustls crypto provider explicitly (`ring`) before any TLS client is built, so the daemon no longer aborts at startup when `ring` and `aws-lc-rs` are linked into the same build — which is exactly what happens once the desktop app hosts the daemon.
- Serve `POST /v1/settings/reload` in production builds. The handler, its tests, and the CLI command all existed, but the route was only registered in the test router, so the CLI's settings reload always missed the running daemon.
- Rotate the provider log. `provider.log` now rolls over at 16 MiB into `provider.log.1…N` (three backups by default) instead of growing without bound, tunable with `--provider-log-max-bytes` (`0` disables rotation) and `--provider-log-max-files` (`0` truncates in place). All tunnels share one sink, so rotation cannot leave a reader appending to a file that was renamed away.
- Derive the api token file from the state file instead of hardcoding `~/.tunnelmux`. A daemon started with a custom `--data-file` now keeps its token alongside it and can no longer rotate the token local clients discover on the default path; `--api-token-file` overrides the location explicitly. The default layout is unchanged.
- Mounted-app response rewriting now covers the `/plugins` namespace in JavaScript string literals, not just `/api`. The DSH client reads its dev-channel endpoint from a quoted literal (`const EVENTS_ENDPOINT = "/plugins/events"`), so under a path mount the browser asked the tunnel host for `/plugins/events` and got a 404 — hot module reload was dead, and a rebuilt plugin left open pages holding a stale bundle revision with nothing to tell them. The rewrite stays a whole-segment whitelist: `/plugin` and `/plugins2` are untouched, and so is every other quoted root-absolute literal in the bundle (PDF content streams, Emscripten paths, prose), which a blanket rule would corrupt.

## [0.3.0] - 2026-08-24

- Add gateway service access gates with a global default code, per-service inherit/custom/public modes, and route-scoped browser cookies.
- Add a polished route access login page with cache-clearing headers for mounted web apps.
- Add in-app update checking and SHA256-verified raw archive installation from GitHub Releases, now preferring the static `tunnelmux-latest.json` manifest before GitHub API fallback.
- Add updater install confirmation, asset/SHA display, and a Restart Now action after install.
- Add GUI controls for default service access, per-service gate modes, generated/copyable access codes, route smoke tests, and update checks.
- Add a DeepSeek / mounted-SPA preset plus root-path exposure hints on service cards.
- Add English / Simplified Chinese UI switching with Auto system-language detection and local persistence.
- Polish the Settings and service drawers with clearer section hierarchy and wider drawer spacing.
- Harden mounted app forwarding by stripping external Host/Origin when routes do not forward the original Host header.
- Document the in-app updater and service access gate workflow.

## [0.2.1] - 2026-03-11

- Add a system tray icon to the GUI, with close-to-hide behavior and a minimal Show/Hide + Quit menu.

## [0.2.0] - 2026-03-08

- Add true tunnel-scoped daemon runtime state so multiple tunnel profiles can coexist with independent status, restart, and route ownership.
- Add tunnel-scoped control-plane APIs for routes, logs, diagnostics, dashboard, metrics, and upstream health.
- Add per-tunnel gateway listener management plus cleanup on tunnel stop and hard delete.
- Add daemon-side hard tunnel deletion and wire GUI tunnel deletion through daemon cleanup before local settings removal.
- Refine the GUI around a tunnel-first model with current-tunnel status summaries, custom tunnel picker, in-app delete confirmation, and quieter passive status messaging.

## [0.1.5] - 2026-03-06

- Redesign the GUI into a single-page easy-first shell with the public URL, tunnel actions, and the service list on one screen.
- Move service add/edit into a side drawer and move settings behind a top-right settings entry to reduce default UI complexity.
- Add Rust-side daemon ownership logic so the GUI can auto-start a local `tunnelmuxd`, prefer bundled binaries, and avoid stopping externally managed daemons.
- Bundle `tunnelmuxd` into native GUI installer workflows and validate the new GUI bundle path through GitHub release rehearsal runs.
- Refine GUI release workflow config injection so bundled daemon assets resolve correctly during cross-platform Tauri packaging.

## [0.1.4] - 2026-03-06

- Add desktop GUI product surface for dashboard, route management, tunnel controls, and diagnostics.
- Add declarative config reload support plus updated API/runtime documentation for reload and diagnostics flows.
- Add native GUI installer packaging for macOS (`.dmg`), Windows (`.msi`), and Linux (`.deb`) in the release workflow.
- Add GUI release signing preflight for macOS and Windows, including temporary workflow wiring for Apple notarization inputs and Trusted Signing config generation.
- Add manual `workflow_dispatch` release rehearsal mode with artifact-only publishing, version validation, and documented operator steps.
- Fix release workflow follow-ups discovered during GitHub rehearsal, including the Tauri action ref and unsigned macOS signing environment scoping.

## [0.1.3] - 2026-03-05

- Add `scripts/install.sh` for one-command install from GitHub Releases (macOS/Linux).
- Add CI shell syntax check for installer script.
- Rewrite README and core docs as professional English-first documentation.
- Normalize integration docs to a generic third-party model.
- Remove mixed-language sections from primary docs.

## [0.1.2] - 2026-03-05

- Sync `Cargo.lock` with workspace version bump so `--locked` release builds pass.
- Keep GitHub Actions release matrix compatible with current macOS Intel runner labels.

## [0.1.1] - 2026-03-05

- Add open-source repository baseline (governance docs and issue/PR templates).
- Add GitHub Actions CI and tag-based release packaging workflows.
- Add checksum (`SHA256SUMS`) generation for release assets.
- Fix macOS Intel release runner label (`macos-15-intel`).
- Expand release and installation documentation.

## [0.1.0] - 2026-03-05

- Initial public release of TunnelMux core/daemon/CLI.
- Tunnel lifecycle API (`start`, `stop`, `status`).
- Route management and gateway forwarding.
- Provider supervision and restart strategy.
- Third-party integration documentation.
