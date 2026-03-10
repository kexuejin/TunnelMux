# Diagnostics Daemon Unavailable Copy Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace raw diagnostics transport failures with daemon-unavailable copy while keeping the existing diagnostics requests.

**Architecture:** Add a shared frontend helper in `tunnel-picker-helpers.mjs` that converts daemon-unavailable request failures into section-specific copy. Update the three diagnostics refresh paths in `app.js` to use that helper, and lock the behavior with frontend tests in `app.test.mjs`.

**Tech Stack:** Plain browser JavaScript, Node test runner, Tauri invoke bridge

---

### Task 1: Add failing diagnostics error tests

**Files:**
- Modify: `crates/tunnelmux-gui/ui/app.test.mjs`
- Test: `crates/tunnelmux-gui/ui/app.test.mjs`

**Step 1: Write the failing test**

Add a helper unit test that expects daemon request failures to map to friendly diagnostics copy, plus a source-level test that expects `app.js` to call the helper in the diagnostics catch branches.

**Step 2: Run test to verify it fails**

Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`
Expected: FAIL because the diagnostics helper does not exist yet and `app.js` still writes raw transport errors.

### Task 2: Implement the shared diagnostics error summarizer

**Files:**
- Modify: `crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Test: `crates/tunnelmux-gui/ui/app.test.mjs`

**Step 1: Write minimal implementation**

Add `summarizeDiagnosticsLoadError(sectionLabel, error)` to `tunnel-picker-helpers.mjs`. Detect daemon-unavailable transport failures and return section-specific copy; otherwise keep the existing `Failed to load ...` string.

Update `app.js` to import the helper and use it for runtime summary, upstream health, and recent logs.

**Step 2: Run test to verify it passes**

Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs`
Expected: PASS

**Step 3: Run syntax verification**

Run: `node --check crates/tunnelmux-gui/ui/app.js`
Expected: PASS

### Task 3: Final verification

**Files:**
- Modify: `crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/app.test.mjs`

**Step 1: Re-run focused verification**

Run: `node --test crates/tunnelmux-gui/ui/app.test.mjs && node --check crates/tunnelmux-gui/ui/app.js`
Expected: PASS
