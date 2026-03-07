# Cloudflare Handoff UX Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add explicit Cloudflare dashboard/docs handoff actions so named tunnel users know where to manage hostname and Access.

**Architecture:** Keep provider state local and add UI-only actions in the existing single-page shell. No daemon changes are required.

**Tech Stack:** Tauri 2, vanilla HTML/CSS/JS

---

### Task 1: Add provider handoff controls

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/styles.css`

**Step 1: Write the failing test**
- Add a focused regression assertion where possible for named tunnel dashboard messaging and handoff action visibility.

**Step 2: Run test to verify it fails**
Run: `cargo test -p tunnelmux-gui`

Expected: coverage gap or failing assertion for missing named-tunnel handoff controls.

**Step 3: Write minimal implementation**
- Add Settings links for Cloudflare dashboard/docs.
- Add a dashboard action for running named tunnels without a public URL.
- Keep quick tunnel UX unchanged.

**Step 4: Run test to verify it passes**
Run: `cargo test -p tunnelmux-gui && node --check crates/tunnelmux-gui/ui/app.js`

Expected: GUI tests and syntax check pass.

**Step 5: Commit**
```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/styles.css docs/plans
git commit -m "feat: add cloudflare handoff actions"
```
