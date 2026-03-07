# Cloudflared Named Tunnel Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add token-backed `cloudflared` named tunnel support while preserving quick tunnel as the default one-click path.

**Architecture:** The GUI persists an optional Cloudflare tunnel token and only exposes the field when `cloudflared` is the selected provider. The GUI forwards provider metadata through the existing tunnel start request, and `tunnelmuxd` switches the `cloudflared` spawn command based on whether token metadata exists.

**Tech Stack:** Tauri 2, Rust, vanilla HTML/CSS/JS

---

### Task 1: Persist Cloudflare token in GUI settings

**Files:**
- Modify: `crates/tunnelmux-gui/src/settings.rs`
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing test**
- Add a settings round-trip assertion for `cloudflared_tunnel_token`.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui save_and_reload_settings_round_trips_token`

Expected: missing field in the settings schema.

**Step 3: Write minimal implementation**
- Add `cloudflared_tunnel_token` to `GuiSettings`.
- Normalize it like the existing token-style fields.
- Add a provider-conditional input in the settings drawer.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui`

Expected: settings tests pass and GUI shell still builds.

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/src/settings.rs crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js
git commit -m "feat: persist cloudflared tunnel token"
```

### Task 2: Forward provider metadata from GUI

**Files:**
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Test: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Write the failing test**
- Add a `start_tunnel` test proving Cloudflare token metadata is forwarded.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui start_tunnel_uses_saved_cloudflared_tunnel_token`

Expected: metadata is absent.

**Step 3: Write minimal implementation**
- Extend `build_tunnel_metadata` to include `cloudflaredTunnelToken`.
- Keep the existing ngrok metadata behavior unchanged.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui`

Expected: metadata forwarding works and no regressions appear.

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/src/commands.rs
git commit -m "feat: send cloudflared token metadata"
```

### Task 3: Spawn cloudflared in named tunnel mode

**Files:**
- Modify: `crates/tunnelmuxd/src/runtime.rs`
- Test: `crates/tunnelmuxd/src/runtime.rs` or `crates/tunnelmuxd/src/main.rs`

**Step 1: Write the failing test**
- Add a command-construction regression showing token metadata should produce `cloudflared tunnel ... run --token ... --url ...`.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmuxd cloudflared`

Expected: current runtime still builds the quick tunnel command only.

**Step 3: Write minimal implementation**
- Switch the `cloudflared` branch in provider spawn logic:
  - token present => named tunnel command
  - token absent => existing quick tunnel command

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmuxd`

Expected: the new command test and the existing daemon suite pass.

**Step 5: Commit**
```bash
git add crates/tunnelmuxd/src/runtime.rs crates/tunnelmuxd/src/main.rs
git commit -m "feat: support cloudflared named tunnel tokens"
```

### Task 4: Final verification

**Files:**
- Modify: `docs/plans/2026-03-07-cloudflared-named-tunnel-design.md`
- Modify: `docs/plans/2026-03-07-cloudflared-named-tunnel-implementation.md`

**Step 1: Write the failing test**
- Add any final regression coverage discovered during integration.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui && cargo test -p tunnelmuxd`

Expected: any missing integration gaps are visible before the last implementation pass.

**Step 3: Write minimal implementation**
- Tighten UI copy and status behavior for token mode.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui && cargo test -p tunnelmuxd && node --check crates/tunnelmux-gui/ui/app.js`

Expected: all tests pass and frontend syntax check succeeds.

**Step 5: Commit**
```bash
git add docs/plans crates/tunnelmux-gui crates/tunnelmuxd
git commit -m "feat: add cloudflared named tunnel mode"
```
