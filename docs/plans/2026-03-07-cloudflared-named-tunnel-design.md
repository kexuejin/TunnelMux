# Cloudflared Named Tunnel Design

**Date:** 2026-03-07

**Scope**
- Add a provider-aware `cloudflared` advanced mode that supports named tunnels via tunnel token.
- Keep the default experience as quick tunnel / one-click start.
- Avoid expanding into multi-tunnel orchestration or full Cloudflare Zero Trust management in this change.

## Problem

The GUI currently treats `cloudflared` as quick tunnel only, while `ngrok` already has provider-specific fields. This creates two issues:

1. `cloudflared` users cannot use a stable named tunnel from the GUI.
2. The settings model implies `cloudflared` is inherently temporary, even though it supports longer-lived managed tunnels.

## Design Goals

- Preserve the simple path: no token means quick tunnel.
- Add a stable path: token means named tunnel.
- Keep provider-specific settings conditional so the drawer stays small.
- Reuse the existing single-tunnel architecture.
- Do not require certificate or dashboard API automation in this change.

## Options Considered

### Option A: Keep quick tunnel only
- Pros: no new logic.
- Cons: does not address the actual product gap.

### Option B: Add token-backed named tunnel mode
- Pros: small surface area, works with current single-tunnel model, stable public hostname can be managed in Cloudflare.
- Cons: hostname / Access policy still configured outside TunnelMux.

### Option C: Full Cloudflare tunnel management in GUI
- Pros: most complete.
- Cons: requires certificate handling, dashboard/API orchestration, more settings, much higher risk.

**Recommendation:** Option B.

## Approved Behavior

- `cloudflared` remains the default provider.
- If `cloudflared` has no token configured, TunnelMux starts a quick tunnel using the current local gateway target URL.
- If a `cloudflared` token is configured, TunnelMux starts `cloudflared` in named tunnel mode against the same local gateway target URL.
- `ngrok` settings remain conditional to `ngrok`.
- `cloudflared` settings remain conditional to `cloudflared`.
- The GUI copy must explain that hostname and Access are managed in Cloudflare when token mode is used.

## Architecture

### GUI settings
- Add `cloudflared_tunnel_token` to persisted GUI settings.
- Show the field only when `cloudflared` is selected.
- Keep existing `ngrok` fields hidden when `cloudflared` is selected.

### Start tunnel request
- Extend metadata generation so provider-specific metadata can include:
  - `cloudflaredTunnelToken`
  - existing `ngrokAuthtoken`
  - existing `ngrokDomain`

### Daemon runtime
- When provider is `cloudflared`:
  - if token metadata exists, run `cloudflared tunnel --no-autoupdate run --token <token> --url <target_url>`
  - otherwise run the current quick tunnel command

This keeps one tunnel runtime with one public URL while allowing a stable Cloudflare-managed tunnel path.

## Error Handling

- Empty token falls back to quick tunnel mode.
- Invalid token or startup failure surfaces through the existing provider startup error path.
- No migration break for existing settings files; missing field defaults to `None`.

## Testing

- GUI settings round-trip must preserve `cloudflared_tunnel_token`.
- GUI metadata builder must include the token.
- GUI `start_tunnel` must forward Cloudflare token metadata.
- Daemon runtime must build the token-backed `cloudflared` command when metadata is present.

## Non-Goals

- No automatic Cloudflare hostname provisioning.
- No Access policy creation or editing.
- No certificate handling or `cloudflared login` flows.
- No multi-tunnel runtime changes.
