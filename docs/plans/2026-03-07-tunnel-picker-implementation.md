# Tunnel Picker Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the native tunnel select with a custom tunnel picker that exposes tunnel state and service counts clearly.

**Architecture:** Reuse the existing tunnel workspace VM, keep the current tunnel bar as the anchor, and add a small popover component in the static HTML/JS shell. The picker remains entirely client-side and calls existing tunnel-switch commands.

**Tech Stack:** Rust, Tauri 2, vanilla HTML/CSS/JS

---

### Task 1: Finalize tunnel workspace VM fields

**Files:**
- Modify: `crates/tunnelmux-gui/src/view_models.rs`
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Test: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Write the failing test**
- Extend the tunnel workspace merge test to assert the picker-visible fields are present.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui load_tunnel_workspace_merges_daemon_summary_when_available`

Expected: FAIL on missing picker summary fields.

**Step 3: Write minimal implementation**
- Ensure `TunnelProfileVm` contains all fields the picker needs.
- Ensure workspace loading merges daemon summary values into the VM.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui load_tunnel_workspace_merges_daemon_summary_when_available`

Expected: PASS

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/src/view_models.rs crates/tunnelmux-gui/src/commands.rs
git commit -m "feat: expose tunnel picker workspace summary"
```

### Task 2: Add custom tunnel picker shell

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing test**
- Add a small JS-level assertion path if practical, otherwise use `node --check` as the red-green guard for the structural change.

**Step 2: Run test to verify it fails**
Run: `node --check crates/tunnelmux-gui/ui/app.js`

Expected: FAIL until the new picker handlers and state are wired correctly.

**Step 3: Write minimal implementation**
- Replace the native `<select>` affordance with:
  - trigger button
  - popover list shell
  - selected-row styling
- Keep `New Tunnel` and `Edit Tunnel` in place.

**Step 4: Run test to verify it passes**
Run: `node --check crates/tunnelmux-gui/ui/app.js`

Expected: PASS

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/ui/app.js
git commit -m "feat: add custom tunnel picker shell"
```

### Task 3: Wire picker open/close and switching

**Files:**
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing test**
- Reuse the existing tunnel workspace and switch flow expectations as the behavior guard.

**Step 2: Run test to verify it fails**
Run: `node --check crates/tunnelmux-gui/ui/app.js`

Expected: FAIL if picker actions are incomplete or syntactically invalid.

**Step 3: Write minimal implementation**
- Add picker state to the client shell.
- Open on trigger click.
- Close on outside click and `Esc`.
- Switch tunnel on row click.
- Reuse current switch command and refresh flow.

**Step 4: Run test to verify it passes**
Run: `node --check crates/tunnelmux-gui/ui/app.js`

Expected: PASS

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/ui/app.js
git commit -m "feat: wire tunnel picker switching"
```

### Task 4: Polish current tunnel summary and copy

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Test: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Write the failing test**
- Add or extend a workspace test to cover summary fields the top bar depends on.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui load_tunnel_workspace_returns_single_current_tunnel_when_configured`

Expected: FAIL if summary assumptions no longer match the UI model.

**Step 3: Write minimal implementation**
- Surface public URL summary when available.
- Keep copy concise in empty states.
- Keep state/service count readable in the current tunnel bar.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui load_tunnel_workspace_ && node --check crates/tunnelmux-gui/ui/app.js`

Expected: PASS

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/src/commands.rs
git commit -m "feat: polish tunnel picker summary"
```

### Task 5: Final verification

**Files:**
- Verify only

**Step 1: Run focused GUI verification**
Run: `cargo test -p tunnelmux-gui`

Expected: PASS

**Step 2: Run dependent package verification**
Run: `cargo test -p tunnelmux-control-client && cargo test -p tunnelmux-cli`

Expected: PASS

**Step 3: Run front-end syntax verification**
Run: `node --check crates/tunnelmux-gui/ui/app.js`

Expected: PASS

**Step 4: Commit**
```bash
git status --short
```

Expected: clean working tree

