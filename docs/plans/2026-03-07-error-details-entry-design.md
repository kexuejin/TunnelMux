# Error Details Entry Design

**Date:** 2026-03-07

**Scope**
- Remove the always-visible `View Details` section from the main page.
- Show an error-details entry only when the current top status message is an error.
- Reuse the existing diagnostics and logs content behind a modal-style surface.

## Problem

The current `View Details` section introduces always-visible secondary UI that most users do not need.

This hurts the “one-click, one-page, low-friction” goal:
- it adds noise even when everything is healthy
- it suggests troubleshooting is a primary workflow
- it forces users to visually scan an element that is usually irrelevant

## Decision

Use **conditional error-details entry** only.

- no default `View Details`
- no persistent troubleshooting block on the page
- only show `View Error Details` next to the status message when the status is currently an error

## UI Model

### Header status row
- keep the existing top status text
- add a right-aligned action button only when `renderStatus(..., true)` is active
- button label: `View Error Details`

### Error details surface
- replace the inline `<details>` block with a hidden modal/drawer-like panel
- open it only from the error action button
- include:
  - current error message
  - runtime summary
  - recent logs
- upstream health can remain available if already present, but should not dominate the surface

## Interaction

- error action button appears only for error states
- clicking the button opens the diagnostics modal
- clicking backdrop or close button closes it
- `Esc` closes it
- opening the modal triggers diagnostics/log refresh if needed

## Non-Goals

- no extra navigation
- no retry flow redesign
- no toast system
- no diagnostics redesign beyond hiding the entry point

## Testing

- add a small helper-level Node test for status action visibility
- run `node --check` for frontend syntax
- keep GUI Rust tests green

