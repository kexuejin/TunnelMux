# Multi-Tunnel Phase 1 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Convert the product and GUI model to tunnel-first with explicit tunnel creation/editing, while preserving a simple default experience for single-tunnel users.

**Architecture:** Introduce tunnel-profile structures in the GUI-facing model and adapt the shell to operate on a current tunnel context instead of global provider settings. This phase focuses on data model and flow; true concurrent multi-tunnel runtime is deferred to Phase 2.

**Tech Stack:** Tauri 2, Rust, vanilla HTML/CSS/JS

---

### Task 1: Add tunnel-profile view models and empty-state flow

**Files:**
- Modify: `crates/tunnelmux-gui/src/view_models.rs`
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing test**
- Add GUI-facing tests for a tunnelless state and tunnel-profile metadata where appropriate.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui`

Expected: missing tunnel-profile types and no tunnelless current-context flow.

**Step 3: Write minimal implementation**
- Introduce tunnel-profile view models.
- Render `Create Tunnel` empty state instead of a service-first shell when there is no current tunnel.
- Keep existing single-tunnel rendering unchanged when one tunnel is active.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui`

Expected: new model tests and existing GUI tests pass.

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/src/view_models.rs crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js
git commit -m "feat: add tunnel-first empty state"
```

### Task 2: Add Create/Edit Tunnel dialog skeleton

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/styles.css`

**Step 1: Write the failing test**
- Add focused assertions for provider-specific fields and dialog state helpers where possible.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui`

Expected: no Create/Edit Tunnel flow exists.

**Step 3: Write minimal implementation**
- Add the modal/drawer skeleton for `Create Tunnel`.
- Support:
  - name
  - provider
  - quick-start defaults
  - provider-specific advanced fields
- Reuse the same structure for future `Edit Tunnel`.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui && node --check crates/tunnelmux-gui/ui/app.js`

Expected: shell remains valid and dialog state logic passes.

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/styles.css
git commit -m "feat: scaffold create tunnel dialog"
```

### Task 3: Move tunnel-local config out of Settings

**Files:**
- Modify: `crates/tunnelmux-gui/src/settings.rs`
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing test**
- Add tests that capture which settings remain application-level versus tunnel-level.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui`

Expected: current settings schema still mixes app and tunnel concerns.

**Step 3: Write minimal implementation**
- Reduce global Settings to app-level configuration only.
- Keep tunnel/provider config in the new tunnel dialog flow.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui`

Expected: updated settings semantics pass and no unrelated regressions appear.

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/src/settings.rs crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js
git commit -m "refactor: separate app settings from tunnel config"
```

### Task 4: Prepare daemon and API boundaries for tunnel profiles

**Files:**
- Modify: `crates/tunnelmuxd/src/main.rs`
- Modify: `crates/tunnelmuxd/src/api.rs`
- Modify: `crates/tunnelmux-core/src/lib.rs`

**Step 1: Write the failing test**
- Add contract-level tests for tunnel-profile structures and current-tunnel selection semantics.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmuxd`

Expected: daemon/core still expose only a single global tunnel model.

**Step 3: Write minimal implementation**
- Add tunnel-profile-compatible core structures and API placeholders without full concurrent runtime yet.
- Preserve backward compatibility where practical during the transition.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmuxd`

Expected: daemon/core tests pass and future Phase 2 hooks are in place.

**Step 5: Commit**
```bash
git add crates/tunnelmuxd/src/main.rs crates/tunnelmuxd/src/api.rs crates/tunnelmux-core/src/lib.rs
git commit -m "feat: prepare tunnel profile data model"
```
