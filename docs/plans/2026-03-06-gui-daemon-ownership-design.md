# GUI Daemon Ownership Design

**Date:** 2026-03-06  
**Status:** Approved for implementation

## Context

The current GUI usability redesign made the app significantly easier to understand:

- `Home` now highlights the current public URL and tunnel controls,
- `Services` replaces route-heavy language,
- `Settings` and `Troubleshooting` are pushed behind secondary surfaces,
- visible in-app branding is present.

That improves comprehension, but it does **not** yet solve first-run friction. The core remaining cost is still operational:

- users must start `tunnelmuxd` manually before the GUI becomes useful,
- the GUI remains a pure API client,
- disconnected state is still a setup problem for new users.

If the product promise is “one-click local tunnel management”, the GUI must be able to prepare the local control-plane itself.

## Goals

- Allow the GUI to launch `tunnelmuxd` automatically when no daemon is reachable.
- Prefer a daemon bundled with the GUI installer/app when available.
- Fall back to `tunnelmuxd` on `PATH` for development and power users.
- Distinguish between an externally managed daemon and a GUI-managed daemon.
- Stop only the daemon that the GUI itself started when the GUI exits.
- Preserve the current daemon API model and avoid turning the frontend into a process manager UI.

## Non-Goals

- No tray/background mode in this iteration.
- No daemon persistence after the GUI exits in this iteration.
- No multi-daemon or multi-profile support.
- No redesign of the daemon HTTP API contract.
- No installer/service-manager integration such as launchd, systemd, or Windows Services.

## User-Confirmed Scope

The user approved these product constraints:

- the GUI may prefer a bundled `tunnelmuxd`,
- if the bundled daemon is missing, the GUI may fall back to `PATH`,
- a GUI-started daemon should stop when the GUI closes,
- an externally started daemon must never be stopped by the GUI.

## Approaches Considered

### 1. Keep manual daemon startup and only improve disconnected messaging

**Pros**
- Minimal engineering cost.
- Preserves the current equal-client architecture exactly.

**Cons**
- Does not reduce first-run cost enough.
- Leaves the biggest onboarding pain untouched.
- Still requires terminal knowledge before the GUI is useful.

**Decision:** Rejected.

### 2. GUI auto-starts daemon and keeps it alive after GUI exit

**Pros**
- Feels closer to a full desktop background utility.
- Better long-lived operator workflow once set up.

**Cons**
- Requires careful orphan-process management.
- Introduces daemon lifecycle ambiguity immediately.
- Expands scope into background/tray behavior too early.

**Decision:** Rejected for the first daemon-ownership iteration.

### 3. GUI auto-starts daemon only for the current session

**Pros**
- Solves first-run friction directly.
- Keeps lifecycle rules simple and explicit.
- Minimizes stale background-process risk.
- Preserves room for future tray/background support without coupling it into the first delivery.

**Cons**
- GUI-managed tunnels stop when the GUI exits.
- Some users will eventually want persistent background behavior.

**Decision:** Recommended.

## Recommended Design

### Ownership Model

The GUI should recognize three daemon states:

1. **External**
   - a daemon is already reachable at the configured base URL,
   - the GUI did not start it,
   - the GUI must not stop it.

2. **Managed**
   - the GUI started the daemon itself,
   - the GUI owns its process handle,
   - the GUI should stop it during application shutdown.

3. **Unavailable**
   - no daemon is reachable,
   - no managed process is active.

This ownership distinction is the core safety boundary.

### Binary Resolution Order

When the GUI needs to start a daemon, it should resolve the executable in this order:

1. bundled sidecar/external binary shipped with the GUI,
2. `tunnelmuxd` found on the user’s `PATH`.

If both fail, the GUI should report a user-facing error that the local daemon binary could not be found.

This order supports both:

- installer users who expect everything to be included,
- developers/power users who already have local binaries available.

### Startup Flow

At GUI startup:

1. load GUI connection settings,
2. probe the configured daemon endpoint,
3. if reachable:
   - mark daemon state as `external`,
   - continue loading the normal dashboard/service UI,
4. if unreachable:
   - resolve a daemon binary,
   - start `tunnelmuxd`,
   - poll `/v1/health` until ready or timed out,
   - if ready, mark daemon state as `managed`,
   - if not ready, terminate the attempted child and show a lightweight failure state.

The frontend should not be responsible for this logic. The Rust/Tauri layer should own it and expose only the resulting connection/ownership state.

### Shutdown Flow

On GUI shutdown:

- if daemon state is `managed`, try a graceful stop of the child process,
- if graceful stop times out, force-kill the child,
- if daemon state is `external`, do nothing.

The application should never rely on PID-only heuristics when a managed child handle is available.

### Runtime State

GUI runtime state should be expanded to store:

- current daemon ownership kind (`external`, `managed`, or `unavailable`),
- optional child process handle / PID for a GUI-managed daemon,
- optional launch metadata for diagnostics/error reporting.

This is runtime-only state and should not be merged into the persisted service configuration model.

### Frontend Contract

The frontend should receive product-oriented status rather than low-level process detail.

Examples:

- `Starting local TunnelMux…`
- `Connected to local TunnelMux`
- `Using existing local TunnelMux`
- `Could not start local TunnelMux`

Detailed process errors should only appear in troubleshooting/details surfaces.

### Packaging Model

To support bundled startup, the GUI bundle needs to ship `tunnelmuxd` alongside `tunnelmux-gui`.

This design assumes Phase 2A will add:

- a Tauri sidecar or bundled external binary entry for `tunnelmuxd`,
- release packaging updates so GUI artifacts contain the daemon binary,
- developer-mode compatibility when running from source.

This does not require changing the public daemon archive release path.

## UX Impact

### Successful Path

1. User opens GUI.
2. GUI checks for an existing daemon.
3. If one exists, it connects immediately.
4. Otherwise, the GUI starts the local daemon.
5. User sees the usual `Home` screen instead of a disconnected setup dead-end.

### Failure Path

If local daemon startup fails, the GUI shows a lightweight product error state with:

- retry,
- open settings,
- view details.

It should not dump raw process-control concepts into the default screen.

## Architecture Notes

This iteration intentionally keeps the current API-first design:

- `tunnelmuxd` remains the control-plane authority,
- the GUI still talks to the daemon over the same HTTP API,
- the new behavior is that the GUI can also ensure the daemon exists locally.

This preserves the value of the existing daemon/CLI/GUI architecture while removing the main onboarding tax.

## Testing Strategy

Phase 2A should verify:

- external daemon detection does not spawn a duplicate daemon,
- managed daemon startup succeeds from a bundled or path-resolved binary,
- failed startup reports a clear error,
- GUI-managed daemon is stopped on GUI exit,
- external daemon survives GUI exit untouched.

Verification should include:

- Rust unit tests for daemon manager state transitions,
- integration tests for binary resolution order and ownership handling,
- manual smoke runs with:
  - no daemon running,
  - external daemon already running,
  - missing daemon binary.

## Success Criteria

This phase is successful when:

- opening the GUI is enough to make TunnelMux usable in the common case,
- installer users no longer need to manually start `tunnelmuxd`,
- the GUI never kills an externally managed daemon,
- lifecycle behavior remains simple enough that users can predict it.
