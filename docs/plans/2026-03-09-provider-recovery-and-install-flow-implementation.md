# Provider Recovery And Install Flow Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Keep provider-missing recovery inside the GUI, prevent duplicate tunnel creation from the create drawer, and add one-click provider install actions for desktop users.

**Architecture:** Keep provider-missing detection in the GUI as the first guard, then add a second guard that rewrites raw daemon missing-binary errors back into the same provider recovery model. Treat `Save and Start` as a two-phase flow: save first, then either close on successful or blocked start, reopening only for true field-level recovery errors. Add a Rust-side installer command so the GUI can launch a platform-native install flow for both `cloudflared` and `ngrok` instead of only copying shell commands.

**Tech Stack:** Tauri 2, Rust, vanilla HTML/CSS/JS, node:test, cargo test

---

### Task 1: Lock in failing recovery and drawer behavior tests

**Files:**
- Modify: `crates/tunnelmux-gui/ui/app.test.mjs`
- Modify: `crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs`
- Modify: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Write the failing tests**
- Add a helper-level regression showing raw daemon `provider executable not found` errors must preserve provider recovery.
- Add frontend source assertions showing save-then-blocked-start closes the create drawer instead of reopening edit mode.
- Add installer guidance assertions for the new one-click provider action path.

**Step 2: Run test to verify it fails**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`
Expected: assertions fail because the current create flow reopens the drawer and no one-click install path exists.

**Step 3: Write minimal implementation**
- Update recovery helpers and app flow only enough to satisfy the new failure cases.

**Step 4: Run test to verify it passes**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`
Expected: UI shell assertions pass.

### Task 2: Keep start/restart missing-provider errors friendly

**Files:**
- Modify: `crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs`
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Test: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Write the failing test**
- Add a Rust regression that maps raw daemon `provider executable not found: cloudflared` errors to the existing friendly install guidance.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui commands::tests::friendly_start_error_when_provider_executable_is_missing -- --exact`
Expected: missing-binary daemon errors are not rewritten yet.

**Step 3: Write minimal implementation**
- Teach GUI recovery helpers and Rust `friendly_start_error` to recognize daemon missing-provider text.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui commands::tests::friendly_start_error_when_provider_executable_is_missing -- --exact`
Expected: provider-missing errors stay on the friendly recovery path.

### Task 3: Prevent duplicate create-on-reclick after blocked start

**Files:**
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/app.test.mjs`

**Step 1: Write the failing test**
- Add a source assertion showing `saveTunnel({ startNow: true })` closes the drawer after a successful save when the start failure is a provider recovery case with no field-level edit target.

**Step 2: Run test to verify it fails**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`
Expected: current flow reopens edit mode and leaves the create affordance in a duplicate-creation state.

**Step 3: Write minimal implementation**
- Close the drawer after save succeeds if the only failure is provider recovery or another non-field error.
- Keep reopening edit mode only for actual recovery targets like token or URL fields.

**Step 4: Run test to verify it passes**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`
Expected: save-and-start blocked states no longer leave the create drawer open.

### Task 4: Add one-click provider install

**Files:**
- Modify: `crates/tunnelmux-gui/Cargo.toml`
- Modify: `crates/tunnelmux-gui/capabilities/default.json`
- Modify: `crates/tunnelmux-gui/src/lib.rs`
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Modify: `crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/app.test.mjs`

**Step 1: Write the failing test**
- Add helper assertions that missing-provider states surface `Install Provider` actions instead of copy-only text.
- Add Rust tests for the install command builder.

**Step 2: Run test to verify it fails**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`
Run: `cargo test -p tunnelmux-gui provider_install_action -- --nocapture`
Expected: no install command exists yet.

**Step 3: Write minimal implementation**
- Add a Tauri command that starts the platform installer flow:
  - macOS: open Terminal on the correct package manager command
  - Windows: open PowerShell on the correct `winget` command
  - Linux: open Terminal when possible, otherwise open provider docs
- Wire missing-provider UI to use install actions first and keep provider switch or recheck as follow-up.

**Step 4: Run test to verify it passes**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`
Run: `cargo test -p tunnelmux-gui provider_install_action`
Expected: helper logic and Rust installer tests pass.

### Task 5: Final verification

**Files:**
- Modify: `docs/plans/2026-03-09-provider-recovery-and-install-flow-implementation.md`

**Step 1: Run targeted verification**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`
Run: `node --check crates/tunnelmux-gui/ui/app.js`
Run: `cargo test -p tunnelmux-gui commands::tests::friendly_start_error_when_provider_executable_is_missing -- --exact`
Run: `cargo test -p tunnelmux-gui start_tunnel_returns_friendly_error_when_provider_is_missing -- --exact`

**Step 2: Run broader verification**
Run: `cargo test -p tunnelmux-gui provider_status_summary`
Run: `cargo test -p tunnelmux-gui missing_binary_clearly`

**Step 3: Review**
- Request code review on the final diff before claiming completion.
