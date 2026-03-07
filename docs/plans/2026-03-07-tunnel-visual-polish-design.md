# Tunnel Visual Polish Design

**Date:** 2026-03-07

**Scope**
- Refine the current tunnel summary bar and custom tunnel picker.
- Improve state readability without changing the one-page information architecture.
- Keep the UI focused on “current tunnel + current services”.

## Problem

The custom picker now exposes the right information, but the visual hierarchy is still too flat:

- the selected tunnel and the running tunnel do not stand apart enough
- `starting` and `error` need stronger visual differentiation
- the current tunnel summary can communicate public reachability more clearly

At this stage, structure is good enough. The remaining issue is visual clarity, not missing features.

## Decision

Apply a **light visual polish pass** only.

Do not add new panels, tabs, or drawers.

Do not add more explanation text.

## Changes

### Current tunnel summary
- Keep the current tunnel name and state badge.
- Make the secondary line more useful:
  - provider
  - state
  - enabled/total services
- Show a public URL line only when a public URL exists.
- Keep empty text short when no public URL is active.

### Tunnel picker rows
- Highlight the selected tunnel row more clearly.
- Increase visual separation between:
  - `running`
  - `starting`
  - `stopped`
  - `error`
- Keep row content compact:
  - name
  - provider/state/service count summary
  - state badge

### Empty-state copy
- Shorten onboarding text.
- Keep copy action-oriented and concrete.

## Non-Goals

- no new runtime data
- no keyboard navigation rewrite
- no extra settings or management surface
- no redesign of service cards or diagnostics layout

## Testing

- Extend helper-level Node tests for tunnel summary formatting.
- Keep existing GUI Rust tests green.
- Verify `app.js` with `node --check`.

