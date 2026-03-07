# Local Release Smoke Check Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Validate that the current TunnelMux workspace can produce a local native GUI release artifact and that the resulting binary/bundle is usable for manual verification.

**Architecture:** This plan does not add product behavior. It exercises the existing Tauri packaging path on the current machine, records artifact locations, and captures any build blockers needed for a later formal release.

**Tech Stack:** Rust workspace, Tauri 2, native desktop bundling

---

### Task 1: Verify release prerequisites locally

**Files:**
- Verify only

**Step 1: Check release-related docs and package config**

Run:
```bash
sed -n '1,220p' docs/RELEASING.md
sed -n '1,120p' crates/tunnelmux-gui/tauri.conf.json
```

Expected: local bundle guidance and icon/bundle config are present.

**Step 2: Check `cargo tauri` is available**

Run:
```bash
cargo tauri --help
```

Expected: command prints usage and exits `0`.

### Task 2: Build local native GUI release artifact

**Files:**
- Output only under `crates/tunnelmux-gui/src-tauri/target/release/bundle/` or Cargo/Tauri default release paths

**Step 1: Run local bundle build**

Run:
```bash
cd crates/tunnelmux-gui
cargo tauri build --bundles dmg -c tauri.conf.json
```

Expected:
- release binary builds
- local macOS `.app` and/or `.dmg` artifacts are emitted
- if bundling fails, capture the exact blocker

**Step 2: Record artifact paths**

Run:
```bash
find src-tauri target -type f \\( -name '*.app' -o -name '*.dmg' -o -name 'tunnelmux-gui' \\) 2>/dev/null
```

Expected: concrete local paths for manual validation.

### Task 3: Verify resulting binaries are runnable

**Files:**
- Verify only

**Step 1: Check binary presence**

Run:
```bash
ls -lah target/release/tunnelmux-gui target/release/tunnelmuxd target/release/tunnelmux-cli
```

Expected: all release binaries exist.

**Step 2: Check basic CLI help on release binaries**

Run:
```bash
target/release/tunnelmuxd --help
target/release/tunnelmux-cli --help
```

Expected: both print usage and exit `0`.

### Task 4: Capture release-prep follow-ups

**Files:**
- Optional update: `CHANGELOG.md`
- Optional update: `docs/RELEASING.md`

**Step 1: Summarize blockers or confirmations**

Record:
- whether bundle build succeeded
- exact artifact paths
- any local-only prerequisites discovered

**Step 2: Commit if docs changed**

```bash
git add docs/RELEASING.md CHANGELOG.md
git commit -m "docs: record local release smoke check notes"
```

