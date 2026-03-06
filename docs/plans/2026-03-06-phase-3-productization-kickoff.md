# Phase 3 Productization Kickoff Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Bring TunnelMux from a released MVP (`v0.1.3`) into the first concrete Phase 3 milestone: aligned product docs, explicit config reload support, and an initial diagnostics surface.

**Architecture:** Keep the current API-first split intact: `tunnelmuxd` remains the control plane, the gateway remains data-plane only, and `tunnelmux-cli` stays a thin API client. Add new productization behavior as small control-plane endpoints and matching CLI subcommands first, then reduce risk by extracting large `main.rs` sections into modules without changing external behavior.

**Tech Stack:** Rust 2024 workspace, `axum`, `tokio`, `clap`, `reqwest`, `serde`, Markdown docs.

---

## Current Status Snapshot (2026-03-06)

- Latest released tag is `v0.1.3`.
- `Phase 0` and `Phase 2` roadmap items are complete.
- `Phase 1` checklist is complete in practice, but `docs/ROADMAP.md` still labels the phase as “In Progress”.
- `docs/INTEGRATION-TEMPLATES.md` exists locally and directly addresses the first unchecked `Phase 3` roadmap item, but is not committed yet.
- `cargo test --workspace --quiet` currently passes in a non-sandboxed environment.

## Two-Week Outcome Target

By the end of this plan, TunnelMux should have:

1. product docs that match the real project state,
2. a supported config reload flow exposed via daemon API + CLI,
3. a first operational diagnostics endpoint for local debugging and support,
4. a smaller surface area inside the monolithic daemon/CLI entrypoints for follow-up GUI work.

### Task 1: Align docs with actual project phase

**Files:**
- Modify: `README.md`
- Modify: `docs/ROADMAP.md`
- Modify: `docs/INTEGRATION.md`
- Add: `docs/INTEGRATION-TEMPLATES.md`
- Test: document consistency checks via `rg`

**Step 1: Write the failing consistency checks**

Run: `rg -n 'Phase 1: Minimum Viable Product \(In Progress\)' docs/ROADMAP.md`
Expected: one match, proving the roadmap state is stale.

Run: `rg -n 'Integration Templates' README.md docs/INTEGRATION.md`
Expected: missing or incomplete top-level references to the new template document.

**Step 2: Confirm the new doc is ready to land**

Run: `sed -n '1,220p' docs/INTEGRATION-TEMPLATES.md`
Expected: Bash, Node.js, and Python templates are present and coherent.

**Step 3: Update the docs minimally**

- mark the integration-template roadmap item as done,
- update the roadmap phase labels to reflect actual progress,
- link `docs/INTEGRATION-TEMPLATES.md` from `README.md` and `docs/INTEGRATION.md`.

Suggested diff shape:

```md
- ## Phase 1: Minimum Viable Product (In Progress)
+ ## Phase 1: Minimum Viable Product (Completed)

- - [ ] generic third-party integration templates
+ - [x] generic third-party integration templates
```

**Step 4: Re-run the consistency checks**

Run: `rg -n 'Phase 1: Minimum Viable Product \(Completed\)' docs/ROADMAP.md`
Expected: one match.

Run: `rg -n 'Integration Templates' README.md docs/INTEGRATION.md docs/INTEGRATION-TEMPLATES.md`
Expected: links and headings found in all intended locations.

**Step 5: Commit**

```bash
git add README.md docs/ROADMAP.md docs/INTEGRATION.md docs/INTEGRATION-TEMPLATES.md
git commit -m "docs: align roadmap and integration templates"
```

### Task 2: Add explicit config reload API and CLI support

**Files:**
- Modify: `crates/tunnelmux-core/src/lib.rs`
- Modify: `crates/tunnelmuxd/src/main.rs`
- Modify: `crates/tunnelmux-cli/src/main.rs`
- Test: `crates/tunnelmuxd/src/main.rs`
- Test: `crates/tunnelmux-cli/src/main.rs`

**Step 1: Write the failing daemon API test**

Add a test near the existing endpoint tests in `crates/tunnelmuxd/src/main.rs` that:

```rust
#[tokio::test]
async fn settings_reload_endpoint_refreshes_routes_from_disk() {
    // 1. bootstrap test state with route A
    // 2. mutate the persisted state file on disk to route B
    // 3. call POST /v1/settings/reload
    // 4. assert GET /v1/routes now returns route B
}
```

**Step 2: Run the targeted test to verify it fails**

