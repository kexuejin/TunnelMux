# Hot Config Reload Design

**Date:** 2026-03-06
**Status:** Approved for implementation

## Context

TunnelMux currently persists a mixed `state.json` file that contains both durable configuration and runtime-derived tunnel state. The daemon also writes this file back from memory during normal operation. That design is fine for state recovery, but it is a poor fit for automatic configuration reload because the daemon would end up watching a file that it also mutates.

We already added two useful building blocks:

- a manual `settings reload` control-plane entrypoint,
- a `diagnostics` endpoint for local runtime inspection.

The next step is to complete the roadmap's hot reload item without blurring the boundary between declarative config and runtime state.

## Goals

- Add true automatic hot configuration reload.
- Keep runtime tunnel state separate from declarative route configuration.
- Preserve current daemon stability and existing manual reload behavior.
- Expose reload health via diagnostics.
- Keep the implementation dependency-light and easy to test.

## Non-Goals

- No GUI work in this iteration.
- No file watcher dependency in this iteration.
- No live restart of running provider processes during config reload.
- No full multi-file config format or plugin system.

## Approaches Considered

### 1. Watch the existing `state.json`

**Pros**
- Smallest initial code delta.

**Cons**
- The daemon writes the same file it would watch.
- Easy to self-trigger reload loops.
- Runtime state and config remain mixed.
- Harder to reason about from future GUI/API clients.

**Decision:** Rejected.

### 2. Add a separate declarative `config.json` and poll for changes

**Pros**
- Clean separation between desired config and observed runtime state.
- No self-trigger loop because the daemon never writes the config file.
- Cross-platform and testable without adding watcher dependencies.
- Fits the current productization stage.

**Cons**
- Adds one more path to manage.
- Reload latency is polling-based rather than instant.

**Decision:** Recommended.

### 3. Keep only manual reload

**Pros**
- Lowest risk.

**Cons**
- Does not complete the roadmap item.
- Leaves the product half-finished from an operator standpoint.

**Decision:** Rejected.

## Recommended Design

### Config Boundary

Introduce a new declarative config file, defaulting to `~/.tunnelmux/config.json`.

- `state.json` remains the daemon-owned runtime snapshot.
- `config.json` becomes the operator-owned desired configuration file.

`config.json` contains only:

- `routes`
- `health_check`

It does **not** contain:

- tunnel process state,
- process IDs,
- restart counters,
- runtime errors.

### Trigger Model

Use a background polling loop with content hashing.

- Add `--config-file`
- Add `--config-reload-interval-ms`
- Poll every `1000ms` by default
- Reload only when the file content hash changes

This keeps the implementation predictable, dependency-light, and portable.

### Startup Order

1. Load runtime `state.json`
2. Resolve startup/default health-check settings
3. Load declarative `config.json` if present
4. Overlay routes and health-check settings from `config.json`
5. Start background monitors

This ensures config wins over stale persisted route state, while runtime tunnel fields still come from the daemon's existing state model.

### Reload Semantics

A config reload updates only declarative configuration:

- `runtime.persisted.routes`
- `runtime.persisted.health_check`
- `state.health_check_settings`

It does **not** directly mutate:

- `running_tunnel`
- `pending_restart`
- live child process ownership

That keeps reload safe and non-disruptive.

### Failure Handling

If reload fails because the config file is invalid:

- keep the last known good in-memory config,
- record the error in diagnostics,
- continue serving with the last applied config,
- retry on the next poll cycle.

If the config file is absent:

- startup treats it as optional,
- background reload records no fatal error until a previously loaded file disappears,
- manual reload can still fall back to the existing state-file reload behavior.

## Data Model Changes

### New daemon-internal declarative config type

A new internal `DeclarativeConfigFile` struct will hold:

- `routes: Vec<RouteRule>`
- `health_check: Option<HealthCheckSettings>`

### New config reload tracking state

Add daemon-local reload status fields for diagnostics:

- config file path
- reload enabled flag
- reload interval
- last reload timestamp
- last reload error
- last applied file digest

## API and CLI Changes

### Daemon

- Keep `POST /v1/settings/reload`
- Change its preferred behavior to reload from `config.json` when configured
- Keep fallback to the previous state-file reload path when no declarative config file exists
- Extend diagnostics payload with config reload metadata

### CLI

- Keep `tunnelmux settings reload`
- Keep `tunnelmux diagnostics`
- No new CLI command is required for the first iteration

## Testing Strategy

### Daemon tests

Add coverage for:

- startup overlay from `config.json`
- one-shot config poll applying changed config
- invalid config preserving last good state
- diagnostics reporting config reload metadata
- manual reload still working when no config file exists

### CLI tests

Keep parser coverage lightweight:

- existing `settings reload`
- diagnostics output model compatibility through existing JSON behavior

## Docs Changes

Update:

- `README.md`
- `docs/ROADMAP.md`
- optionally `docs/ARCHITECTURE.md` if the config boundary needs to be called out explicitly

The roadmap should mark both:

- hot configuration reload
- operational audit and diagnostics

as complete once implemented.

## Rollout Notes

This design intentionally favors operational clarity over maximum feature surface. It creates a clean foundation for a future GUI because a GUI can safely edit `config.json` as the desired state while treating daemon diagnostics and dashboard endpoints as runtime truth.
