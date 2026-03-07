# Provider Status CTA Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add one contextual CTA to the provider-status card so users can immediately take the next relevant action.

**Architecture:** Extend `ProviderStatusVm` with optional action metadata and derive it in the GUI backend. The frontend renders one optional CTA button in the existing provider-status card and dispatches a small set of fixed UI actions.

**Tech Stack:** Tauri 2, Rust, vanilla HTML/CSS/JS

---

### Task 1: Extend provider status summaries with CTA metadata

**Files:**
- Modify: `crates/tunnelmux-gui/src/view_models.rs`
- Modify: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Write the failing test**
- Extend existing provider-status tests to assert:
  - named Cloudflare setup => `open_cloudflare`
  - ngrok auth error => `open_settings`
  - upstream unreachable => `review_services`

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui provider_status_summary`

Expected: current summaries have no CTA metadata.

**Step 3: Write minimal implementation**
- Add optional `action_kind` and `action_label` to `ProviderStatusVm`.
- Populate them in the existing summary derivation helper.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui`

Expected: updated summary tests and existing GUI tests pass.

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/src/view_models.rs crates/tunnelmux-gui/src/commands.rs
git commit -m "feat: add provider status actions"
```

### Task 2: Render and wire the CTA button

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/styles.css`

**Step 1: Write the failing test**
- Add a focused UI-level regression if possible, or rely on command-level action metadata tests and frontend syntax validation.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui && node --check crates/tunnelmux-gui/ui/app.js`

Expected: CTA render/binding path is still missing.

**Step 3: Write minimal implementation**
- Add one CTA button to the provider-status card.
- Support:
  - `open_cloudflare`
  - `open_settings`
  - `review_services`
- Hide the button when no action is present.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui && node --check crates/tunnelmux-gui/ui/app.js`

Expected: GUI tests pass and frontend syntax is valid.

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/styles.css docs/plans
git commit -m "feat: wire provider status cta"
```
