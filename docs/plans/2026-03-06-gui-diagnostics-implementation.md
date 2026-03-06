# GUI Diagnostics Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a polling-based diagnostics workspace to the Tauri GUI so operators can inspect runtime summary, upstream health, and recent provider logs without leaving the desktop app.

**Architecture:** Extend the existing GUI stack instead of adding a parallel path. The frontend adds a `Diagnostics` workspace and thin polling controller; new Tauri commands in `crates/tunnelmux-gui` call the shared `tunnelmux-control-client`; the shared client gains a small non-streaming log-tail helper for `GET /v1/tunnel/logs`.

**Tech Stack:** Rust workspace crates, Tauri, plain HTML/CSS/JS, `reqwest`, `tokio`, `axum` test servers, existing `tunnelmux-core` API models.

---

### Task 1: Add shared client support for log-tail retrieval

**Files:**
- Modify: `crates/tunnelmux-control-client/src/lib.rs`
- Test: `crates/tunnelmux-control-client/src/lib.rs`

**Step 1: Write the failing tests**

Add tests in `crates/tunnelmux-control-client/src/lib.rs` for:

```rust
#[tokio::test]
async fn tunnel_logs_decodes_success_payload() {}

#[tokio::test]
async fn tunnel_logs_surfaces_structured_error_message() {}
```

Use a tiny local `axum` server to assert that:
- `GET /v1/tunnel/logs?lines=50` decodes into `TunnelLogsResponse`,
- bearer auth still propagates when configured,
- non-2xx responses surface the structured API error string.

**Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tunnelmux-control-client tunnel_logs_decodes_success_payload -- --exact`
Expected: FAIL because the shared client does not yet expose a log-tail helper.

**Step 3: Write the minimal implementation**

Add a small helper such as:

```rust
pub async fn tunnel_logs(&self, lines: usize) -> anyhow::Result<TunnelLogsResponse>
```

Implementation requirements:
- call `GET /v1/tunnel/logs`,
- send `lines` as a query parameter,
- reuse existing token injection and response decoding,
- return `TunnelLogsResponse` from `tunnelmux_core`.

**Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tunnelmux-control-client tunnel_logs_decodes_success_payload -- --exact`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-control-client/src/lib.rs
git commit -m "feat: add control client log tail support"
```

### Task 2: Add diagnostics view models for GUI-facing data

**Files:**
- Modify: `crates/tunnelmux-gui/src/view_models.rs`
- Test: `crates/tunnelmux-gui/src/view_models.rs`

**Step 1: Write the failing tests**

Add tests in `crates/tunnelmux-gui/src/view_models.rs` for:

```rust
#[test]
fn diagnostics_summary_vm_preserves_counts_and_reload_state() {}

#[test]
fn upstream_health_vm_maps_unknown_health_to_neutral_label() {}

#[test]
fn log_tail_vm_preserves_requested_lines_and_order() {}
```

Cover these behaviors:
- `DiagnosticsResponse` maps into a summary VM with route counts, tunnel state, config reload state, and reload error,
- `healthy = None` maps to a neutral/unknown health state,
- log-tail view models keep line order and requested tail length metadata stable for the UI.

**Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tunnelmux-gui diagnostics_summary_vm_preserves_counts_and_reload_state -- --exact`
Expected: FAIL because diagnostics view models do not exist yet.

**Step 3: Write the minimal implementation**

