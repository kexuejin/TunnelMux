# GUI MVP Design

**Date:** 2026-03-06
**Status:** Approved for implementation

## Context

TunnelMux already has a working daemon (`tunnelmuxd`), a CLI client (`tunnelmux-cli`), and a documented local control-plane API. Phase 3 of the roadmap still has one major product-facing gap: a GUI MVP.

The current architecture is already suitable for a desktop control surface:

- `tunnelmuxd` is the only control-plane authority,
- the CLI is an API client rather than a privileged manager,
- `tunnelmux-core` already defines shared protocol models.

That means the GUI should not invent a parallel execution model. It should act as a local operator console for the existing daemon.

## Goals

- Deliver a desktop GUI MVP for macOS, Windows, and Linux.
- Keep `tunnelmuxd` as the single source of control-plane truth.
- Support the highest-value operator flows in one window.
- Reuse as much existing Rust API/client logic as possible.
- Avoid introducing a Node/React toolchain for the first iteration.
- Keep the design small enough to implement and verify quickly.

## Non-Goals

- No daemon lifecycle ownership from the GUI in this iteration.
- No system tray, auto-start, or background resident behavior.
- No live log viewer, diagnostics screen, or upstream-health screen in the MVP.
- No SSE-driven real-time dashboard in the MVP.
- No route import/export, bulk edit, or advanced table filtering.
- No plugin model or multi-tenant desktop features.

## User-Confirmed Scope

The GUI MVP is an **operations console**.

User-confirmed decisions:

- value target: operations console rather than read-only monitoring,
- daemon relationship: connect to an already-running `tunnelmuxd`,
- feature scope: dashboard + tunnel start/stop + route list/create/update/delete,
- platform target: macOS + Windows + Linux all count as MVP targets,
- frontend stack: pure `Tauri + HTML/CSS/JS`.

## Approaches Considered

### 1. Browser-style frontend calling daemon HTTP API directly

**Pros**
- Simple mental model.
- Minimal Rust bridge layer.

**Cons**
- GUI would need to manage direct HTTP concerns from JavaScript.
- Authentication and request behavior would be duplicated outside Rust.
- Harder to share data models and error handling with the CLI.
- Less clean for desktop packaging and local-secret handling.

**Decision:** Rejected.

### 2. Tauri frontend with Rust command bridge and shared control client

**Pros**
- Preserves the daemon as the single control authority.
- Avoids browser CORS/security awkwardness.
- Allows reuse of Rust request/auth/error logic.
- Lets the frontend stay thin and platform-neutral.
- Fits the existing API-first architecture.

**Cons**
- Requires one extra shared client abstraction.
- Slightly more Rust-side plumbing up front.

**Decision:** Recommended.

### 3. GUI shell that spawns `tunnelmux-cli` subcommands

**Pros**
- Fastest possible prototype.
- Could reuse CLI behavior immediately.

**Cons**
- Fragile output parsing boundary.
- Poor UX for interactive forms and status refresh.
- Hard to test and scale as a real product surface.
- Couples GUI behavior to CLI text/JSON output shape.

**Decision:** Rejected.

## Recommended Design

### Architecture

Add a new desktop application crate, `crates/tunnelmux-gui`, based on Tauri.

The GUI does **not** talk to the daemon directly from JavaScript. Instead:

1. the frontend calls Tauri commands,
2. Tauri commands call a shared Rust control-plane client,
3. the shared client talks to `tunnelmuxd` over the existing HTTP API.

To support this cleanly, extract the generic HTTP/auth/error logic from the CLI into a small shared crate, tentatively `crates/tunnelmux-control-client`.

Resulting layers:

- `tunnelmuxd` — daemon and HTTP control API
- `tunnelmux-control-client` — reusable Rust API client
- `tunnelmux-cli` — command-line control surface using shared client
- `tunnelmux-gui` — desktop control surface using shared client through Tauri commands

### Window Structure

The MVP uses a single desktop window with three primary regions:

1. **Top status bar**
   - connection state
   - tunnel state
   - provider
   - public base URL
   - manual refresh action

2. **Tunnel action panel**
   - start form: provider, target URL, auto-restart
   - stop action

3. **Routes workspace**
   - route table/list
   - create/edit form
   - delete action with confirmation

This keeps the UI compact, familiar, and implementation-friendly.