Run: `cargo test -p tunnelmuxd settings_reload_endpoint_refreshes_routes_from_disk -- --exact`
Expected: FAIL because `/v1/settings/reload` does not exist yet.

**Step 3: Add the minimal shared API model**

Add a small response type in `crates/tunnelmux-core/src/lib.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReloadSettingsResponse {
    pub reloaded: bool,
    pub route_count: usize,
    pub tunnel_state: TunnelState,
}
```

**Step 4: Implement the daemon endpoint**

- register `POST /v1/settings/reload` in the protected control router,
- reload persisted state from `state.data_file`,
- preserve live process-only fields that should not be forged from disk,
- refresh in-memory health-check settings from the reloaded snapshot,
- return a concise reload summary.

Minimal handler shape:

```rust
async fn reload_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ReloadSettingsResponse>, ApiError> {
    let persisted = load_persisted_state(&state.data_file).await?;
    let mut runtime = state.runtime.lock().await;
    runtime.persisted.routes = persisted.routes;
    runtime.persisted.health_check = persisted.health_check.clone();
    *state.health_check_settings.write().await =
        resolve_health_check_settings(defaults, persisted.health_check);
    Ok(Json(ReloadSettingsResponse { /* ... */ }))
}
```

**Step 5: Add the CLI command and test**

Add `settings reload` in `crates/tunnelmux-cli/src/main.rs` and a parser/output test such as:

```rust
#[test]
fn parses_settings_reload_command() {
    let cli = Cli::try_parse_from(["tunnelmux", "settings", "reload"]).unwrap();
    // assert command matches SettingsCommand::Reload
}
```

**Step 6: Run the targeted tests to verify they pass**

Run: `cargo test -p tunnelmuxd settings_reload_endpoint_refreshes_routes_from_disk -- --exact`
Expected: PASS.

Run: `cargo test -p tunnelmux-cli parses_settings_reload_command -- --exact`
Expected: PASS.

**Step 7: Run the broader verification**

Run: `cargo test --workspace --quiet`
Expected: all tests pass.

**Step 8: Commit**

```bash
git add crates/tunnelmux-core/src/lib.rs crates/tunnelmuxd/src/main.rs crates/tunnelmux-cli/src/main.rs
git commit -m "feat: add explicit config reload flow"
```

### Task 3: Add operational diagnostics snapshot endpoint

**Files:**
- Modify: `crates/tunnelmux-core/src/lib.rs`
- Modify: `crates/tunnelmuxd/src/main.rs`
- Modify: `crates/tunnelmux-cli/src/main.rs`
- Modify: `README.md`
- Test: `crates/tunnelmuxd/src/main.rs`
- Test: `crates/tunnelmux-cli/src/main.rs`

**Step 1: Write the failing daemon test**

Add a test near the dashboard/metrics tests:

```rust
#[tokio::test]
async fn diagnostics_endpoint_returns_local_runtime_context() {
    // assert response includes data file path, provider log path,
    // route count, tunnel state, and pending_restart summary
}
```

**Step 2: Run the targeted test to verify it fails**

Run: `cargo test -p tunnelmuxd diagnostics_endpoint_returns_local_runtime_context -- --exact`
Expected: FAIL because `/v1/diagnostics` is not implemented.

**Step 3: Add shared response types**

Add a focused model in `crates/tunnelmux-core/src/lib.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticsResponse {
    pub data_file: String,
    pub provider_log_file: String,
    pub route_count: usize,
    pub enabled_route_count: usize,
    pub tunnel_state: TunnelState,
    pub pending_restart: bool,
}
```

**Step 4: Implement the endpoint in the daemon**

- add `GET /v1/diagnostics` to the protected router,
- build the response from existing `AppState` data instead of inventing new persistence,
- keep the payload intentionally small and support-oriented.

**Step 5: Add CLI support**

Add a top-level command such as `tunnelmux diagnostics` that prints JSON by default and a human summary in table/text mode.

Parser safety net:

```rust
#[test]
fn parses_diagnostics_command() {
    let cli = Cli::try_parse_from(["tunnelmux", "diagnostics"]).unwrap();
    // assert command matches Command::Diagnostics
}
```

**Step 6: Run targeted validation**

Run: `cargo test -p tunnelmuxd diagnostics_endpoint_returns_local_runtime_context -- --exact`
Expected: PASS.

Run: `cargo test -p tunnelmux-cli parses_diagnostics_command -- --exact`
Expected: PASS.

