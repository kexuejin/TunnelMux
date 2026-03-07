# Tunnel Visual Polish Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Improve tunnel readability in the current tunnel bar and custom picker without adding new UI surfaces.

**Architecture:** Keep the current single-page shell, extend pure front-end helper functions for summary/state formatting, and apply a focused HTML/CSS/JS polish pass. Reuse existing tunnel workspace data.

**Tech Stack:** Tauri 2, vanilla HTML/CSS/JS, Rust GUI tests, Node test runner

---

### Task 1: Add failing helper tests

**Files:**
- Modify: `crates/tunnelmux-gui/ui/app.test.mjs`
- Modify: `crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs`

**Step 1: Write the failing test**
- Add helper assertions for:
  - current tunnel public URL summary behavior
  - picker row class/state formatting

**Step 2: Run test to verify it fails**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`

Expected: FAIL on missing helper exports or wrong formatting.

**Step 3: Write minimal implementation**
- Extend helper module with the smallest formatting helpers needed by the UI.

**Step 4: Run test to verify it passes**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`

Expected: PASS

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/ui/app.test.mjs crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs
git commit -m "test: cover tunnel visual polish helpers"
```

### Task 2: Polish current tunnel summary

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing test**
- Reuse the helper test from Task 1 to pin the summary strings.

**Step 2: Run test to verify it fails**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`

Expected: FAIL until the summary helpers and usage line up.

**Step 3: Write minimal implementation**
- Update the current tunnel summary rendering to use the helper output.
- Only show public URL summary when one exists.
- Keep the copy concise.

**Step 4: Run test to verify it passes**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs && node --check crates/tunnelmux-gui/ui/app.js`

Expected: PASS

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js
git commit -m "feat: polish current tunnel summary"
```

### Task 3: Strengthen picker visual hierarchy

**Files:**
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing test**
- Use the helper tests as the red/green guard for row state formatting.

**Step 2: Run test to verify it fails**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`

Expected: FAIL if the helper logic does not match intended states.

**Step 3: Write minimal implementation**
- Increase selected-row emphasis.
- Add clearer styles for `running`, `starting`, `stopped`, and `error`.
- Keep structure unchanged.

**Step 4: Run test to verify it passes**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs && node --check crates/tunnelmux-gui/ui/app.js`

Expected: PASS

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/ui/app.js
git commit -m "feat: strengthen tunnel picker hierarchy"
```

### Task 4: Final GUI verification

**Files:**
- Verify only

**Step 1: Run GUI tests**
Run: `cargo test -p tunnelmux-gui`

Expected: PASS

**Step 2: Run helper and syntax verification**
Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs && node --check crates/tunnelmux-gui/ui/app.js`

Expected: PASS

**Step 3: Commit**
```bash
git status --short
```

Expected: clean working tree

