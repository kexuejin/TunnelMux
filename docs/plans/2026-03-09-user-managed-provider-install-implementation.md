# User-Managed Provider Install Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let TunnelMux install `cloudflared` and `ngrok` into a user-owned tools directory, prefer those binaries at runtime, and expose an in-app install state machine instead of relying on system package managers as the main path.

**Architecture:** Add a Rust-side provider installer module that owns manifest lookup, download, checksum verification, versioned placement, and stable symlink/copy promotion under the GUI app-data tools directory. Update provider probing and daemon launch to prefer local tools, then extend the existing GUI provider-recovery surfaces to show install lifecycle states and invoke the new installer commands.

**Tech Stack:** Tauri 2, Rust, reqwest, tokio, vanilla HTML/CSS/JS, node:test, cargo test

---

### Task 1: Introduce local provider installer state and manifest model

**Files:**
- Create: `crates/tunnelmux-gui/src/provider_installer.rs`
- Modify: `crates/tunnelmux-gui/src/state.rs`
- Modify: `crates/tunnelmux-gui/src/lib.rs`
- Test: `crates/tunnelmux-gui/src/provider_installer.rs`

**Step 1: Write the failing test**

Add tests in `crates/tunnelmux-gui/src/provider_installer.rs` covering:
- tools root path resolution for the current app
- manifest lookup for `cloudflared` and `ngrok`
- default install state is `idle`

**Step 2: Run test to verify it fails**

Run: `cargo test -p tunnelmux-gui provider_installer::tests::manifest_lookup_for_supported_providers -- --exact`

Expected: FAIL because the installer module and manifest model do not exist yet.

**Step 3: Write minimal implementation**

Create `crates/tunnelmux-gui/src/provider_installer.rs` with:
- `ProviderInstallSource`
- `ProviderInstallState`
- `ProviderInstallManifestEntry`
- app-data tools root resolver
- static manifest entries for supported providers

Update `crates/tunnelmux-gui/src/state.rs` so GUI state can hold provider install progress if runtime tracking is needed.

Export the new module from `crates/tunnelmux-gui/src/lib.rs`.

**Step 4: Run test to verify it passes**

Run: `cargo test -p tunnelmux-gui provider_installer::tests::manifest_lookup_for_supported_providers -- --exact`

Expected: PASS

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/src/provider_installer.rs crates/tunnelmux-gui/src/state.rs crates/tunnelmux-gui/src/lib.rs
git commit -m "feat: add provider installer manifest model"
```

### Task 2: Make provider resolution prefer TunnelMux-managed local tools

**Files:**
- Modify: `crates/tunnelmux-gui/src/daemon_manager.rs`
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Modify: `crates/tunnelmux-gui/src/view_models.rs`
- Test: `crates/tunnelmux-gui/src/daemon_manager.rs`
- Test: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Write the failing test**

Add tests covering:
- provider resolution prefers `tools/bin/<provider>` over `/opt/homebrew/bin/<provider>`
- provider availability view model reports `local_tools`, `system_path`, or `missing`

**Step 2: Run test to verify it fails**

Run: `cargo test -p tunnelmux-gui provider_availability_prefers_local_tools`

Expected: FAIL because provider probing only checks PATH/common directories today.

**Step 3: Write minimal implementation**

Update `crates/tunnelmux-gui/src/daemon_manager.rs` to:
- accept extra local tools search directories
- resolve `tools/bin/cloudflared` and `tools/bin/ngrok` before PATH/common directories

Update `crates/tunnelmux-gui/src/commands.rs` and `crates/tunnelmux-gui/src/view_models.rs` so provider availability snapshots include source metadata needed by the UI.

**Step 4: Run test to verify it passes**

Run: `cargo test -p tunnelmux-gui provider_availability_prefers_local_tools`

Expected: PASS

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/src/daemon_manager.rs crates/tunnelmux-gui/src/commands.rs crates/tunnelmux-gui/src/view_models.rs
git commit -m "feat: prefer local provider tools"
```

### Task 3: Implement download, checksum verification, and atomic promotion

**Files:**
- Modify: `crates/tunnelmux-gui/src/provider_installer.rs`
- Test: `crates/tunnelmux-gui/src/provider_installer.rs`

**Step 1: Write the failing test**

Add tests covering:
- download target uses `.tmp/`
- checksum mismatch aborts promotion
- successful install creates a versioned directory and stable `tools/bin/<provider>` target
- a failed install does not replace an existing working binary

**Step 2: Run test to verify it fails**

Run: `cargo test -p tunnelmux-gui provider_installer::tests::checksum_mismatch_does_not_promote_binary -- --exact`

Expected: FAIL because install/promotion logic does not exist yet.

**Step 3: Write minimal implementation**