**Step 7: Update user-facing docs**

Add one short README section showing:

```bash
tunnelmux diagnostics
tunnelmux settings reload
```

**Step 8: Run workspace verification and commit**

Run: `cargo test --workspace --quiet`
Expected: all tests pass.

```bash
git add README.md crates/tunnelmux-core/src/lib.rs crates/tunnelmuxd/src/main.rs crates/tunnelmux-cli/src/main.rs
git commit -m "feat: add diagnostics snapshot endpoint"
```

### Task 4: Extract daemon modules without changing behavior

**Files:**
- Create: `crates/tunnelmuxd/src/api.rs`
- Create: `crates/tunnelmuxd/src/gateway.rs`
- Create: `crates/tunnelmuxd/src/persistence.rs`
- Create: `crates/tunnelmuxd/src/runtime.rs`
- Modify: `crates/tunnelmuxd/src/main.rs`
- Test: `crates/tunnelmuxd/src/main.rs`

**Step 1: Snapshot existing behavior with targeted tests**

Run:

```bash
cargo test -p tunnelmuxd dashboard_endpoint_returns_composite_snapshot -- --exact
cargo test -p tunnelmuxd websocket_proxy_supports_wss_upstream -- --exact
cargo test -p tunnelmuxd apply_routes_endpoint_replaces_routes_when_enabled -- --exact
```

Expected: PASS before refactor.

**Step 2: Extract persistence helpers first**

Move `load_persisted_state`, `save_state_file`, and default path helpers into `persistence.rs`.

**Step 3: Extract HTTP handlers and gateway logic separately**

- move control-plane handlers/builders into `api.rs`,
- move proxy/request forwarding helpers into `gateway.rs`,
- move monitor/restart logic into `runtime.rs`.

Keep `main.rs` limited to wiring, startup, and module exports.

**Step 4: Re-run the same targeted tests**

Expected: PASS with no externally visible behavior change.

**Step 5: Run workspace verification**

Run: `cargo test --workspace --quiet`
Expected: all tests pass.

**Step 6: Commit**

```bash
git add crates/tunnelmuxd/src/main.rs crates/tunnelmuxd/src/api.rs crates/tunnelmuxd/src/gateway.rs crates/tunnelmuxd/src/persistence.rs crates/tunnelmuxd/src/runtime.rs
git commit -m "refactor: split daemon modules for productization work"
```

### Task 5: Reduce CLI entrypoint size before GUI MVP work

**Files:**
- Create: `crates/tunnelmux-cli/src/commands.rs`
- Create: `crates/tunnelmux-cli/src/output.rs`
- Create: `crates/tunnelmux-cli/src/client.rs`
- Modify: `crates/tunnelmux-cli/src/main.rs`
- Test: `crates/tunnelmux-cli/src/main.rs`

**Step 1: Capture parser and decision-function safety nets**

Run:

```bash
cargo test -p tunnelmux-cli decide_expose_tunnel_action_starts_when_tunnel_not_running -- --exact
cargo test -p tunnelmux-cli infer_expose_route_action_classifies_create_update_and_unchanged -- --exact
cargo test -p tunnelmux-cli parses_diagnostics_command -- --exact
```

Expected: PASS before refactor.

**Step 2: Extract HTTP client helpers**

Move request construction, auth header setup, and shared response decoding into `client.rs`.

**Step 3: Extract output formatting and command runners**

- move human/JSON rendering helpers into `output.rs`,
- move subcommand execution branches into `commands.rs`.

**Step 4: Re-run targeted tests**

Expected: PASS.

**Step 5: Run workspace verification and commit**

Run: `cargo test --workspace --quiet`
Expected: all tests pass.

```bash
git add crates/tunnelmux-cli/src/main.rs crates/tunnelmux-cli/src/client.rs crates/tunnelmux-cli/src/commands.rs crates/tunnelmux-cli/src/output.rs
git commit -m "refactor: split cli command and output modules"
```

## Suggested Execution Order

1. Task 1 — merge the docs truth first.
2. Task 2 — add supported reload semantics.
3. Task 3 — add operator-facing diagnostics.
4. Task 4 — refactor daemon after new endpoints are stable.
5. Task 5 — refactor CLI after command surface settles.

## Verification Checklist

- `cargo test --workspace --quiet`
- smoke test `tunnelmux settings reload` against a local daemon
- smoke test `tunnelmux diagnostics`
- manually confirm README / roadmap / integration docs tell a consistent Phase 3 story