### Supported API Surface

The MVP only needs these daemon endpoints:

- `GET /v1/health`
- `GET /v1/tunnel/status`
- `POST /v1/tunnel/start`
- `POST /v1/tunnel/stop`
- `GET /v1/routes`
- `POST /v1/routes`
- `PUT /v1/routes/{id}`
- `DELETE /v1/routes/{id}`

Anything outside this set is intentionally deferred.

### State Flow

#### Startup

On window startup:

1. load saved GUI connection settings,
2. probe `GET /v1/health`,
3. if reachable, fetch tunnel status and routes,
4. render the dashboard.

#### Refresh

- tunnel status can be refreshed on a lightweight timer,
- routes are refreshed on initial load, after successful mutations, and on manual refresh,
- the frontend does not attempt advanced client-side cache reconciliation.

#### Mutations

For tunnel start/stop and route create/update/delete:

- submit command,
- wait for daemon response,
- show success or error,
- re-fetch authoritative data.

This favors correctness over optimistic UI complexity.

## Data and Configuration Model

### GUI-owned local settings

The GUI keeps only local desktop settings such as:

- `base_url` (default `http://127.0.0.1:4765`)
- optional bearer token
- small UI preferences if needed later

These are desktop-client settings, not daemon configuration.

### Route editing

The route editor mirrors the existing API shape instead of inventing a GUI-only model:

- `id`
- `match_host`
- `match_path_prefix`
- `strip_path_prefix`
- `upstream_url`
- `fallback_upstream_url`
- `health_check_path`
- `enabled`

A single form should be reused for both create and update flows.

## Error Handling

Errors should be grouped by operator meaning rather than only transport source.

### Connection errors

Examples:
- daemon not running,
- wrong base URL,
- unreachable local port.

UX behavior:
- show a persistent connection warning,
- keep the settings editable,
- do not pretend the dashboard is current.

### Authorization errors

Examples:
- missing token,
- invalid bearer token.

UX behavior:
- surface a clear authentication error,
- keep token/base URL editable,
- allow retry without restarting the GUI.

### Validation errors

Examples:
- invalid route payload,
- malformed upstream URL,
- duplicate route ID.

UX behavior:
- show error near the relevant form,
- preserve user input,
- do not clear the form on failure.

### Server errors

Examples:
- daemon internal failure,
- unexpected API response.

UX behavior:
- show request failure message,
- preserve the last rendered successful snapshot,
- allow manual retry.

## Cross-Platform Strategy

The MVP should be structured for all three desktop platforms from the start while minimizing platform-specific behavior.

### Included in MVP

- single window
- standard Tauri packaging structure
- portable HTML/CSS/JS UI
- standard HTTP client behavior via Rust

### Explicitly deferred

- tray icons/menu bar extras
- OS auto-launch hooks
- platform-native notifications beyond simple dialogs
- platform-specific window chrome customization

### Packaging assumptions

- macOS uses system WebKit,
- Windows uses WebView2,
- Linux uses WebKitGTK.

The repo and docs should make these assumptions explicit so cross-platform builds are predictable.

## Testing Strategy

### Shared client tests

Add Rust tests covering:

- bearer token request handling,
- response decoding,
- structured error decoding,
- route and tunnel operation helpers.

### GUI command tests

Add Rust-side tests for the Tauri-facing command layer using mock or test daemon endpoints where practical.

The command layer should own most logic so the frontend JavaScript remains thin.

### Frontend verification

Keep frontend logic intentionally small:

- render state,
- collect form inputs,
- call Tauri commands,
- display results.

This reduces the need for a separate JS-heavy testing stack in the MVP.

## Documentation Changes

Update docs to cover:

- GUI MVP architecture in `docs/ARCHITECTURE.md`
- GUI startup/build instructions in `README.md`
- release/build expectations in `docs/RELEASING.md`
- roadmap completion state in `docs/ROADMAP.md` once implementation lands

## Rollout Notes

This GUI MVP is intentionally a control console, not a full observability suite. That boundary keeps the first desktop release useful without overcommitting to live streams, system tray behavior, or advanced workflow automation.

The design also sets up the right long-term layering:

- daemon remains the system of record,
- CLI and GUI become equal API clients,
- the shared Rust control client becomes the common contract layer for future product surfaces.
