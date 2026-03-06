# GUI MVP Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a cross-platform Tauri GUI MVP that connects to an already-running `tunnelmuxd`, shows tunnel status and routes, supports tunnel start/stop, and supports route create/update/delete without introducing a Node/React frontend stack.

**Architecture:** Extract the existing Rust HTTP/auth/error logic into a shared control-plane client crate, refactor the CLI to use that shared client, and add a new `crates/tunnelmux-gui` Tauri application whose frontend is plain HTML/CSS/JS. The GUI stays thin: JavaScript renders state and submits forms through Tauri commands, while Rust owns request logic, local connection settings, and error mapping.

**Tech Stack:** Rust 2024 workspace, `tauri`, `tokio`, `reqwest`, `serde`, `axum` test servers, static HTML/CSS/JS, Markdown docs, GitHub Actions release workflow.

---

### Task 1: Extract a shared control-plane client crate

**Files:**
- Create: `crates/tunnelmux-control-client/Cargo.toml`
- Create: `crates/tunnelmux-control-client/src/lib.rs`
- Modify: `Cargo.toml`
- Modify: `crates/tunnelmux-cli/Cargo.toml`
- Test: `crates/tunnelmux-control-client/src/lib.rs`

**Step 1: Write the failing tests**

Add tests in `crates/tunnelmux-control-client/src/lib.rs` for:

```rust
#[tokio::test]
async fn tunnel_status_decodes_success_payload() {}

#[tokio::test]
async fn create_route_surfaces_structured_error_message() {}
```

The tests should spin a tiny local `axum` server, return representative JSON payloads, and assert that the future shared client correctly:
- decodes `TunnelStatusResponse`,
- sends bearer auth when configured,
- converts error JSON into readable `anyhow` failures.

**Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tunnelmux-control-client tunnel_status_decodes_success_payload -- --exact`
Expected: FAIL because the new crate and client API do not exist yet.

**Step 3: Write the minimal implementation**

Create `crates/tunnelmux-control-client/src/lib.rs` with a reusable client surface such as:

```rust
pub struct ControlClientConfig {
    pub base_url: String,
    pub token: Option<String>,
}

pub struct TunnelmuxControlClient {
    client: reqwest::Client,
    config: ControlClientConfig,
}

impl TunnelmuxControlClient {
    pub async fn health(&self) -> anyhow::Result<HealthResponse>;
    pub async fn tunnel_status(&self) -> anyhow::Result<TunnelStatusResponse>;
    pub async fn start_tunnel(&self, payload: &TunnelStartRequest) -> anyhow::Result<TunnelStatusResponse>;
    pub async fn stop_tunnel(&self) -> anyhow::Result<TunnelStatusResponse>;
    pub async fn list_routes(&self) -> anyhow::Result<RoutesResponse>;
    pub async fn create_route(&self, payload: &CreateRouteRequest) -> anyhow::Result<RoutesResponse>;
    pub async fn update_route(&self, id: &str, payload: &CreateRouteRequest) -> anyhow::Result<RouteRule>;
    pub async fn delete_route(&self, id: &str, ignore_missing: bool) -> anyhow::Result<DeleteRouteResponse>;
}
```

Move over the generic request building, bearer token handling, response decoding, and API error extraction that currently live in `crates/tunnelmux-cli/src/client.rs`.

Update the workspace in `Cargo.toml` to include the new member and add `tunnelmux-control-client` as a dependency of the CLI.

**Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tunnelmux-control-client tunnel_status_decodes_success_payload -- --exact`
Expected: PASS.

**Step 5: Commit**

```bash
git add Cargo.toml crates/tunnelmux-control-client/Cargo.toml crates/tunnelmux-control-client/src/lib.rs crates/tunnelmux-cli/Cargo.toml
git commit -m "feat: add shared control-plane client crate"
```

### Task 2: Refactor the CLI to use the shared client

**Files:**
- Modify: `crates/tunnelmux-cli/src/client.rs`
- Modify: `crates/tunnelmux-cli/src/commands.rs`
- Modify: `crates/tunnelmux-cli/src/main.rs`
- Test: `crates/tunnelmux-cli/src/main.rs`

**Step 1: Write the failing tests**

Add CLI regression tests in `crates/tunnelmux-cli/src/main.rs` for:

```rust
#[tokio::test]
async fn diagnostics_command_uses_shared_control_client() {}

#[tokio::test]
async fn tunnel_status_command_still_prints_json_payload() {}
```