In `crates/tunnelmux-gui/src/provider_installer.rs` add:
- downloader using `reqwest`
- checksum verifier
- temp-file cleanup
- versioned install directories
- stable launcher path promotion
- executable-bit handling for Unix

**Step 4: Run test to verify it passes**

Run: `cargo test -p tunnelmux-gui provider_installer::tests::checksum_mismatch_does_not_promote_binary -- --exact`

Expected: PASS

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/src/provider_installer.rs
git commit -m "feat: add provider download and promotion flow"
```

### Task 4: Expose installer commands to the GUI

**Files:**
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Modify: `crates/tunnelmux-gui/src/lib.rs`
- Test: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Write the failing test**

Add command-level tests covering:
- `install_provider_to_local_tools` starts an install for `cloudflared`
- `install_provider_to_local_tools` returns progress/status data
- failures map to friendly retryable messages

**Step 2: Run test to verify it fails**

Run: `cargo test -p tunnelmux-gui install_provider_to_local_tools_reports_status`

Expected: FAIL because the command does not exist yet.

**Step 3: Write minimal implementation**

Add Tauri commands for:
- starting provider install
- reading provider install status
- optionally clearing failed status

Register them in `crates/tunnelmux-gui/src/lib.rs`.

**Step 4: Run test to verify it passes**

Run: `cargo test -p tunnelmux-gui install_provider_to_local_tools_reports_status`

Expected: PASS

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/src/commands.rs crates/tunnelmux-gui/src/lib.rs
git commit -m "feat: expose provider installer commands"
```

### Task 5: Replace install CTAs with local install lifecycle UI

**Files:**
- Modify: `crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.test.mjs`

**Step 1: Write the failing test**

Add UI tests covering:
- missing provider uses `Install <provider>` as the primary action
- downloading state disables CTA and shows `Installing…`
- installed local-tools state shows provider-ready copy
- failed installs show `Retry Install`

**Step 2: Run test to verify it fails**

Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`

Expected: FAIL because the UI only knows missing vs installed, not install lifecycle.

**Step 3: Write minimal implementation**

Update the helper and app code so:
- home, empty-state, drawer, and provider-status surfaces all consume the same install state model
- install action invokes the new local installer command
- recheck actions refresh both install status and provider availability
- `Save and Start` remains closed after a successful save even if start is blocked

**Step 4: Run test to verify it passes**

Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`

Expected: PASS

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.test.mjs
git commit -m "feat: add local provider install lifecycle ui"
```

### Task 6: Wire runtime start/restart to local provider installs

**Files:**
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Modify: `crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Test: `crates/tunnelmux-gui/src/commands.rs`
- Test: `crates/tunnelmux-gui/ui/app.test.mjs`

**Step 1: Write the failing test**

Add regressions covering:
- start uses the local tools provider when installed by TunnelMux
- raw daemon missing-binary errors still map back to the same recovery UI
- start no longer falls back to system install prompts when local install succeeds

**Step 2: Run test to verify it fails**

Run: `cargo test -p tunnelmux-gui start_tunnel_uses_local_provider_install_when_available`

Expected: FAIL because start/restart has not been updated to distinguish local install state.

**Step 3: Write minimal implementation**

Update start/restart wiring so:
- local provider installs are treated as provider-ready
- start failures preserve field-level recovery for config issues
- missing local/system providers still route back to install lifecycle UI

**Step 4: Run test to verify it passes**

Run: `cargo test -p tunnelmux-gui start_tunnel_uses_local_provider_install_when_available`

Expected: PASS

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/src/commands.rs crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/app.test.mjs
git commit -m "feat: use local provider installs for tunnel start"
```

### Task 7: Final verification

**Files:**
- Modify: `docs/plans/2026-03-09-user-managed-provider-install-design.md`
- Modify: `docs/plans/2026-03-09-user-managed-provider-install-implementation.md`

**Step 1: Run focused verification**

Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`
Run: `node --check crates/tunnelmux-gui/ui/app.js`
Run: `cargo test -p tunnelmux-gui provider_installer`
Run: `cargo test -p tunnelmux-gui provider_status_summary`

Expected: PASS

**Step 2: Run runtime verification**

Run: `cargo test -p tunnelmux-gui missing_binary_clearly`
Run: `cargo test -p tunnelmux-gui start_tunnel_returns_friendly_error_when_provider_is_missing -- --exact`
Run: `cargo test -p tunnelmux-gui friendly_start_error_when_provider_executable_is_missing -- --exact`

Expected: PASS

**Step 3: Manual smoke check**

Run: `cargo build -p tunnelmux-gui -p tunnelmuxd`

Then verify manually:
- missing provider shows `Install <provider>`
- install completes into the local tools directory
- `Save and Start` works after local install without requiring global PATH changes

**Step 4: Review**

- Request code review on the final diff before claiming completion.
