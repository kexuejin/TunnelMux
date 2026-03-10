# User-Managed Provider Install Design

**Date:** 2026-03-09

## Scope

- Let TunnelMux install `cloudflared` and `ngrok` into a user-owned tools directory.
- Make the GUI prefer those local provider binaries before looking at system `PATH`.
- Keep the current system-installer path as a fallback, not the primary flow.

## Problem

The current GUI can launch a provider install flow, but it still relies on system package managers:

1. `brew` / `winget` can require passwords, UAC, or other system authorization.
2. Installation is not truly owned by TunnelMux, so the app cannot reliably know which binary it should use.
3. Missing-provider recovery still depends on external environment state rather than an app-managed lifecycle.

## Goals

- Make provider installation work in a user-owned directory without administrator privileges in the common case.
- Keep provider resolution deterministic: local TunnelMux-managed tools first, then system `PATH`.
- Preserve clear recovery states in the GUI for missing, downloading, installed, and failed states.
- Avoid automatic upgrades or cross-version management in the first iteration.

## Options Considered

### Option A: User-managed provider tools directory

- TunnelMux downloads provider binaries into app data, verifies them, and launches them by absolute path.
- Pros: no `sudo` in the common path, deterministic runtime, app-owned recovery flow.
- Cons: requires download, integrity, and local version bookkeeping.

### Option B: Keep launching system installers

- TunnelMux continues opening Terminal / PowerShell for `brew` / `winget`.
- Pros: lowest implementation effort.
- Cons: still permission-sensitive and still not app-owned.

### Option C: Bundle providers inside the TunnelMux installer

- Ship providers together with the GUI bundle.
- Pros: best first-run experience.
- Cons: larger bundles, more release complexity, and distribution/licensing risk.

**Recommendation:** Option A.

## Approved Behavior

- TunnelMux installs providers into a per-user tools directory under app data.
- Provider resolution order becomes:
  1. TunnelMux-managed local tools
  2. existing system `PATH`
  3. missing
- Missing-provider CTA becomes an in-app install action.
- If local install fails, the UI offers retry and keeps a fallback path to the existing system install flow or docs.
- `Save and Start` / `Restart Tunnel` use the same recovery model whether the provider is missing before start or the daemon surfaces a missing-binary error.

## Architecture

### Local tools layout

Use a GUI-owned tools root such as:

- macOS: `~/Library/Application Support/com.tunnelmux.gui/tools/`
- Windows: app-local data equivalent
- Linux: app-local data equivalent

Recommended structure:

- `tools/manifests/providers.json` or equivalent installed-state file
- `tools/bin/cloudflared`
- `tools/bin/ngrok`
- `tools/cloudflared/<version>/...`
- `tools/ngrok/<version>/...`
- `tools/.tmp/...` for partial downloads

The stable `tools/bin/<provider>` entry is what TunnelMux resolves and passes to the daemon.

### State model

Track two concepts separately:

1. **Resolution**
   - `local_tools`
   - `system_path`
   - `missing`

2. **Install lifecycle**
   - `idle`
   - `downloading`
   - `installed`
   - `failed`

This lets the GUI distinguish:

- installed for TunnelMux
- installed elsewhere on the machine
- currently being installed
- install attempted but failed

### Download and verification

- Maintain a provider release manifest per OS/arch with:
  - provider name
  - version
  - platform key
  - download URL
  - checksum
- Download into `tools/.tmp/`
- Verify checksum before promotion
- Extract or place the provider binary into a versioned directory
- Mark the final binary executable where required
- Atomically swap the stable `tools/bin/<provider>` target only after success

### Runtime integration

- Extend provider resolution to look in the local tools directory before current `PATH` / common search directories.
- Keep daemon startup wiring unchanged except for preferring the resolved local tools path.
- Keep provider availability probing and GUI summaries aligned with the same resolution logic to avoid split-brain status.

## UI Flow

### Missing provider

- Primary action: `Install cloudflared` / `Install ngrok`
- Secondary action: `Recheck Provider`
- Optional tertiary path later: `Use System Install`

### Installing

- Primary action disabled
- Button label and/or status copy changes to `Installing…`
- Home, empty-state, and drawer surfaces show consistent progress copy

### Installed

- If source is `local_tools`, show copy like `Installed for TunnelMux`
- If source is `system_path`, show copy like `Available on this machine`

### Failed

- Show a friendly failure message
- Primary action becomes `Retry Install`
- Fallback action can open system install docs or the current external installer flow

## Error Handling

- Network failure: keep old version, discard partial download, show retry state
- Checksum mismatch: discard payload, mark install failed, do not promote
- Binary extraction or permission failure: keep old version, show retry state
- Start failure after local install: classify separately from missing-provider state so config errors still route back to tunnel fields

## Testing

- Rust tests for provider resolution priority: local tools over system path
- Rust tests for manifest parsing, checksum validation, and atomic promotion
- GUI helper tests for `missing -> downloading -> installed -> failed`
- Smoke test proving a locally installed provider becomes startable without touching global `PATH`

## Non-Goals

- No background auto-update
- No multi-version switcher UI
- No arbitrary custom provider URLs
- No bundling provider binaries into the GUI installer in this iteration
