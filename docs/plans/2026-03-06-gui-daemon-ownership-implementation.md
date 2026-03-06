# GUI Daemon Ownership Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let the GUI automatically start a local `tunnelmuxd` when needed, prefer a bundled daemon binary, and safely distinguish GUI-managed daemon lifecycle from externally managed daemon lifecycle.

**Architecture:** Add a Rust-side daemon manager inside `crates/tunnelmux-gui` that owns binary resolution, startup, readiness polling, and shutdown rules. Keep the frontend product-focused by exposing only connection/ownership state through Tauri commands and startup wiring. Ship `tunnelmuxd` alongside the GUI bundle so installer users get the daemon automatically, while preserving a `PATH` fallback for development and power users.

**Tech Stack:** Rust/Tauri v2, Tokio process management, Tauri bundle config, GitHub Actions release packaging, Markdown docs.

---

### Task 1: Add the daemon ownership design and plan docs

**Files:**
- Create: `docs/plans/2026-03-06-gui-daemon-ownership-design.md`
- Create: `docs/plans/2026-03-06-gui-daemon-ownership-implementation.md`

**Step 1: Verify the docs do not already exist**

Run:

```bash
test ! -f docs/plans/2026-03-06-gui-daemon-ownership-design.md
test ! -f docs/plans/2026-03-06-gui-daemon-ownership-implementation.md
```

Expected: PASS before the files are created.

**Step 2: Save the approved design**

Write `docs/plans/2026-03-06-gui-daemon-ownership-design.md` with:

- ownership model (`external`, `managed`, `unavailable`),
- bundled-binary then `PATH` lookup order,
- startup/shutdown rules,
- packaging impact,
- user-facing behavior.

**Step 3: Save this implementation plan**

Write `docs/plans/2026-03-06-gui-daemon-ownership-implementation.md`.

**Step 4: Commit**

```bash
git add docs/plans/2026-03-06-gui-daemon-ownership-design.md docs/plans/2026-03-06-gui-daemon-ownership-implementation.md
git commit -m "docs: add GUI daemon ownership plan"
```

### Task 2: Introduce daemon manager state and ownership tests

**Files:**
- Create: `crates/tunnelmux-gui/src/daemon_manager.rs`
- Modify: `crates/tunnelmux-gui/src/state.rs`
- Modify: `crates/tunnelmux-gui/src/lib.rs`
- Test: `crates/tunnelmux-gui/src/daemon_manager.rs`

**Step 1: Write the failing daemon ownership tests**

Add tests in `crates/tunnelmux-gui/src/daemon_manager.rs` covering:

- external daemon detection does not request a spawn,
- bundled path is preferred over `PATH`,
- `PATH` fallback is used when bundled binary is missing,
- missing binary yields a clear error,
- ownership state marks GUI-spawned daemon as managed.

**Step 2: Run the focused test to verify RED**

Run:

```bash
cargo test -p tunnelmux-gui daemon_manager -- --nocapture
```

Expected: FAIL because the daemon manager module does not exist yet.

**Step 3: Add the minimal daemon manager types**

Create `crates/tunnelmux-gui/src/daemon_manager.rs` with:

- daemon ownership enum,
- daemon connection state struct,
- binary resolution helpers,
- spawn/readiness helper seams,
- tests using lightweight temp-path fixtures where possible.

Update `crates/tunnelmux-gui/src/state.rs` to hold runtime daemon manager state.

**Step 4: Re-run the focused daemon manager tests**

Run:

```bash
cargo test -p tunnelmux-gui daemon_manager -- --nocapture
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/src/daemon_manager.rs crates/tunnelmux-gui/src/state.rs crates/tunnelmux-gui/src/lib.rs
git commit -m "feat: add GUI daemon ownership state"
```

### Task 3: Add runtime daemon startup and shutdown behavior

**Files:**
- Modify: `crates/tunnelmux-gui/src/daemon_manager.rs`
- Modify: `crates/tunnelmux-gui/src/lib.rs`
- Modify: `crates/tunnelmux-gui/src/commands.rs`
- Test: `crates/tunnelmux-gui/src/daemon_manager.rs`

**Step 1: Write the failing lifecycle tests**

Add tests for:

- successful managed daemon startup after probe failure,
- startup timeout returns a product-safe error,
- GUI shutdown stops only managed daemon state,
- external daemon state is preserved on shutdown.

Use small local test helpers or injected process abstractions rather than real long-lived binaries where possible.

**Step 2: Run the focused lifecycle tests to verify RED**

Run:

```bash
cargo test -p tunnelmux-gui daemon_manager_lifecycle -- --nocapture
```

Expected: FAIL because lifecycle control does not exist yet.

**Step 3: Implement minimal lifecycle wiring**

Implement:

- `ensure_local_daemon` Rust function/command,
- readiness polling against the configured base URL,
- managed child handle storage in app state,
- app shutdown hook that stops only GUI-managed daemon.

