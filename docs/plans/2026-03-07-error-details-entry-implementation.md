# Error Details Entry Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Hide diagnostics from the default page and expose them only through a `View Error Details` action when the current status is an error.

**Architecture:** Keep diagnostics data-loading logic intact, but move the diagnostics shell behind a modal-style panel. A small pure helper decides whether the header should show the error-details action.

**Tech Stack:** Tauri 2, vanilla HTML/CSS/JS, Node test runner

---

### Task 1: Add failing helper test

**Files:**
- Modify: `crates/tunnelmux-gui/ui/app.test.mjs`
- Modify: `crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs`

**Step 1: Write the failing test**
- Add a helper test for status action visibility:
  - error state → show error details action
  - non-error state → hide action

**Step 2: Run test to verify it fails**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`

Expected: FAIL on missing helper export.

**Step 3: Write minimal implementation**
- Add the helper to the shared UI helper module.

**Step 4: Run test to verify it passes**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`

Expected: PASS

### Task 2: Replace inline details block with modal entry

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing test**
- Use the helper test as the red/green guard for status-action logic.

**Step 2: Run test to verify it fails**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`

Expected: FAIL until app logic uses the helper correctly.

**Step 3: Write minimal implementation**
- Replace the main-page `<details>` shell with a hidden diagnostics modal.
- Add `View Error Details` button in the status row.
- Open only on error.
- Close on backdrop and `Esc`.

**Step 4: Run test to verify it passes**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs && node --check crates/tunnelmux-gui/ui/app.js`

Expected: PASS

### Task 3: Final verification

**Files:**
- Verify only

**Step 1: Run GUI tests**
Run: `cargo test -p tunnelmux-gui`

Expected: PASS

**Step 2: Run frontend verification**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs && node --check crates/tunnelmux-gui/ui/app.js`

Expected: PASS

