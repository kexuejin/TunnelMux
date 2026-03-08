# Zero-Enabled Service Follow-Through Implementation Plan

**Goal:** Make the home hero and start-success messaging distinguish between `no services yet` and `saved but disabled services`.

**Architecture:** Keep the change in the GUI frontend helpers and action wiring. Reuse existing route counts from the current tunnel snapshot and existing services-panel highlight behavior.

**Tech Stack:** Tauri 2, vanilla JS, Node test runner

---

### Task 1: Lock the new guidance in tests

**Files:**
- Modify: `crates/tunnelmux-gui/ui/app.test.mjs`

**Steps:**
1. Add a failing helper test for dashboard guidance with `route_count > 0` and `enabled_services === 0`.
2. Add a failing helper test for `summarizeZeroServiceHeroAction()` returning `Review Services` when disabled services already exist.
3. Add a failing helper test for `summarizeStartSuccessAction()` returning `review_services` for an all-disabled service set.
4. Add a failing UI wiring assertion for the hero and status action handlers to reuse `highlightServicesPanel()`.
5. Run the focused JS test file and confirm the new assertions fail for the intended reason.

### Task 2: Implement the minimum UI change

**Files:**
- Modify: `crates/tunnelmux-gui/ui/tunnel-picker-helpers.mjs`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Steps:**
1. Update the helper inputs to consider total route count alongside enabled service count.
2. Return `Review Services` when services exist but all are disabled.
3. Reuse the same distinction in dashboard copy and start-success messages.
4. Wire hero and status `review_services` actions to the existing services highlight affordance.

### Task 3: Verify and update the local queue

**Files:**
- Modify: `.codex-local/tasks.md`

**Steps:**
1. Re-run the focused JS test file until it passes.
2. Run the adjacent verification command(s) needed to confirm no syntax regressions.
3. Mark the completed zero-enabled-service items in `.codex-local/tasks.md`.
