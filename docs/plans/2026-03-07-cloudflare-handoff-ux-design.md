# Cloudflare Handoff UX Design

**Date:** 2026-03-07

**Scope**
- Improve the GUI experience for `cloudflared` named tunnel mode.
- Make it explicit that hostnames and Access policies are managed in Cloudflare.
- Add direct handoff actions instead of pretending TunnelMux manages Cloudflare resources.

## Problem

After adding token-backed named tunnel support, the runtime can now be healthy without a locally discoverable public URL. That is technically correct, but the UX still leaves the user with an incomplete next step: they need an obvious place to go to manage hostname and Access.

## Design Goals

- Preserve one-click quick tunnel behavior.
- Keep named tunnel mode honest and low-friction.
- Add actionable handoff points to Cloudflare without adding fake local settings.
- Avoid introducing Cloudflare API/state management into TunnelMux.

## Recommendation

Add provider-specific handoff actions for `cloudflared`:

- In Settings, when `cloudflared` is selected:
  - show a short explanation for quick vs named tunnel mode;
  - show `Open Cloudflare Dashboard`;
  - show `Open Tunnel Docs`.
- In the dashboard hero, when a named tunnel is running without a locally known public URL:
  - show `Open Cloudflare` as a contextual action.

## Non-Goals

- No hostname field stored locally.
- No Access policy editor.
- No Cloudflare API integration.
