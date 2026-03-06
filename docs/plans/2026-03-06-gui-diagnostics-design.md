# GUI Diagnostics Design

**Date:** 2026-03-06
**Status:** Approved for implementation

## Context

TunnelMux now has a working GUI MVP in `crates/tunnelmux-gui`, but the desktop experience still leans heavily toward control actions rather than troubleshooting. Operators can start and stop tunnels and manage routes, yet they still need the CLI or raw API to answer common questions such as:

- is the daemon reachable,
- what does the runtime think its current state is,
- which upstream is unhealthy,
- what recent provider logs say.

The daemon and docs already expose the required server-side capabilities:

- `GET /v1/diagnostics`
- `GET /v1/upstreams/health`
- `GET /v1/tunnel/logs?lines=N`

That means the gap is primarily a GUI presentation and command-layer gap, not a new backend capability gap.

## Goals

- Add a diagnostics workspace to the existing GUI MVP.
- Surface the highest-value troubleshooting data without leaving the desktop app.
- Reuse the shared Rust control client and current Tauri command pattern.
- Keep the implementation small enough to land quickly and validate with targeted tests.
- Preserve the current GUI architecture: JavaScript UI -> Tauri commands -> shared Rust client -> daemon HTTP API.

## Non-Goals

- No SSE or stream-based live logs in this iteration.
- No log filtering, searching, exporting, or copy-diagnostics bundle flow.
- No daemon auto-launch or lifecycle management changes.
- No new daemon API fields or new control-plane endpoints unless a tiny client gap must be filled.
- No release-pipeline or installer work in this task.

## User-Confirmed Scope

This iteration is a **real-time operations panel** built on lightweight polling.

Confirmed direction:

- add a dedicated `Diagnostics` workspace,
- include runtime summary, upstream health, and recent logs,
- use polling rather than SSE for the first version,
- keep the existing dashboard and routes flows intact,
- avoid expanding into packaging, tray features, or daemon ownership.

## Approaches Considered

### 1. Static diagnostics page with manual refresh only

**Pros**
- Lowest implementation cost.
- Minimal frontend state.
- Easy to test.

**Cons**
- Weak troubleshooting ergonomics.
- Requires repetitive manual refreshing while debugging runtime issues.
- Feels noticeably behind the rest of the control console.

**Decision:** Rejected.

### 2. Polling-based diagnostics workspace

**Pros**
- Good operator experience with modest implementation cost.
- Reuses existing HTTP endpoints and current Tauri command architecture.
- Keeps state handling predictable and easy to test.
- Leaves a clean upgrade path to SSE later.

**Cons**
- Not fully real-time.
- Requires timer lifecycle handling in the frontend.

**Decision:** Recommended.

### 3. Fully stream-driven diagnostics workbench

**Pros**
- Best real-time responsiveness.
- Strongest long-running observability experience.

**Cons**
- Higher complexity in Tauri command plumbing and frontend subscription management.
- Larger testing surface.
- Too much scope for the next GUI increment.

**Decision:** Deferred.

## Recommended Design

### Architecture

Extend the existing GUI architecture without introducing a parallel data path.

1. The frontend adds a `Diagnostics` workspace and polling controller.
2. The Tauri layer adds focused commands for diagnostics summary, upstream health, and recent logs.
3. The shared Rust control client is extended only where the GUI still lacks an endpoint helper.
4. The daemon remains unchanged unless a tiny compatibility adjustment is required.

This keeps the GUI thin, preserves shared request/auth/error handling, and avoids duplicating HTTP logic in JavaScript.

### Workspace Structure

Add a third primary workspace alongside the current dashboard and routes UI.

The diagnostics workspace contains three sections:

1. **Runtime Summary**
   - tunnel state
   - provider
   - public URL
   - route counts
   - config reload state
   - recent config reload error if present

2. **Upstream Health**
   - upstream URL
   - health-check path
   - health state (`healthy`, `unhealthy`, `unknown`)
   - last checked timestamp
   - last error

3. **Recent Logs**
   - last `N` provider log lines
   - selectable tail size (`50`, `100`, `200`)
   - manual refresh action
   - local clear-display action only

### Refresh Model

Use polling rather than streaming.

- diagnostics summary and upstream health refresh every `5s`,
- recent logs refresh every `3s`,
- timers start only while the diagnostics workspace is active,
- timers stop when the user leaves that workspace.

This keeps background activity low and avoids stale hidden polling.

### State Model

Add GUI-facing view models that are easy for the frontend to render directly:

- `DiagnosticsWorkspaceSnapshot`
- `DiagnosticsSummaryVm`
- `UpstreamHealthVm`
- `LogTailVm`

Each panel should track:

- current data,
- loading state,
- last-updated timestamp,
- local error message.

Errors are panel-scoped rather than page-scoped so one failing endpoint does not blank the entire diagnostics page.

### Command Layer

Add focused Tauri commands in `crates/tunnelmux-gui/src/commands.rs`:

- `load_diagnostics_summary`
- `load_upstreams_health`
- `load_recent_logs`

The commands should:

- resolve saved GUI settings,
- build the shared control client,
- call the matching control-plane endpoint,
- map responses into frontend-friendly view models,
- return stable string errors when requests fail.

### Shared Client Changes

The shared control client already supports:

- `health`
- `diagnostics`
- `upstreams_health`

It does not yet expose a helper for `GET /v1/tunnel/logs?lines=N`.

Add a small shared-client method for log tail retrieval so both CLI and GUI can share the same non-streaming logs request path in the future.

### Frontend Behavior

The frontend should not perform direct HTTP requests.

Instead it should:

- register a `Diagnostics` navigation target,
- invoke the three new Tauri commands,
- render each panel independently,
- preserve the last successful data when a refresh fails,
- show lightweight loading and error states per panel.

For the first iteration, log refresh replaces the whole displayed tail rather than incrementally appending entries. That keeps the state model simple and predictable.

## Error Handling

- If the daemon is unreachable, each diagnostics panel shows a local error message.
- If only one endpoint fails, other panels continue rendering their last good data.
- Authentication or base URL misconfiguration continues using the current GUI error style.
- Unknown upstream health (`healthy = null`) renders as a neutral state rather than an error.
- Empty log tails render as a valid empty state, not a failure.

## Testing Strategy

### Shared client

Add targeted tests for the new non-streaming logs helper:

- success decode for `GET /v1/tunnel/logs`
- error propagation for non-2xx log-tail responses

### Tauri command layer

Add tests for:

- diagnostics summary command mapping daemon payloads into GUI snapshots,
- upstream health command mapping mixed healthy/unhealthy/unknown upstreams,
- recent logs command preserving line order and honoring `lines` input,
- command error propagation on connection failures.

### Frontend

Add lightweight UI tests around:

- diagnostics workspace activation,
- polling lifecycle start/stop behavior,
- per-panel rendering for loading, success, empty, and error states.

No end-to-end desktop automation is required for this iteration.

## Delivery Plan

Recommended commit sequence:

1. `feat: add control client log tail support`
2. `feat: add tauri diagnostics commands and view models`
3. `feat: add GUI diagnostics workspace`
4. `test/docs: cover diagnostics polling flow`

## Open Follow-Ups

These are intentionally deferred until after this MVP-sized diagnostics increment lands:

- SSE-backed live logs and health streams
- richer log inspection tools
- diagnostics export bundle
- upstream probe actions
- native installer packaging work
