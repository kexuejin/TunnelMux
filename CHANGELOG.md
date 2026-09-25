# Changelog

All notable changes to this project should be documented in this file.

## [Unreleased]

## [0.4.0] - 2026-09-25

- Merge `tunnelmuxd` into the desktop app as an in-process library. The GUI now owns one control port, state directory, API token, and provider process tree; an already-running daemon is adopted and left untouched, while an embedded daemon and its tunnels stop cleanly with the app.
- Remove the daemon sidecar from GUI builds and release packaging while keeping the standalone `tunnelmuxd` and `tunnelmux-cli` entry points.
- Rebuild the desktop console around Overview, Services, Tunnels, Diagnostics, and Settings views, with a persistent sidebar, clearer primary actions, semantic light/dark themes, and matching public access/welcome pages.
- Make tunnel-scoped operations consistent across the daemon, GUI, and CLI. Route access state is isolated by tunnel and route, duplicate route IDs no longer collide across tunnels, and delete/replace operations clean up stale gates.
- Add unified CLI tunnel selection with `--tunnel-id`, named Cloudflare readiness without a public-URL requirement, safer route/tunnel ID validation, and Unicode-safe table truncation.
- Harden the control plane: local API-token discovery is restricted to loopback URLs, authentication endpoints require the control bearer token, health probes omit credentials, and gateway requests strip route-gate authorization/cookie material before reaching an upstream.
- Remove API-level provider executable overrides. Provider binaries now come only from daemon startup configuration, with embedded GUI startup resolving the local tools or system installation.
- Store GUI control and provider tokens in the platform credential store, migrate legacy plaintext fields on load, and keep non-secret configuration in `settings.json`.
- Add per-tunnel operation generations and cancellation so stop/delete/shutdown cannot race an in-flight start or revive a provider after shutdown; monitor, gateway, SSE, and WebSocket tasks now participate in coordinated teardown.
- Make state, settings, and API-token writes atomic and owner-only on Unix, add single-writer protection, and surface persistence failures instead of reporting a successful write.
- Harden gateway forwarding with upstream identity/header filtering, encoded-body safeguards, route snapshots, bounded health checks, rate limiting, streaming/WebSocket support, and mounted-app rewriting for both `/api` and `/plugins` JavaScript literals.
- Expand the desktop updater to verified `.tar.gz` and `.zip` raw archives, require SHA-256, enforce semver/basename/size/timeout limits, add the Windows delayed-replacement helper, and direct native `.dmg`/`.msi`/`.deb` installations to the platform package flow.
- Fix provider log rotation, custom data-file/token path handling, production settings reload, cloudflared transport auto-negotiation, and access-code TTL preservation.
- Rework release and CI gates: sidecar-free release config, least-privilege workflow permissions, shell/Node/GUI checks, release-manifest verification, updater packaging tests, and RustSec auditing.
- Publish a bilingual GitHub Pages documentation site with FAQ, API, architecture, integration, SEO, and launch material.

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