Keep the frontend contract simple and product-oriented.

**Step 4: Re-run focused lifecycle tests**

Run:

```bash
cargo test -p tunnelmux-gui daemon_manager_lifecycle -- --nocapture
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/src/daemon_manager.rs crates/tunnelmux-gui/src/lib.rs crates/tunnelmux-gui/src/commands.rs
git commit -m "feat: let GUI manage local daemon startup"
```

### Task 4: Surface daemon ownership state in the GUI shell

**Files:**
- Modify: `crates/tunnelmux-gui/ui/index.html`
- Modify: `crates/tunnelmux-gui/ui/app.js`
- Modify: `crates/tunnelmux-gui/ui/styles.css`
- Modify: `crates/tunnelmux-gui/README.md`
- Modify: `README.md`

**Step 1: Write the failing copy/structure check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("missing local daemon startup copy") unless html.include?("local TunnelMux"); abort("missing Retry") unless html.include?("Retry")'
```

Expected: FAIL or incomplete matches because daemon ownership UX is not yet wired into the shell.

**Step 2: Implement product-oriented daemon states**

Update the frontend so `Home` can display:

- starting local daemon,
- connected to external daemon,
- connected to GUI-managed daemon,
- failed to start local daemon.

Do not expose raw PID/process control terminology in the normal UI.

**Step 3: Update docs**

Document that:

- the GUI now attempts to start a local daemon automatically,
- bundled daemon is preferred,
- GUI-managed daemon stops when the GUI exits,
- externally started daemon is never stopped by the GUI.

**Step 4: Re-run the structure check**

Run:

```bash
ruby -e 'html = File.read("crates/tunnelmux-gui/ui/index.html"); abort("missing local daemon startup copy") unless html.include?("local TunnelMux"); abort("missing Retry") unless html.include?("Retry")'
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/README.md README.md
git commit -m "feat: surface GUI-managed daemon state"
```

### Task 5: Ship `tunnelmuxd` with GUI bundles and dev fallback

**Files:**
- Modify: `crates/tunnelmux-gui/tauri.conf.json`
- Modify: `.github/workflows/release.yml`
- Modify: `scripts/collect-gui-bundles.sh`
- Modify: `docs/RELEASING.md`

**Step 1: Write the failing packaging structure check**

Run:

```bash
ruby -e 'text = File.read("crates/tunnelmux-gui/tauri.conf.json"); abort("missing bundled daemon config") unless text.include?("externalBin") || text.include?("resources")'
```

Expected: FAIL because GUI bundles do not yet include `tunnelmuxd`.

**Step 2: Add the minimal packaging implementation**

Update packaging so GUI bundles include the daemon binary in a Tauri-compatible way.

Ensure:

- development runs still work from source,
- release bundles include the daemon binary,
- artifact collection scripts remain correct.

**Step 3: Re-run the packaging structure check**

Run:

```bash
ruby -e 'text = File.read("crates/tunnelmux-gui/tauri.conf.json"); abort("missing bundled daemon config") unless text.include?("externalBin") || text.include?("resources"); puts "packaging config ok"'
```

Expected: PASS.

**Step 4: Commit**

```bash
git add crates/tunnelmux-gui/tauri.conf.json .github/workflows/release.yml scripts/collect-gui-bundles.sh docs/RELEASING.md
git commit -m "build: bundle daemon with GUI artifacts"
```

### Task 6: Run final verification for Phase 2A

**Files:**
- Modify if needed: any files touched above to fix verification issues

**Step 1: Run GUI-focused verification**

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

**Step 3: Run packaging/config verification**

Run:

```bash
ruby -e 'require "json"; JSON.parse(File.read("crates/tunnelmux-gui/tauri.conf.json")); puts "tauri config ok"'
bash -n scripts/collect-gui-bundles.sh
```

Expected: PASS.

**Step 4: Manual smoke scenarios**

Run:

```bash
cargo run -p tunnelmux-gui
```

Verify manually:

- no daemon running → GUI starts local daemon,
- external daemon already running → GUI connects without spawning another daemon,
- closing GUI stops only GUI-managed daemon,
- missing daemon binary shows a lightweight failure state.

**Step 5: Commit final fixes**

```bash
git add crates/tunnelmux-gui/src/daemon_manager.rs crates/tunnelmux-gui/src/state.rs crates/tunnelmux-gui/src/lib.rs crates/tunnelmux-gui/src/commands.rs crates/tunnelmux-gui/ui/index.html crates/tunnelmux-gui/ui/app.js crates/tunnelmux-gui/ui/styles.css crates/tunnelmux-gui/tauri.conf.json .github/workflows/release.yml scripts/collect-gui-bundles.sh README.md crates/tunnelmux-gui/README.md docs/RELEASING.md
git commit -m "feat: let GUI auto-start local daemon"
```
