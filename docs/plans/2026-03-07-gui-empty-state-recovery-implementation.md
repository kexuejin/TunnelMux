# GUI Empty State and Recovery Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the services empty-state crash, add a built-in no-routes onboarding fallback, and keep the daemon alive across GUI restarts so reopening the app reconnects cleanly.

**Architecture:** The GUI shell remains single-page. Empty-state rendering is fixed in the frontend and supported with explicit snapshot messaging from the GUI backend. The daemon keeps serving independently of the GUI process, and a lightweight built-in fallback page is served only when no user routes are configured.

**Tech Stack:** Tauri 2, Rust, Axum, vanilla HTML/CSS/JS

---

### Task 1: Fix GUI empty-state rendering

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing test**

- Add a focused test path in the existing GUI logic or a minimal regression assertion around empty route rendering.
- Cover:
  - no exception when `routes=[]`;
  - `Add Service` stays visible;
  - onboarding copy is rendered.

**Step 2: Run test to verify it fails**

Run: `cargo test -p tunnelmux-gui`

Expected: failure or current missing-coverage gap for the empty-state rendering regression.

**Step 3: Write minimal implementation**

- Remove references to deleted DOM nodes.
- Introduce a safe empty-state render branch.
- Keep route action wiring unchanged for non-empty route lists.

**Step 4: Run test to verify it passes**

Run: `cargo test -p tunnelmux-gui`

Expected: new regression passes and existing GUI tests remain green.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js
git commit -m "fix: harden gui services empty state"
```

### Task 2: Add no-routes onboarding fallback

**Files:**
- Modify: `crates/tunnelmuxd/src/main.rs`
- Modify: `crates/tunnelmuxd/src/api.rs`
- Modify: `crates/tunnelmux-gui/src/view_models.rs`
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Test: `crates/tunnelmuxd/src/main.rs`

**Step 1: Write the failing test**

- Add a daemon-side test proving that requests to the gateway with zero user routes return a built-in welcome page instead of a generic failure.
- Add a GUI-side assertion, if needed, that zero routes produce onboarding messaging instead of a blank/error state.

**Step 2: Run test to verify it fails**

Run: `cargo test -p tunnelmuxd no_routes`

Expected: the gateway does not yet serve the built-in welcome content.

**Step 3: Write minimal implementation**

- Serve a small built-in HTML welcome page when no user routes match and there are zero configured routes.
- Keep this fallback out of persisted route data and out of editable service lists.
- Add snapshot/message copy that tells the GUI why the empty state exists.

**Step 4: Run test to verify it passes**

Run: `cargo test -p tunnelmuxd`

Expected: fallback test passes and daemon tests stay green.

**Step 5: Commit**

```bash
git add crates/tunnelmuxd/src/main.rs crates/tunnelmuxd/src/api.rs crates/tunnelmux-gui/src/view_models.rs crates/tunnelmux-gui/src/commands.rs
git commit -m "feat: add welcome fallback for empty routes"
```

### Task 3: Decouple daemon lifetime from GUI exit

**Files:**
- Modify: `crates/tunnelmux-gui/src/lib.rs`
- Modify: `crates/tunnelmux-gui/src/daemon_manager.rs`
- Test: `crates/tunnelmux-gui/src/daemon_manager.rs`

**Step 1: Write the failing test**

- Add a regression test for the desired reconnect semantics:
  - a reachable daemon on startup is treated as active;
  - GUI shutdown no longer tears down daemon ownership state as part of normal exit.

**Step 2: Run test to verify it fails**

Run: `cargo test -p tunnelmux-gui daemon_manager`

Expected: current shutdown behavior still assumes GUI-owned daemon teardown.

**Step 3: Write minimal implementation**

- Remove the GUI exit hook that stops the managed daemon.
- Keep startup bootstrap intact.
- Preserve reconnect behavior when the daemon is already reachable.

**Step 4: Run test to verify it passes**

Run: `cargo test -p tunnelmux-gui`

Expected: reconnect behavior passes and daemon bootstrap tests remain green.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/src/lib.rs crates/tunnelmux-gui/src/daemon_manager.rs
git commit -m "feat: preserve daemon across gui restarts"
```

### Task 4: End-to-end verification

**Files:**
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Modify: `crates/tunnelmux-gui/src/view_models.rs`
- Modify: `crates/tunnelmuxd/src/main.rs`

**Step 1: Write the failing test**

- Add any missing regression coverage discovered during integration.

**Step 2: Run test to verify it fails**

Run: `cargo test -p tunnelmux-gui && cargo test -p tunnelmuxd`

Expected: any missing integration coverage fails before the last implementation pass.

**Step 3: Write minimal implementation**

- Tighten copy, onboarding messages, and status transitions to match the approved design.

**Step 4: Run test to verify it passes**

Run: `cargo test -p tunnelmux-gui && cargo test -p tunnelmuxd && node --check crates/tunnelmux-gui/ui/app.js`

Expected: all tests pass and frontend syntax check succeeds.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui crates/tunnelmuxd
git commit -m "feat: improve gui empty state and recovery"
```
