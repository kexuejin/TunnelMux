# Provider Status CTA Design

**Date:** 2026-03-07

**Scope**
- Add one primary action to the `Provider Status` card.
- Keep the card lightweight and contextual.
- Do not add a second status surface or a new settings hub.

## Problem

The provider-status summary now surfaces the right message, but it still leaves the user without a direct next action. The user should not need to infer whether they should open Cloudflare, go back to tunnel settings, or review local services.

## Design Goals

- One status card, one main CTA.
- CTA only appears when there is an actionable next step.
- Keep actions consistent with existing shell patterns.

## Decision

Use a single provider-status CTA with three action modes:

- `open_cloudflare`
  - for running `cloudflared` named tunnels that need hostname / Access setup.
- `open_settings`
  - for `ngrok` auth/domain/config errors that require provider settings changes.
- `review_services`
  - for upstream/local service unreachable cases.

When no next step is needed, hide the CTA.

## Interaction Model

- The CTA lives inside the existing `Provider Status` card.
- Clicking it performs one of:
  - open Cloudflare dashboard in browser;
  - open the existing settings drawer;
  - scroll/focus the services area so the user can inspect the current service list.

## Non-Goals

- No second button in the card.
- No per-route troubleshooting actions.
- No new modal or wizard.
