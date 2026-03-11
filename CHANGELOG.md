# Changelog

All notable changes to this project should be documented in this file.

## [Unreleased]

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
