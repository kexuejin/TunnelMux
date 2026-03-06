# GUI Usability Redesign Design

**Date:** 2026-03-06  
**Status:** Approved for implementation

## Context

TunnelMux now ships a desktop GUI, but the current experience still feels like a control console:

- the first screen exposes multiple workspaces and a large amount of operational detail,
- the UI surface does not present a visible in-app brand/icon despite shipping installer/window icon assets,
- route management is modeled directly as route editing instead of as service management,
- diagnostics and monitoring are overly prominent for the majority of users,
- the GUI still assumes an already-running `tunnelmuxd`, which limits true one-click onboarding.

The current backend product model remains:

- **single tunnel**
- **multiple local service routes**

That model is still correct. The usability problem is primarily information architecture and interaction design, not backend capability mismatch.

## Goals

- Make the GUI feel like a lightweight desktop product instead of an operations console.
- Optimize the first-run and daily-use path for “start tunnel, get URL, add service, copy URL”.
- Replace route-centric language with service-centric language where possible.
- Keep advanced behavior available without making it a first-class burden.
- Add a visible in-app brand/icon treatment using existing app assets.
- Reduce monitoring and diagnostics to an on-demand troubleshooting flow.

## Non-Goals

- No multi-tunnel management in this iteration.
- No route-primitive editor exposed as a first-class GUI workflow.
- No large monitoring dashboard or observability center.
- No expansion of provider-specific advanced configuration beyond high-value fields.
- No backend redesign of the single-tunnel / multi-route model.

## User-Confirmed Scope

The user explicitly confirmed these constraints:

- the GUI must prioritize ease of use and one-step onboarding,
- “advanced” service management must still be **converged and simple**,
- service-level advanced configuration should be limited to:
  - exposure mode,
  - health check,
  - fallback upstream,
- provider configuration belongs to tunnel-level settings, not per-service editing,
- heavy monitoring should be de-emphasized because most users do not need it.

## Approaches Considered

### 1. Keep the current console model and visually simplify it

**Pros**
- Lowest implementation cost.
- Reuses most of the current workspace structure.

**Cons**
- Preserves the wrong mental model.
- Still teaches users about operations, routes, and diagnostics too early.
- Does not feel “one-click” even with cleaner styling.

**Decision:** Rejected.

### 2. Dual-layer product UI: easy-first home + service management + hidden troubleshooting

**Pros**
- Matches the desired product goal: reduce usage cost.
- Keeps advanced capability without pushing it to every user.
- Aligns with the current backend model of one tunnel with many routes.
- Allows a clear migration path toward daemon lifecycle ownership later.

**Cons**
- Requires substantial frontend re-organization.
- Needs some terminology and settings reshaping.

**Decision:** Recommended.

### 3. Wizard-first application for every use

**Pros**
- Very easy for first-time users.
- Strongly guided onboarding.

**Cons**
- Becomes annoying for repeat daily use.
- Makes simple repeat operations slower.
- Adds stateful onboarding logic that does not age well.

**Decision:** Rejected as the default model.

## Recommended Design

### Product Model

The GUI should present TunnelMux as:

- one current tunnel connection,
- one current public URL,
- multiple attached services exposed through that tunnel.

The GUI should **not** lead with:

- route rules,
- diagnostics workspaces,
- runtime metrics,
- route internals such as `match_host`, `strip_path_prefix`, or raw upstream wiring.

### Navigation Model

Primary navigation should be reduced to:

- `Home`
- `Services`
- `Settings`

Diagnostics should no longer be a top-level daily-use workspace. Instead, it becomes:

- `Troubleshooting`
- opened only from:
  - an explicit secondary entry in settings/advanced UI, or
  - an error / degraded-state “View details” action.

### Home

`Home` is the result page, not a control panel.

It should contain only the highest-value surfaces:

1. **Brand header**
   - visible icon / logo treatment,
   - concise product name and state,
   - minimal secondary controls.

2. **Public URL card**
   - current tunnel URL if running,
   - `Copy URL`,
   - `Open`,
   - current provider shown as lightweight metadata.

3. **Tunnel control card**
   - provider selection,
   - `Start Tunnel`,
   - `Stop Tunnel`.

