# Tunnel Picker Design

**Date:** 2026-03-07

**Scope**
- Replace the current native tunnel `<select>` with a lightweight custom picker.
- Keep the single-page model and current tunnel bar.
- Improve tunnel switching clarity without adding tabs or a dedicated management page.

## Problem

The current native `<select>` is functional but too constrained:

- it cannot show status clearly
- it cannot show service counts clearly
- it makes multi-tunnel state feel hidden
- it visually regresses once more than one tunnel exists

That conflicts with the product goal of “simple by default, clear when advanced”.

## Decision

Use a **custom popover tunnel picker** anchored in the current tunnel bar.

Do **not** add tabs.

Do **not** add a separate tunnel management page.

## Why

- keeps the default flow on one page
- gives enough room to show tunnel health and service counts
- avoids new navigation or layout complexity
- scales better than a native `<select>` once users add a second or third tunnel

## Interaction Model

### Trigger
- clicking the current tunnel switcher opens a compact popover
- trigger shows:
  - current tunnel name
  - current tunnel state
  - enabled/total services

### List items
Each tunnel row shows:
- tunnel name
- provider
- state badge
- enabled/total services

### Selection
- clicking a tunnel row closes the popover
- current tunnel switches immediately
- dashboard, services, diagnostics, and provider summary refresh
- status text confirms the switch

### Dismissal
- click outside closes
- `Esc` closes

## Visual Constraints

- one compact popover, not a full drawer
- max-height with scroll for longer lists
- selected tunnel row highlighted
- existing state colors reused:
  - `running`
  - `starting`
  - `stopped`
  - `error`

## Non-Goals

- fuzzy search
- keyboard roving focus
- reordering tunnels
- bulk actions
- separate tunnel overview page

## Data Model

Reuse existing GUI tunnel workspace data.

The picker depends on fields already available or already added in the current branch:
- tunnel id
- tunnel name
- provider
- state
- route count
- enabled route count
- optional public URL

No new backend endpoint is required.

## Error Handling

- if tunnel workspace fails to load, fall back to current status message
- if switch fails, keep the picker closed and show error status
- if no tunnels exist, keep the existing empty state

## Testing

- Rust GUI tests keep validating merged tunnel workspace data
- front-end verification ensures `app.js` remains valid
- no screenshot-golden testing required for this slice

