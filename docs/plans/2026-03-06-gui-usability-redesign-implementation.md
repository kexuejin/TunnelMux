# GUI Usability Redesign Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Redesign the TunnelMux GUI so the default experience is easy-first, service-centric, and branded, with troubleshooting and advanced capability pushed behind secondary entry points.

**Architecture:** Keep the current Rust/Tauri command layer mostly intact for Phase 1, and focus implementation on frontend information architecture, terminology, and lightweight GUI settings expansion. Treat the existing backend route model as the source of truth, but translate it into a service-centric UI. Defer daemon lifecycle ownership to a later phase instead of mixing it into the first usability pass.

**Tech Stack:** Tauri v2, plain HTML/CSS/JS frontend, Rust GUI settings/command layer, Markdown docs.

---

### Task 1: Document the approved usability redesign

**Files:**
- Create: `docs/plans/2026-03-06-gui-usability-redesign-design.md`
- Create: `docs/plans/2026-03-06-gui-usability-redesign-implementation.md`

**Step 1: Verify the design docs do not already exist**

Run:

```bash
test ! -f docs/plans/2026-03-06-gui-usability-redesign-design.md
test ! -f docs/plans/2026-03-06-gui-usability-redesign-implementation.md
```

Expected: PASS before files are created.

**Step 2: Save the approved design**

Write `docs/plans/2026-03-06-gui-usability-redesign-design.md` with:

- goals/non-goals,
- approved navigation model (`Home / Services / Settings`),
- service-centric editor design,
- troubleshooting demotion,
- phased delivery plan.

**Step 3: Save this implementation plan**

Write `docs/plans/2026-03-06-gui-usability-redesign-implementation.md`.

**Step 4: Commit**

```bash
git add docs/plans/2026-03-06-gui-usability-redesign-design.md docs/plans/2026-03-06-gui-usability-redesign-implementation.md
git commit -m "docs: add GUI usability redesign plan"
```

### Task 2: Add visible in-app branding and simplify primary navigation

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Reuse: `crates/tunnelmux-gui/icons/icon.png`

**Step 1: Write the failing structure check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("missing Home") unless html.include?("Home"); abort("missing Services") unless html.include?("Services"); abort("missing Settings") unless html.include?("Settings")'
```

Expected: FAIL because the current UI still exposes `Operations / Routes / Diagnostics`.

**Step 2: Replace the top-level shell**

Update `crates/tunnelmux-gui/ui/index.html` and `crates/tunnelmux-gui/ui/styles.css` so the app shell:

- shows a visible brand/icon in the header,
- renames primary navigation to `Home`, `Services`, and `Settings`,
- removes diagnostics from primary navigation,
- restructures the hero area into a compact branded header.

**Step 3: Update navigation state handling**

Modify `crates/tunnelmux-gui/ui/app.js` so workspace switching understands:

- `home`,
- `services`,
- `settings`,
- no primary `diagnostics` workspace.

**Step 4: Re-run the structure check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("missing Home") unless html.include?("Home"); abort("missing Services") unless html.include?("Services"); abort("missing Settings") unless html.include?("Settings"); abort("still exposing Operations") if html.include?(">Operations<"); abort("still exposing Routes") if html.include?(">Routes<")'
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/ui/app.js
git commit -m "feat: simplify GUI navigation and branding"
```

### Task 3: Redesign Home into a result-first surface

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing copy check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("missing Public URL card") unless html.include?("Public URL"); abort("missing Manage Services") unless html.include?("Manage Services"); abort("legacy tunnel form still primary") if html.include?("Start Tunnel</h2>")'
```

Expected: FAIL on the current layout.

**Step 2: Implement the new Home layout**

Rework the Home screen so it contains only:

- a public URL card,
- a lightweight tunnel control card,
- a service summary card,
- a minimal disconnected / stopped state.

Do not show long connection forms or diagnostics panels on the default Home surface.

**Step 3: Keep provider selection lightweight**

Update JavaScript state/rendering so provider selection remains available on Home, but only as a small control attached to tunnel start/stop behavior.

**Step 4: Re-run the copy check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("missing Public URL card") unless html.include?("Public URL"); abort("missing Manage Services") unless html.include?("Manage Services"); abort("missing Tunnel control") unless html.include?("Tunnel Control")'
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/ui/app.js
git commit -m "feat: redesign GUI home for easy-first flow"
```

### Task 4: Convert route management into service-centric cards and editor

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Modify: `crates/tunnelmux-gui/ui/app.js`

