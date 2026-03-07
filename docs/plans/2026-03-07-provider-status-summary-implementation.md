# Provider Status Summary Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a concise provider-status summary card to the main GUI using existing tunnel status and recent provider logs.

**Architecture:** The GUI backend derives a compact summary from recent provider logs plus the current dashboard/tunnel snapshot. The frontend renders a small card in the main page and refreshes it alongside the existing dashboard.

**Tech Stack:** Tauri 2, Rust, vanilla HTML/CSS/JS

---

### Task 1: Add a provider-status view model and parser

**Files:**
- Modify: `crates/tunnelmux-gui/src/view_models.rs`
- Modify: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Write the failing test**
- Add parser tests for:
  - cloudflared named tunnel running without public URL;
  - ngrok auth/domain error line;
  - upstream unreachable line.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui provider_status`

Expected: the parser/helper does not exist yet.

**Step 3: Write minimal implementation**
- Add `ProviderStatusVm`.
- Add a helper that derives the summary from tunnel status + recent log lines.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui`

Expected: parser tests and existing GUI tests pass.

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/src/view_models.rs crates/tunnelmux-gui/src/commands.rs
git commit -m "feat: derive provider status summary"
```

### Task 2: Render the summary card in the main GUI

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/styles.css`

**Step 1: Write the failing test**
- Add a command-level regression ensuring the summary endpoint returns a meaningful result for mocked provider logs.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui provider_status`

Expected: no summary endpoint or summary payload yet.

**Step 3: Write minimal implementation**
- Add the card markup.
- Load the summary alongside dashboard/routes.
- Keep it hidden when no summary is available.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui && node --check crates/tunnelmux-gui/ui/app.js`

Expected: GUI tests pass and frontend syntax remains valid.

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/styles.css
git commit -m "feat: show provider status summary"
```
