# GUI Native Bundle Release Design

**Date:** 2026-03-06
**Status:** Approved for implementation

## Context

TunnelMux now ships three binaries through GitHub Releases:

- `tunnelmuxd`
- `tunnelmux-cli`
- `tunnelmux-gui`

The current release workflow packages those as raw platform archives and publishes `SHA256SUMS`. That is sufficient for daemon and CLI consumers, but it leaves a productization gap for the desktop GUI: GUI users still download a raw executable rather than a native installer.

The current repository already contains the basic ingredients needed for native GUI bundling:

- a Tauri GUI crate at `crates/tunnelmux-gui`,
- an existing icon asset at `crates/tunnelmux-gui/icons/icon.png`,
- `bundle.active = true` in `crates/tunnelmux-gui/tauri.conf.json`.

However, the release pipeline and docs explicitly say native GUI installers are not yet built. That means the gap is primarily in release orchestration, bundle metadata, and documentation.

## Goals

- Add native GUI installer assets to GitHub Releases.
- Preserve the current raw binary archives for daemon/CLI users.
- Keep the release pipeline incremental rather than replacing the existing packaging flow.
- Target the highest-value installer formats first:
  - macOS: `dmg`
  - Windows: `msi`
  - Linux: `deb`
- Keep the first iteration small enough to land without introducing signing or auto-update scope.

## Non-Goals

- No macOS notarization or Developer ID signing in this iteration.
- No Windows Authenticode signing in this iteration.
- No Linux multi-format expansion beyond the first chosen native package.
- No Tauri auto-updater feed or update server integration.
- No removal of existing raw binary release archives.
- No redesign of project branding beyond minimum bundle metadata and icon wiring.

## User-Confirmed Scope

This iteration is a **CI-native GUI bundling pass**, not a full commercial distribution system.

Confirmed direction:

- keep existing release archives,
- add native GUI bundle assets alongside them,
- prioritize getting CI bundling working end-to-end,
- defer signing, notarization, and updater work,
- avoid a broad release system rewrite.

## Approaches Considered

### 1. Keep shipping raw binaries only

**Pros**
- No release workflow risk.
- No additional platform bundle complexity.

**Cons**
- GUI installation experience remains weak.
- Productization remains incomplete for desktop users.
- Tauri bundling capabilities stay unused.

**Decision:** Rejected.

### 2. Replace existing archives with installer-only GUI releases

**Pros**
- Cleaner GUI-facing release story.
- Less duplication in release assets.

**Cons**
- Breaks the current archive-based delivery path used by daemon/CLI users.
- Forces all release consumers into a new asset model at once.
- Couples GUI packaging decisions to non-GUI distribution.

**Decision:** Rejected.

### 3. Hybrid release model with raw archives plus native GUI bundles

**Pros**
- Preserves the stable archive flow for daemon/CLI.
- Adds a better desktop installation path for GUI users.
- Minimizes migration risk.
- Fits the current product stage: incremental productization.

**Cons**
- Release page contains more assets.
- CI workflow becomes somewhat more complex.

**Decision:** Recommended.

## Recommended Design

### Release Topology

Keep the existing archive job intact and add a parallel GUI-bundle job.

Recommended release workflow structure:

1. `build` job
   - keep compiling `tunnelmuxd`, `tunnelmux-cli`, and `tunnelmux-gui`
   - keep packaging raw platform archives exactly as today

2. `gui_bundle` job
   - run native Tauri bundle generation for `crates/tunnelmux-gui`
   - target platform-native installer formats only
   - upload those bundle outputs as workflow artifacts

3. `publish` job
   - download both raw archive artifacts and GUI bundle artifacts
   - generate one `SHA256SUMS` file over all final assets
   - publish every asset to the GitHub Release

This keeps GUI packaging additive rather than disruptive.

### Native Targets

The first release iteration should cover:

- macOS Intel: `dmg`
- macOS Apple Silicon: `dmg`
- Windows: `msi`
- Linux: `deb`

This is deliberately narrower than “all Tauri bundle targets.” The point is to get one solid native installer format per primary platform rather than over-expanding scope.

### Tauri Bundle Configuration

`crates/tunnelmux-gui/tauri.conf.json` should be extended from “bundle is active” to “bundle is publishable.”

That means adding at least:

- non-empty `bundle.icon` entries,
- stable product metadata used by native installers,
- bundle configuration that supports the chosen target set cleanly.

The current `icon.png` asset should be wired into the bundling story, and any missing platform icon derivatives should be added if Tauri packaging requires them.

### CI Strategy

The implementation should use the official Tauri bundling path in CI, while keeping the current release flow in control of final publishing.

Practical design constraints:

- GUI bundling should not take over release publishing for the entire repo,
- CLI and daemon archives should continue using the existing packaging path,
- GUI bundle collection should be deterministic and easy to debug.

To keep the workflow maintainable, bundle-artifact collection should be normalized in one place rather than repeated inline across multiple jobs. A small repository script is acceptable if it keeps YAML simpler and easier to validate.

### Asset Naming

Raw archives keep their current names.

GUI installer assets should keep Tauri-native or close-to-native naming rather than being aggressively rewrapped. That reduces coupling to Tauri internals and keeps CI less brittle.

The release page will therefore contain two asset classes:

1. core/raw archives for daemon/CLI/manual users,
2. native GUI installers for desktop users.

### Failure Isolation

The archive and GUI bundle paths should remain logically separated.

That means:
- a GUI bundle failure is easier to diagnose when isolated to its own job,
- archive logic remains readable and stable,
- future signing/notarization work can extend the GUI path without disturbing core archive packaging.

## Error Handling and Risk Management

- Missing signing credentials should not block this iteration because signing is explicitly out of scope.
- Linux bundle generation must continue installing WebKitGTK/Tauri build dependencies.
- Bundle collection must fail clearly if the expected `dmg`, `msi`, or `deb` asset is not produced.
- Documentation must explicitly warn that first-release GUI installers are unsigned.

## Testing and Verification Strategy

This task is mostly release/configuration work, so verification is build- and artifact-oriented rather than unit-test heavy.

Recommended verification layers:

1. **Static verification**
   - JSON validity for `tauri.conf.json`
   - workflow structure sanity checks
   - shell-script syntax checks if a helper script is added

2. **Project verification**
   - `cargo check -p tunnelmux-gui`
   - `cargo test --workspace --quiet`

3. **Host-platform smoke verification**
   - on the current machine, run one local native bundle build for the local platform if tooling is available
   - confirm the expected installer file appears in the Tauri bundle output directory

The first iteration should not claim cross-platform installer success beyond what is actually verified in CI or on a matching host.

## Delivery Plan

Recommended commit sequence:

1. `build: add GUI bundle metadata and icon wiring`
2. `build: add GUI bundle artifact collection helpers`
3. `ci: add native GUI bundle release job`
4. `docs: document GUI installer assets`

## Deferred Follow-Ups

These are intentionally deferred until after native bundle release assets land:

- macOS code signing and notarization
- Windows code signing
- Linux multi-format bundles (`rpm`, `AppImage`, etc.)
- Tauri updater metadata and release channels
- installer post-install validation on all target platforms