Add GUI-facing types in `crates/tunnelmux-gui/src/view_models.rs`, for example:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticsSummaryVm { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpstreamHealthVm { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogTailVm { /* ... */ }
```

Implementation requirements:
- keep the structs frontend-friendly and serializable,
- include explicit health-state labeling for `healthy`, `unhealthy`, and `unknown`,
- avoid leaking daemon-only field names into the UI where a clearer name helps.

**Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tunnelmux-gui diagnostics_summary_vm_preserves_counts_and_reload_state -- --exact`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/src/view_models.rs
git commit -m "feat: add GUI diagnostics view models"
```

### Task 3: Add Tauri diagnostics commands and command-layer tests

**Files:**
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Modify: `crates/tunnelmux-gui/src/lib.rs`
- Test: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Write the failing tests**

Add tests in `crates/tunnelmux-gui/src/commands.rs` for:

```rust
#[tokio::test]
async fn load_diagnostics_summary_returns_connected_snapshot() {}

#[tokio::test]
async fn load_upstreams_health_maps_mixed_health_states() {}

#[tokio::test]
async fn load_recent_logs_returns_requested_tail_lines() {}

#[tokio::test]
async fn diagnostics_commands_surface_connection_errors_cleanly() {}
```

Use local `axum` routes for:
- `GET /v1/diagnostics`
- `GET /v1/upstreams/health`
- `GET /v1/tunnel/logs`

Assert that the command-layer helpers:
- load saved settings from the temp directory,
- map daemon payloads into the new view models,
- preserve log ordering,
- return string errors that the frontend can display directly.

**Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tunnelmux-gui load_diagnostics_summary_returns_connected_snapshot -- --exact`
Expected: FAIL because the diagnostics command helpers are not implemented yet.

**Step 3: Write the minimal implementation**

In `crates/tunnelmux-gui/src/commands.rs` add command helpers such as:

```rust
pub async fn load_diagnostics_summary_from_settings_dir(...) -> Result<DiagnosticsSummaryVm, String>
pub async fn load_upstreams_health_from_settings_dir(...) -> Result<Vec<UpstreamHealthVm>, String>
pub async fn load_recent_logs_from_settings_dir(...) -> Result<LogTailVm, String>
```

Then expose matching `#[tauri::command]` functions and register them in `crates/tunnelmux-gui/src/lib.rs`.

Implementation requirements:
- reuse the shared control client,
- keep command return values stable and serializable,
- do not make one panel depend on another panel's success,
- validate `lines` input defensively before making the request.

**Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tunnelmux-gui load_diagnostics_summary_returns_connected_snapshot -- --exact`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/src/commands.rs crates/tunnelmux-gui/src/lib.rs
git commit -m "feat: add GUI diagnostics commands"
```

### Task 4: Wire the diagnostics workspace into the Tauri frontend

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/styles.css`

**Step 1: Keep the frontend change surface small**

Before editing, identify the minimum DOM additions needed:
- add a `Diagnostics` navigation target,
- add containers for runtime summary, upstream health, and recent logs,
- add refresh and line-count controls only where they directly support the diagnostics flow.

**Step 2: Write the minimal implementation**

Update the frontend so it:
- shows a diagnostics workspace without disturbing dashboard and routes behavior,
- starts polling only when the diagnostics workspace is active,
- stops polling when the user navigates away,
- replaces the displayed log tail on refresh rather than incrementally appending,
- renders panel-scoped loading, empty, and error states.

Keep the JavaScript logic thin: DOM lookup, command invocation, timer lifecycle, and rendering.

**Step 3: Run a targeted build check**

Run: `cargo check -p tunnelmux-gui`
Expected: PASS.

**Step 4: Run a manual smoke verification**

Run: `cargo run -p tunnelmux-gui`

Manual checklist:
- diagnostics workspace opens,
- summary/upstreams/logs load against a running local daemon,
- leaving diagnostics stops active polling,
- returning to diagnostics resumes polling,
- dashboard and routes still behave as before.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/styles.css
git commit -m "feat: add GUI diagnostics workspace"
```

### Task 5: Update docs and run full verification

**Files:**
- Modify: `README.md`
- Modify: `crates/tunnelmux-gui/README.md`
- Modify: `docs/ROADMAP.md`

**Step 1: Update documentation**

Document:
- the new diagnostics workspace,
- that it uses polling rather than SSE,
- what data is included in the first release.

**Step 2: Run formatting**

Run: `cargo fmt`
Expected: PASS.

**Step 3: Run targeted validation**

Run: `cargo test -p tunnelmux-control-client`
Expected: PASS.

Run: `cargo test -p tunnelmux-gui`
Expected: PASS.

**Step 4: Run workspace verification**

Run: `cargo test --workspace --quiet`
Expected: PASS.

**Step 5: Commit**

```bash
git add README.md crates/tunnelmux-gui/README.md docs/ROADMAP.md
git commit -m "docs: document GUI diagnostics workspace"
```
