# Zero-Enabled Service Follow-Through Design

**Date:** 2026-03-09

**Scope**
- Distinguish `no services yet` from `saved services exist but all are disabled` in the main dashboard guidance.
- Reuse the existing services-panel highlight flow instead of inventing a new recovery path.
- Keep the easy-first home screen aligned with the current README and GUI direction.

## Problem

The home hero currently decides its next-step guidance from enabled service count alone. That treats two different states as the same:

- a brand-new tunnel with no saved services; and
- a running tunnel with saved services that are all disabled.

In the second case, the UI suggests creating another service instead of reviewing or enabling the one that already exists.

## Recommendation

Use both counts that the GUI already knows about:

- **`route_count === 0`** → keep the current `Add Service` handoff.
- **`route_count > 0 && enabled_services === 0`** → switch the handoff to `Review Services`.

Apply the same distinction to:

- the dashboard hero copy shown before sharing; and
- the start-success status action after a tunnel begins running.

Keep the follow-through lightweight by routing `Review Services` back to the existing `highlightServicesPanel()` affordance.

## Non-Goals

- No daemon API changes.
- No new services editor flows.
- No changes to advanced troubleshooting panels.

## Testing

- Add focused JS helper coverage for zero-total-service versus zero-enabled-service dashboard guidance.
- Add focused JS helper coverage for start-success action selection.
- Add focused UI wiring coverage that `Review Services` hero and status actions reuse `highlightServicesPanel()`.