Use a temporary local HTTP server that returns fixed payloads and assert the command runner still:
- reaches the correct endpoint,
- honors bearer token configuration,
- prints stable JSON output.

**Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tunnelmux-cli diagnostics_command_uses_shared_control_client -- --exact`
Expected: FAIL because the CLI is still wired to its private request helpers.

**Step 3: Write the minimal implementation**

Refactor `crates/tunnelmux-cli/src/commands.rs` so non-streaming commands use `TunnelmuxControlClient` for:
- health,
- diagnostics,
- tunnel status/start/stop,
- routes list/add/update/delete/apply where practical.

Keep SSE-specific CLI behavior local in `crates/tunnelmux-cli/src/client.rs` for now, but remove duplicated request/response helpers that moved to the shared crate.

**Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tunnelmux-cli diagnostics_command_uses_shared_control_client -- --exact`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-cli/src/client.rs crates/tunnelmux-cli/src/commands.rs crates/tunnelmux-cli/src/main.rs
git commit -m "refactor: move CLI control requests to shared client"
```

### Task 3: Scaffold the Tauri GUI shell and local settings store

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/tunnelmux-gui/Cargo.toml`
- Create: `crates/tunnelmux-gui/build.rs`
- Create: `crates/tunnelmux-gui/src/lib.rs`
- Create: `crates/tunnelmux-gui/src/main.rs`
- Create: `crates/tunnelmux-gui/src/settings.rs`
- Create: `crates/tunnelmux-gui/src/state.rs`
- Create: `crates/tunnelmux-gui/tauri.conf.json`
- Create: `crates/tunnelmux-gui/capabilities/default.json`
- Create: `crates/tunnelmux-gui/ui/index.html`
- Create: `crates/tunnelmux-gui/ui/app.js`
- Create: `crates/tunnelmux-gui/ui/styles.css`
- Test: `crates/tunnelmux-gui/src/settings.rs`

**Step 1: Write the failing tests**

Add tests in `crates/tunnelmux-gui/src/settings.rs` for:

```rust
#[test]
fn load_settings_returns_default_base_url_when_missing() {}

#[test]
fn save_and_reload_settings_round_trips_token() {}
```

The tests should use a temporary directory and verify that GUI-local settings serialize and deserialize without depending on a real Tauri runtime.

**Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tunnelmux-gui load_settings_returns_default_base_url_when_missing -- --exact`
Expected: FAIL because the GUI crate and settings store do not exist yet.

**Step 3: Write the minimal implementation**

Create a new Tauri app crate with:

- a tiny `main.rs` that boots Tauri,
- a testable `src/lib.rs` for app wiring,
- `settings.rs` that stores GUI connection settings,
- `state.rs` for shared app state.

Define a settings model such as:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuiSettings {
    pub base_url: String,
    pub token: Option<String>,
}
```

Use a file-backed store under the app config directory, defaulting `base_url` to `http://127.0.0.1:4765`.

Add a minimal static frontend (`index.html`, `app.js`, `styles.css`) that can render a shell view and call Tauri commands once those commands exist.

**Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tunnelmux-gui load_settings_returns_default_base_url_when_missing -- --exact`
Expected: PASS.

**Step 5: Commit**

```bash
git add Cargo.toml crates/tunnelmux-gui
git commit -m "feat: scaffold Tauri GUI shell"
```

### Task 4: Add dashboard loading and tunnel control commands

**Files:**
- Modify: `crates/tunnelmux-gui/src/lib.rs`
- Create: `crates/tunnelmux-gui/src/commands.rs`
- Modify: `crates/tunnelmux-gui/src/state.rs`
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Test: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Write the failing tests**

Add tests in `crates/tunnelmux-gui/src/commands.rs` for:

```rust
#[tokio::test]
async fn refresh_dashboard_returns_tunnel_snapshot_for_connected_daemon() {}

#[tokio::test]
async fn start_tunnel_command_maps_connection_errors_cleanly() {}
```

Use a small local HTTP test server and assert the GUI command layer returns structured success/error payloads that the frontend can render directly.

**Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tunnelmux-gui refresh_dashboard_returns_tunnel_snapshot_for_connected_daemon -- --exact`
Expected: FAIL because the GUI command layer does not exist yet.

**Step 3: Write the minimal implementation**

Add Tauri commands for:

