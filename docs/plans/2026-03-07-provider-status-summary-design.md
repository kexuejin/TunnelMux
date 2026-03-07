# Provider Status Summary Design

**Date:** 2026-03-07

**Scope**
- Surface one concise provider-status summary in the main GUI.
- Reuse existing tunnel log data instead of introducing a new daemon API.
- Keep Details as the deep-dive view.

## Problem

The GUI now supports more tunnel modes, but the user still needs to open Details to infer whether the provider actually connected, hit an auth problem, or is waiting on external setup.

## Design Goals

- Show one short provider summary on the main page.
- Prefer actionable messages over raw logs.
- Avoid another monitoring-heavy panel.
- Keep the logic provider-aware.

## Recommendation

Add a lightweight `Provider Status` card on the main page that displays:

- **success** when a provider tunnel is clearly up;
- **warning** when the provider is up but external setup is still needed;
- **error** when startup/auth/upstream issues are detected from recent logs.

The summary is derived in the GUI backend from:
- current tunnel snapshot;
- a short tail of provider logs.

## Heuristics

- `cloudflared` named tunnel + running + no public URL:
  - “Named tunnel connected. Configure hostname and Access in Cloudflare.”
- quick `cloudflared` log containing a trycloudflare URL:
  - “Quick tunnel active.”
- `ngrok` log containing `ERR_NGROK_*`:
  - show the code and a short auth/domain hint.
- provider/upstream connection failures:
  - “Tunnel started, but the local service is unreachable.”

## Non-Goals

- No log viewer replacement.
- No long historical state.
- No new daemon-side persistence or metrics model.