4. **Service summary card**
   - number of services,
   - lightweight health summary,
   - `Manage Services`.

No long forms or monitoring panels should appear on `Home`.

### Services

The current `Routes` workspace should be reframed as `Services`.

Users should manage “services being exposed”, not “route definitions”.

The page should show service cards with:

- service name,
- local service URL,
- public path or subdomain,
- enabled / issue state,
- actions: edit, enable/disable, delete.

### Service Editor

Service editing should happen in a drawer or modal, not in a persistent heavy form.

#### Basic fields

- Service name
- Local service URL
- Public path

#### Advanced fields

Advanced mode must remain intentionally narrow:

1. **Exposure**
   - path or subdomain
2. **Health Check**
   - enabled + path
3. **Fallback**
   - optional fallback local URL

The following route primitives should remain hidden behind GUI translation and should not be shown directly:

- `match_host`
- `match_path_prefix`
- `strip_path_prefix`
- raw route JSON

### Settings

Settings should be split conceptually into:

- **Connection Settings**
  - base URL
  - control-plane bearer token
- **Tunnel Settings**
  - default provider
  - auto restart
  - provider-specific settings

Provider-specific configuration should live here because it is tunnel-level behavior, not per-service behavior.

#### Provider-specific scope

For the first iteration:

- `cloudflared`
  - default provider selection
  - auto restart
- `ngrok`
  - authtoken
  - reserved domain (optional)

This keeps the settings small while still covering the highest-value provider options currently supported by the runtime.

### Troubleshooting

Troubleshooting should be a second-line tool, not a primary screen.

Its job is to explain failure when needed, not to act as a live observability dashboard.

Daily users should only see:

- connected / not connected,
- running / not running,
- service healthy / issue.

Detailed views such as:

- runtime summary,
- upstream health,
- recent logs,

should appear only when the user explicitly asks for details or when an error state prompts them to investigate.

### Visible Branding / Icon

The GUI should visibly reuse the shipped icon assets inside the app itself.

This means:

- the header includes a recognizable app mark,
- the app feels branded when opened,
- the installer/window icon assets are not the only place branding exists.

## First-Run and Daily-Use Flows

### First Run

1. Open app.
2. App checks whether `tunnelmuxd` is reachable.
3. If reachable and tunnel is running:
   - show current public URL immediately.
4. If reachable and tunnel is not running:
   - show lightweight tunnel-start card.
5. If not reachable:
   - show a minimal “TunnelMux is not ready” state with:
     - retry,
     - open settings.

### Daily Use

1. Open app.
2. Copy current public URL or start tunnel.
3. Add / edit service if needed.
4. Leave troubleshooting hidden unless something breaks.

## Phased Delivery

### Phase 1: Productize the GUI shell

Deliver:

- `Home / Services / Settings` navigation,
- visible in-app branding/icon,
- service-card management,
- simplified editor,
- diagnostics demoted to troubleshooting.

This phase solves the immediate usability issue without requiring backend lifecycle changes.

### Phase 2: True one-click onboarding

Deliver GUI ownership of local daemon lifecycle:

- detect local `tunnelmuxd`,
- start it when absent,
- manage its lifecycle from the desktop app.

This is the phase that materially unlocks “one-click onboarding”.

### Phase 3: Carefully chosen advanced capability

Deliver only high-value advanced configuration:

- `ngrok` auth/domain,
- fallback upstream,
- health checks,
- other proven high-frequency needs.

No large monitoring center should be added unless real usage justifies it.

## Testing Strategy

Phase 1 verification should focus on:

- `cargo test -p tunnelmux-gui`
- `cargo check -p tunnelmux-gui`
- manual smoke validation of:
  - connected running state,
  - connected stopped state,
  - connection failure state,
  - add/edit/delete service,
  - troubleshooting only opening on demand.

## Success Criteria

This redesign is successful when:

- first-time users can understand the app in seconds,
- the first visible goal is the public URL, not the control plane,
- service management feels like app/service exposure, not route editing,
- diagnostics stop competing for attention with the core workflow,
- the GUI feels like a product surface rather than an operator console.