- loading saved settings,
- saving settings,
- probing daemon connectivity,
- refreshing tunnel status,
- starting a tunnel,
- stopping a tunnel.

Return stable UI-facing payloads such as:

```rust
#[derive(Serialize)]
pub struct DashboardSnapshot {
    pub connected: bool,
    pub tunnel: Option<TunnelStatus>,
    pub message: Option<String>,
}
```

Update `ui/app.js` and `ui/index.html` to:
- show connection state,
- render tunnel state/provider/public URL,
- submit start/stop actions,
- disable buttons while requests are in flight,
- show inline error banners.

**Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tunnelmux-gui refresh_dashboard_returns_tunnel_snapshot_for_connected_daemon -- --exact`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/src/lib.rs crates/tunnelmux-gui/src/commands.rs crates/tunnelmux-gui/src/state.rs crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/styles.css
git commit -m "feat: add GUI dashboard and tunnel controls"
```

### Task 5: Add the routes workspace with create/update/delete flows

**Files:**
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Create: `crates/tunnelmux-gui/src/view_models.rs`
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Test: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Write the failing tests**

Add tests in `crates/tunnelmux-gui/src/commands.rs` for:

```rust
#[tokio::test]
async fn save_route_creates_enabled_route_and_returns_fresh_list() {}

#[tokio::test]
async fn delete_route_returns_updated_route_list() {}
```

The tests should verify that the command layer:
- maps GUI form input into API payloads,
- supports both create and update with one save path,
- re-fetches authoritative routes after mutation.

**Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tunnelmux-gui save_route_creates_enabled_route_and_returns_fresh_list -- --exact`
Expected: FAIL because route CRUD commands and UI wiring do not exist yet.

**Step 3: Write the minimal implementation**

Add GUI commands for:
- listing routes,
- saving a route (create or update based on edit mode),
- deleting a route.

Add `view_models.rs` to keep GUI-specific form defaults and display helpers separate from raw API models.

Update the frontend to:
- render a routes table/list,
- open the shared form for create and edit,
- prefill form values when editing,
- confirm before delete,
- refresh the list after each successful mutation,
- preserve user input on validation failure.

**Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tunnelmux-gui save_route_creates_enabled_route_and_returns_fresh_list -- --exact`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/src/commands.rs crates/tunnelmux-gui/src/view_models.rs crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/styles.css
git commit -m "feat: add GUI route management workspace"
```

### Task 6: Document GUI usage and wire release/build verification

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `README.md`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/RELEASING.md`
- Modify: `docs/ROADMAP.md`
- Optional: `crates/tunnelmux-gui/README.md`
- Test: workspace verification commands

**Step 1: Write the failing consistency checks**

Run: `rg -n 'GUI MVP|tunnelmux-gui' README.md docs/ARCHITECTURE.md docs/RELEASING.md docs/ROADMAP.md`
Expected: no GUI MVP documentation exists yet.

**Step 2: Write the minimal docs and release updates**

- document how the GUI connects to an already-running daemon,
- document platform prerequisites for Tauri packaging,
- add the GUI binary or bundle flow to `.github/workflows/release.yml`,
- mark `GUI MVP (Tauri)` complete in `docs/ROADMAP.md` when implementation is actually finished.

**Step 3: Run full verification**

Run: `cargo fmt`
Expected: no formatting errors.

Run: `cargo test --workspace --quiet`
Expected: all workspace tests pass.

Run: `cargo check -p tunnelmux-gui`
Expected: the GUI crate compiles in the local environment.

**Step 4: Commit**

```bash
git add .github/workflows/release.yml README.md docs/ARCHITECTURE.md docs/RELEASING.md docs/ROADMAP.md crates/tunnelmux-gui/README.md
git commit -m "docs: document and release GUI MVP"
```

## Suggested Execution Order

1. Task 1 — extract the shared control-plane client.
2. Task 2 — refactor the CLI to consume the shared client.
3. Task 3 — scaffold the Tauri shell and settings store.
4. Task 4 — add dashboard loading and tunnel controls.
5. Task 5 — add route CRUD workspace.
6. Task 6 — document, package, and verify the full workspace.

## Verification Checklist

- `cargo fmt`
- `cargo test --workspace --quiet`
- `cargo check -p tunnelmux-gui`
- manually connect the GUI to a local daemon and exercise:
  - tunnel start,
  - tunnel stop,
  - route create,
  - route update,
  - route delete