**Step 1: Write the failing terminology check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("missing Services") unless html.include?("Services"); abort("legacy Create Route still present") if html.include?("Create Route"); abort("legacy Routes Workspace still present") if html.include?("Routes Workspace")'
```

Expected: FAIL on the current UI.

**Step 2: Rename the surface**

Change the page language from routes to services:

- `Routes Workspace` → `Services`,
- `Create Route` → `Add Service`,
- `Upstream URL` → `Local Service URL`,
- `Match Path Prefix` → `Public Path`.

**Step 3: Collapse advanced editing**

Update the editor UI so the default editing surface only shows:

- service name,
- local service URL,
- public path.

Add an `Advanced` disclosure for:

- exposure mode,
- health check,
- fallback URL.

Do not render raw route internals directly.

**Step 4: Preserve existing backend behavior**

Map the simplified UI back onto the existing route API in `app.js` without changing the server contract yet.

**Step 5: Re-run the terminology check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("missing Add Service") unless html.include?("Add Service"); abort("legacy Create Route still present") if html.include?("Create Route"); abort("legacy Routes Workspace still present") if html.include?("Routes Workspace")'
```

Expected: PASS.

**Step 6: Commit**

```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/ui/app.js
git commit -m "feat: make GUI services service-centric"
```

### Task 5: Split settings and demote diagnostics into troubleshooting

**Files:**
- Modify: `crates/tunnelmux-gui/src/settings.rs`
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/README.md`
- Modify: `README.md`

**Step 1: Write the failing settings/troubleshooting check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("missing Tunnel Settings") unless html.include?("Tunnel Settings"); abort("missing Connection Settings") unless html.include?("Connection Settings"); abort("diagnostics still top-level") if html.include?("data-workspace-tab=\"diagnostics\"")'
```

Expected: FAIL on the current layout.

**Step 2: Extend GUI settings**

Update `crates/tunnelmux-gui/src/settings.rs` and the corresponding command plumbing so GUI settings can persist at least:

- control base URL,
- bearer token,
- default provider,
- auto restart preference,
- optional `ngrok` auth/domain settings if surfaced in Phase 1.

Keep stored settings normalized and backward compatible with existing files where practical.

**Step 3: Rework the Settings page**

Update UI so `Settings` contains:

- connection settings,
- tunnel settings,
- a secondary troubleshooting entry.

Diagnostics should no longer be presented as a daily-use primary workspace.

**Step 4: Keep troubleshooting on-demand**

Move detailed diagnostics/log surfaces behind an explicit secondary action such as:

- `Troubleshooting`,
- `View details`,
- or a nested panel within Settings.

The goal is that most users never need to open it during normal operation.

**Step 5: Update docs**

Refresh `crates/tunnelmux-gui/README.md` and `README.md` to describe the new easy-first GUI model and the reduced emphasis on diagnostics.

**Step 6: Re-run the settings/troubleshooting check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("missing Tunnel Settings") unless html.include?("Tunnel Settings"); abort("missing Connection Settings") unless html.include?("Connection Settings"); abort("diagnostics still top-level") if html.include?("data-workspace-tab=\"diagnostics\""); puts "settings/troubleshooting structure ok"'
```

Expected: PASS.

**Step 7: Commit**

```bash
git add crates/tunnelmux-gui/src/settings.rs crates/tunnelmux-gui/src/commands.rs crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/README.md README.md
git commit -m "feat: simplify GUI settings and troubleshooting"
```

### Task 6: Run final verification for the usability redesign

**Files:**
- Modify if needed: any files touched above to fix validation issues

**Step 1: Run GUI-focused Rust verification**

Run:

```bash
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

**Step 3: Run text checks for the new information architecture**

Run:

```bash
rg -n 'Home|Services|Settings|Troubleshooting|Add Service|Public URL' crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/styles.css
```

Expected: PASS with matches in the redesigned GUI shell.

**Step 4: Manual smoke test**

Run:

```bash
cargo run -p tunnelmux-gui
```

Then verify manually:

- connected running state shows the public URL first,
- disconnected state offers retry/settings instead of a large form,
- adding a service uses the simplified editor,
- diagnostics are only reachable on demand,
- visible in-app branding/icon is present.

**Step 5: Commit final fixes**

```bash
git add crates/tunnelmux-gui/src/settings.rs crates/tunnelmux-gui/src/commands.rs crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/README.md README.md
git commit -m "feat: deliver GUI usability redesign"
```
