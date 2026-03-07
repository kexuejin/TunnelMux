# Multi-Tunnel Runtime Phase 2 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Upgrade `tunnelmuxd` from a single active tunnel runtime to multiple independent tunnel workers under one daemon.

**Architecture:** Replace the single `running_tunnel` / `pending_restart` state with per-tunnel runtime maps, make routes tunnel-scoped, and update lifecycle APIs to require explicit tunnel ids. The GUI continues to operate on the currently selected tunnel.

**Tech Stack:** Rust, Axum, Tauri 2, vanilla HTML/CSS/JS

---

### Task 1: Introduce tunnel-scoped core models

**Files:**
- Modify: `crates/tunnelmux-core/src/lib.rs`
- Test: `crates/tunnelmuxd/src/main.rs`

**Step 1: Write the failing test**
- Add tests for tunnel-scoped route/tunnel workspace structures and lifecycle requests with explicit `tunnel_id`.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmuxd tunnel_`

Expected: current core models are still single-tunnel oriented.

**Step 3: Write minimal implementation**
- Add explicit tunnel ids to lifecycle and route request/response models.
- Add tunnel-scoped summaries for runtime/workspace responses.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmuxd`

Expected: new model tests and daemon suite pass.

**Step 5: Commit**
```bash
git add crates/tunnelmux-core/src/lib.rs crates/tunnelmuxd/src/main.rs
git commit -m "feat: add tunnel-scoped core models"
```

### Task 2: Replace single runtime with tunnel runtime map

**Files:**
- Modify: `crates/tunnelmuxd/src/main.rs`
- Modify: `crates/tunnelmuxd/src/runtime.rs`
- Modify: `crates/tunnelmuxd/src/persistence.rs`

**Step 1: Write the failing test**
- Add a runtime test proving two tunnels can exist independently in daemon state.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmuxd runtime`

Expected: current daemon only supports one runtime slot.

**Step 3: Write minimal implementation**
- Replace `running_tunnel` and `pending_restart` with per-tunnel maps.
- Persist tunnel-scoped runtime state.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmuxd`

Expected: runtime tests and daemon suite pass.

**Step 5: Commit**
```bash
git add crates/tunnelmuxd/src/main.rs crates/tunnelmuxd/src/runtime.rs crates/tunnelmuxd/src/persistence.rs
git commit -m "feat: support tunnel-scoped runtime state"
```

### Task 3: Make routes tunnel-scoped

**Files:**
- Modify: `crates/tunnelmux-core/src/lib.rs`
- Modify: `crates/tunnelmuxd/src/api.rs`
- Modify: `crates/tunnelmuxd/src/gateway.rs`
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Modify: `crates/tunnelmux-gui/src/view_models.rs`

**Step 1: Write the failing test**
- Add tests showing routes are isolated by `tunnel_id`.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmuxd routes_`

Expected: current route model is global.

**Step 3: Write minimal implementation**
- Add `tunnel_id` to routes.
- Filter gateway and route APIs by tunnel context.
- Make GUI route CRUD always use the current tunnel.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmuxd && cargo test -p tunnelmux-gui`

Expected: route isolation works and GUI tests stay green.

**Step 5: Commit**
```bash
git add crates/tunnelmux-core/src/lib.rs crates/tunnelmuxd/src/api.rs crates/tunnelmuxd/src/gateway.rs crates/tunnelmux-gui/src/commands.rs crates/tunnelmux-gui/src/view_models.rs
git commit -m "feat: scope services to tunnels"
```

### Task 4: Add tunnel-scoped lifecycle APIs

**Files:**
- Modify: `crates/tunnelmuxd/src/api.rs`
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing test**
- Add tests showing start/stop/restart are isolated by tunnel id.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmuxd start_tunnel`

Expected: current lifecycle APIs are global.

**Step 3: Write minimal implementation**
- Require tunnel id in tunnel lifecycle operations.
- Update GUI tunnel actions to operate on the selected tunnel.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmuxd && cargo test -p tunnelmux-gui && node --check crates/tunnelmux-gui/ui/app.js`

Expected: tunnel lifecycle is isolated and GUI remains valid.

**Step 5: Commit**
```bash
git add crates/tunnelmuxd/src/api.rs crates/tunnelmux-gui/src/commands.rs crates/tunnelmux-gui/ui/app.js
git commit -m "feat: add tunnel-scoped lifecycle actions"
```
