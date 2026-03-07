# GUI Empty State and Recovery Design

**Date:** 2026-03-07

**Scope**
- Fix the current services empty-state crash in the single-page GUI.
- Make first-run and no-route behavior immediately understandable.
- Keep the local daemon alive across GUI restarts so reopening the app reconnects to the current tunnel instead of showing a misleading stopped state.

## Problem

The current GUI has two user-facing failures:

1. The services workspace can crash on empty data because the frontend still references a removed `services-count` element.
2. When there are no routes, the user sees an empty list and may start a tunnel that has nothing meaningful to serve.
3. The GUI currently owns daemon lifetime. Closing the GUI stops the daemon, so reopening the GUI often creates a fresh daemon process that loads the previous persisted status as `stopped`, which feels like an unexpected tunnel loss.

## Design Goals

- No crash on empty routes.
- First-run experience should explain what TunnelMux needs from the user in one screen.
- No fake user service should be auto-created behind the user's back.
- Closing and reopening the GUI should preserve the current daemon and reconnect to it.
- Recovery should remain explicit and low-risk; avoid creating duplicate provider tunnels.

## Options Considered

### Option A: Auto-create a real default service
- Pros: tunnel can immediately route somewhere.
- Cons: requires guessing a local upstream port; highly error-prone and misleading.

### Option B: Show an in-UI empty state only
- Pros: minimal implementation.
- Cons: tunnel can still be live while public requests fail with no useful landing page.

### Option C: In-UI empty state plus built-in welcome fallback
- Pros: first-run experience is self-explanatory, no guessed upstream, public requests get a helpful landing page.
- Cons: requires a small amount of system-owned route/fallback logic.

**Recommendation:** Option C, but scoped minimally: fix the GUI empty state now and introduce a built-in welcome/fallback page only when there are no user routes. This keeps onboarding clear without creating fake user services.

## Approved Behavior

### 1. Empty Services Workspace
- The services panel must never throw when the route list is empty or partially unavailable.
- When there are no user routes:
  - keep the services list area visible;
  - show a short onboarding card;
  - keep a prominent `Add Service` action visible;
  - show copy that explains the next step: add a local service URL.

### 2. Built-in Welcome Fallback
- If there are no user-defined routes, TunnelMux should expose a built-in welcome page from the local gateway.
- The welcome page is system-owned, not a normal editable route.
- Once the user adds a real route, the fallback should stop being the primary experience.

### 3. GUI / Daemon Lifetime
- Closing the GUI must no longer stop the managed daemon.
- Reopening the GUI should connect to the still-running daemon and reflect current tunnel status.
- The GUI should treat a reachable existing daemon as the canonical runtime, regardless of whether the current GUI process originally spawned it.

### 4. Recovery Model
- If the daemon is still alive, reconnect and show the live tunnel.
- If the daemon is gone, do not silently recreate a provider tunnel just from stale persisted state.
- Instead, show the actual stopped/error state and allow the user to explicitly restart with `Start Tunnel`.

This avoids duplicate provider sessions after an unexpected daemon restart.

## Architecture

### Frontend
- Remove stale DOM assumptions from `app.js`.
- Add explicit empty-state rendering helpers so the services panel can switch between:
  - user route list;
  - onboarding empty state;
  - loading/error copy.

### GUI daemon lifecycle
- Remove the GUI shutdown hook that stops the managed daemon.
- Preserve daemon bootstrap on app startup.
- On startup, if the daemon health endpoint is reachable, reconnect to it and show live status.

### Welcome fallback
- Implement the fallback in `tunnelmuxd`, not in the GUI.
- The fallback should be served by the local gateway when no user routes match.
- It should not be persisted as a user route and should not appear as an editable service card.

## Error Handling

- Empty routes must render safely even if some DOM nodes are absent.
- Failed route loads should degrade to an error message plus visible `Add Service`.
- If the daemon health endpoint is reachable but tunnel status cannot be loaded, show daemon connected / tunnel unavailable, not a generic crash.

## Testing

- Add frontend-safe rendering coverage by verifying empty-state DOM assumptions in the existing JS shell logic.
- Add Rust tests for daemon reconnect behavior where a reachable daemon is treated as active on GUI startup.
- Add daemon-side tests for welcome fallback behavior when there are zero routes.

## Non-Goals

- No automatic creation of guessed upstream services like `localhost:3000`.
- No daemon auto-restart of stale provider sessions purely from persisted `running` state.
- No redesign of the full dashboard layout in this change.
