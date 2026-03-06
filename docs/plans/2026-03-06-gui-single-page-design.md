# GUI Single-Page Easy-First Design

**Date:** 2026-03-06  
**Status:** Approved for implementation

## Context

The first GUI usability redesign significantly improved the product surface:

- visible in-app branding/icon,
- clearer terminology (`Services` instead of route-heavy language),
- reduced emphasis on diagnostics,
- lightweight tunnel defaults and settings model.

However, the GUI still carries too much navigation and too many major sections for the default use case.

The user’s updated product feedback is more aggressive:

- the GUI should feel like a single, obvious workspace,
- three top-level tabs are still too heavy,
- the default interaction should be “start tunnel, see URL, manage services”,
- settings should exist, but only behind a right-top entry point,
- troubleshooting should stay hidden unless needed,
- onboarding should eventually guide first use, but the steady-state shell should remain one page.

## Goals

- Collapse the primary GUI experience into a single page.
- Make the main screen sufficient for the common flow:
  - start tunnel,
  - see public URL,
  - add/edit/delete services,
  - check service status.
- Move settings behind a compact right-top gear button.
- Keep troubleshooting secondary and hidden by default.
- Preserve service-centric language and avoid exposing raw route terminology.

## Non-Goals

- No return to multi-tab primary navigation.
- No route-primitive editing surface on the main page.
- No diagnostics dashboard on the default surface.
- No new daemon-lifecycle behavior in this UI iteration.
- No multi-page settings flow.

## User-Confirmed Scope

The user explicitly confirmed:

- one main page is preferred over three tabs,
- when nothing is running, the page should emphasize an immediate start action,
- when services exist, the page should show the list directly,
- services should expose status plus edit/delete/add actions,
- settings should be a single entry point in the top-right corner,
- later onboarding may guide first use, token setup, or initial service creation.

## Approaches Considered

### 1. Keep the `Home / Services / Settings` tab model

**Pros**
- Already implemented.
- Clear internal separation.

**Cons**
- Still feels heavier than necessary.
- Makes users decide where to go before they have results.
- Contradicts the latest user guidance.

**Decision:** Rejected.

### 2. Single page with drawers for service editing and settings

**Pros**
- Keeps the core workflow visible at all times.
- Maintains context while editing services.
- Makes settings available without dominating the main experience.
- Matches the desired “one page is enough” product feel.

**Cons**
- Requires tighter UI discipline so the page does not become crowded.
- Needs careful hierarchy for empty/running/error states.

**Decision:** Recommended.

### 3. Wizard-first primary experience

**Pros**
- Strong first-run guidance.
- Extremely newcomer-friendly.

**Cons**
- Adds friction for repeat use.
- Risks turning normal usage into repeated onboarding.
- Better treated as a later enhancement than the default shell.

**Decision:** Rejected as the primary shell model.

## Recommended Design

### Core Structure

The GUI should have:

- **one main page**
- **one settings drawer**
- **one service editor drawer**
- **one on-demand troubleshooting surface**

No primary tab bar should remain.

### Main Page

The main page should contain three stacked sections:

1. **Header**
   - brand icon and product name,
   - concise status text,
   - right-top settings gear button.

2. **Tunnel Summary**
   - current public URL when available,
   - current running status,
   - primary action:
     - `Start Tunnel` when not running,
     - `Copy URL` when running,
   - secondary action:
     - `Stop Tunnel` when running.

3. **Services List**
   - if no services exist:
     - empty state with `Add Service`,
   - if services exist:
     - list of services directly on the main page.

### Service List

Each service row/card should show:

- service name,
- local service URL,
- public path or public host/path summary,
- current enabled state,
- lightweight service health / issue indicator where available,
- actions:
  - edit,
  - delete,
  - add.

The list itself is the main operational view. Users should not need to switch contexts to see it.

### Service Editor

Service add/edit should open in a **right-side drawer**, not a full page and not a blocking modal.

Default fields:

- service name,
- local service URL,
- public path.

Advanced disclosure:

- exposure mode,
- health check path,
- fallback local URL.

This keeps editing lightweight while preserving room for the few approved advanced options.

### Settings Entry

Settings should be opened from a **right-top gear button**.

The settings drawer should include:

- connection settings:
  - base URL,
  - bearer token,
- tunnel settings:
  - default provider,
  - gateway target URL,
  - auto restart,
  - `ngrok` authtoken,
  - `ngrok` reserved domain.

It should not be a major navigation destination. It is a support surface.

### Troubleshooting

Troubleshooting remains secondary:

- hidden by default,
- opened from:
  - a `View details` affordance when something fails,
  - or a low-emphasis entry near the bottom of settings.

The key product rule is that most users should never need to open it during normal operation.

### Empty / First-Run State

When the app first opens:

- if the tunnel is not running, the page should emphasize:
  - `Start Tunnel`,
  - `Add Service`.

Future onboarding may add:

- one-click first-service setup,
- a guided token/settings prompt when necessary.

But the steady-state shell remains the same single page.

## UX Principles

- Results first.
- Settings second.
- Troubleshooting last.
- Keep users on one page whenever possible.
- Editing should preserve context.

## Testing Strategy

Verification for this redesign should include:

- GUI-focused Rust tests still passing,
- static structure checks ensuring the primary tab bar is removed,
- manual Tauri smoke run verifying:
  - one-page shell,
  - settings gear opens settings drawer,
  - service edit/add uses a drawer,
  - tunnel start/stop remains accessible,
  - troubleshooting is hidden by default.

## Success Criteria

This redesign is successful when:

- users no longer need to understand multiple top-level sections,
- the first obvious action is starting the tunnel or managing services,
- services are always visible on the main page,
- settings no longer compete with the main flow,
- the GUI feels lightweight enough that one page is truly sufficient.
