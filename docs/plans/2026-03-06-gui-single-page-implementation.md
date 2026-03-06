# GUI Single-Page Easy-First Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Collapse the GUI into a single-page easy-first shell with a right-side service editor drawer, a right-top settings drawer, and hidden-by-default troubleshooting.

**Architecture:** Reuse the current Rust/Tauri command layer and service-centric terminology from the first usability pass, but replace the multi-workspace shell with one main page. Keep settings and troubleshooting available through secondary drawers/panels so the default view stays focused on public URL, tunnel controls, and the service list.

**Tech Stack:** Tauri v2, plain HTML/CSS/JS frontend, Rust GUI settings/command layer, Markdown docs.

---

### Task 1: Add the single-page design and implementation docs

**Files:**
- Create: `docs/plans/2026-03-06-gui-single-page-design.md`
- Create: `docs/plans/2026-03-06-gui-single-page-implementation.md`

**Step 1: Verify the docs do not already exist**

Run:

```bash
test ! -f docs/plans/2026-03-06-gui-single-page-design.md
test ! -f docs/plans/2026-03-06-gui-single-page-implementation.md
```

Expected: PASS before the files are created.

**Step 2: Save the approved design**

Write `docs/plans/2026-03-06-gui-single-page-design.md`.

**Step 3: Save this implementation plan**

Write `docs/plans/2026-03-06-gui-single-page-implementation.md`.

**Step 4: Commit**

```bash
git add docs/plans/2026-03-06-gui-single-page-design.md docs/plans/2026-03-06-gui-single-page-implementation.md
git commit -m "docs: add GUI single-page redesign plan"
```

### Task 2: Remove primary tabs and introduce the single main page shell

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing shell structure check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("tab navigation still present") if html.include?("data-workspace-tab"); abort("missing settings button") unless html.include?("Settings") || html.include?("gear")'
```

Expected: FAIL because the current GUI still has a tab-based shell.

**Step 2: Implement the single-page shell**

Change the shell so:

- the tab bar is removed,
- the main page always shows:
  - brand header,
  - tunnel summary/actions,
  - service list,
- the settings entry is moved to a right-top gear/button.

**Step 3: Re-run the shell structure check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("tab navigation still present") if html.include?("data-workspace-tab"); abort("missing settings button") unless html.include?("Settings") || html.include?("settings-button")'
```

Expected: PASS.

**Step 4: Commit**

```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/ui/app.js
git commit -m "feat: collapse GUI into a single-page shell"
```

### Task 3: Turn service add/edit into a drawer flow

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing drawer check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("service drawer missing") unless html.include?("service-drawer"); abort("settings still looks like page section") if html.include?("workspace-settings")'
```

Expected: FAIL on the current structure.

**Step 2: Implement service drawer behavior**

Update the GUI so:

- services remain visible on the main page,
- `Add Service` and `Edit` open a right-side drawer,
- drawer keeps the simplified service fields and advanced disclosure,
- closing the drawer returns immediately to the single-page list context.

**Step 3: Re-run the drawer check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("service drawer missing") unless html.include?("service-drawer")'
```

Expected: PASS.

**Step 4: Commit**

```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/ui/app.js
git commit -m "feat: add service drawer workflow"
```

### Task 4: Move settings behind a gear-triggered drawer

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `README.md`
- Modify: `crates/tunnelmux-gui/README.md`

**Step 1: Write the failing settings check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("settings drawer missing") unless html.include?("settings-drawer"); abort("main page still includes full settings section") if html.include?("Connection Settings") && html.include?("Tunnel Settings") && !html.include?("settings-drawer")'
```

Expected: FAIL on the current structure.

**Step 2: Implement settings drawer**

Move the existing connection/tunnel settings into a right-side drawer that opens from a top-right button.

Keep troubleshooting behind this settings surface as a secondary entry.

**Step 3: Update docs**

Update `README.md` and `crates/tunnelmux-gui/README.md` so the GUI is described as a single-page shell with a settings entry point rather than a multi-surface workflow.

**Step 4: Re-run the settings check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("settings drawer missing") unless html.include?("settings-drawer")'
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/ui/app.js README.md crates/tunnelmux-gui/README.md
git commit -m "feat: move GUI settings behind drawer"
```

### Task 5: Keep troubleshooting hidden until needed

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing troubleshooting check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("troubleshooting still primary") if html.include?("Troubleshooting") && html.include?("main page")'
```

Expected: FAIL or reveal that troubleshooting is still too prominent.

**Step 2: Implement hidden-by-default troubleshooting**

Ensure troubleshooting:

- is not part of the main page layout,
- is opened only from settings or error-state actions,
- stays out of the default visual hierarchy.

**Step 3: Re-run the troubleshooting check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("troubleshooting trigger missing") unless html.include?("View details") || html.include?("Troubleshooting")'
```

Expected: PASS with troubleshooting only reachable through a secondary path.

**Step 4: Commit**

```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/ui/app.js
git commit -m "feat: hide troubleshooting behind secondary actions"
```

### Task 6: Run final verification for the single-page redesign

**Files:**
- Modify if needed: any files touched above to fix verification issues

**Step 1: Run GUI-focused verification**

Run:

```bash
node --check crates/tunnelmux-gui/ui/app.js
cargo test -p tunnelmux-gui
cargo check -p tunnelmux-gui
```

Expected: PASS.

**Step 2: Run workspace verification**

Run:

```bash
cargo test --workspace --locked
```

Expected: PASS.

**Step 3: Run structure verification**

Run:

```bash
rg -n 'Public URL|Add Service|settings-drawer|service-drawer' crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/styles.css
```

Expected: PASS with matches in the single-page shell.

**Step 4: Manual smoke**

Run:

```bash
cargo run -p tunnelmux-gui
```

Verify manually:

- the GUI opens to one page,
- settings open from the top-right button,
- add/edit uses a drawer,
- the service list is always visible,
- the start action is obvious when nothing is running.

**Step 5: Commit final fixes**

```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui.styles.css crates/tunnelmux-gui/ui/app.js README.md crates/tunnelmux-gui/README.md
git commit -m "feat: deliver single-page GUI shell"
```
