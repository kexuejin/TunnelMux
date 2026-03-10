# Diagnostics Daemon Unavailable Copy Design

**Goal:** Keep diagnostics requests in the GUI, but replace raw request transport failures with a clearer daemon-unavailable message when the local TunnelMux daemon cannot be reached.

## Problem

`Start Tunnel` failures can leave the GUI in a state where the diagnostics drawer still requests runtime summary, upstream health, and recent logs. When the local daemon is not reachable at the configured base URL, the UI currently surfaces raw transport errors such as `request failed: http://127.0.0.1:4765/v1/upstreams/health`.

That message is technically accurate but poor UX. It exposes an internal URL and makes the user diagnose a secondary symptom instead of the real problem: the local daemon is unavailable.

## Decision

Keep the existing diagnostics requests and add a shared frontend error summarizer for diagnostics panels.

The summarizer will:

- detect daemon-unavailable request failures from transport-style messages,
- return a friendly section-specific message for runtime summary, upstream health, and recent logs,
- preserve existing raw error behavior for unrelated failures.

## Scope

Modify:

- `crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs`
- `crates/tunnelmux-gui/ui/app.js`
- `crates/tunnelmux-gui/ui/app.test.mjs`

No Rust API changes are required because the control-plane endpoints and command wrappers already behave correctly in tests.

## UX Copy

When the daemon is unavailable:

- Runtime summary: `Local TunnelMux daemon is unavailable, so runtime summary is unavailable right now.`
- Upstream health: `Local TunnelMux daemon is unavailable, so upstream health is unavailable right now.`
- Recent logs: `Local TunnelMux daemon is unavailable, so recent logs are unavailable right now.`

For non-daemon failures, keep the existing `Failed to load ...` format.

## Verification

- Add failing frontend tests for the summarizer and `app.js` wiring.
- Run `node --test crates/tunnelmux-gui/ui/app.test.mjs`.
- Run `node --check crates/tunnelmux-gui/ui/app.js`.
