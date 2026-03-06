# v0.1.4 Release Prep Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Prepare TunnelMux `v0.1.4` so the repository version metadata, release notes, and release workflow inputs are aligned for a new tag and release rehearsal.

**Architecture:** This is a release-preparation change, not a feature change. The work should keep runtime behavior unchanged while synchronizing every user-visible and build-critical version string, then verifying that workspace metadata and release packaging still resolve cleanly. The release notes should summarize work already merged to `main` since `v0.1.3`, especially the GUI packaging and release workflow improvements.

**Tech Stack:** Rust workspace (`cargo`), Tauri config JSON, Markdown docs, GitHub Actions release workflow.

---

### Task 1: Inventory version-controlled release metadata

**Files:**
- Inspect: `Cargo.toml`
- Inspect: `Cargo.lock`
- Inspect: `crates/tunnelmux-gui/tauri.conf.json`
- Inspect: `crates/tunnelmux-gui/src/commands.rs`
- Inspect: `README.md`
- Inspect: `docs/API.md`
- Inspect: `CHANGELOG.md`

**Step 1: Find current `0.1.3` references**

Run: `rg -n '0\\.1\\.3|v0\\.1\\.3|version\\s*=\\s*"0\\.1\\.3"' .`
Expected: workspace/package, docs, and test fixtures are identified.

**Step 2: Confirm latest release delta**

Run: `git log --oneline v0.1.3..main`
Expected: commits since `v0.1.3` are listed so release notes can be summarized accurately.

**Step 3: Commit**

```bash
git add docs/plans/2026-03-06-v0-1-4-release-prep.md
git commit -m "docs: add v0.1.4 release prep plan"
```

### Task 2: Bump build-critical version metadata to `0.1.4`

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/tunnelmux-gui/tauri.conf.json`
- Modify: `crates/tunnelmux-gui/src/commands.rs`

**Step 1: Update workspace version values**

Change `Cargo.toml` workspace and path dependency versions from `0.1.3` to `0.1.4`.

**Step 2: Update GUI bundle version**

Change `crates/tunnelmux-gui/tauri.conf.json` version from `0.1.3` to `0.1.4`.

**Step 3: Update version fixture used by GUI command tests**

Change the hard-coded `HealthResponse.version` test fixture in `crates/tunnelmux-gui/src/commands.rs` from `0.1.3` to `0.1.4`.

**Step 4: Refresh lockfile metadata**

Run: `cargo check --workspace --locked`
Expected: if `Cargo.lock` is stale, Cargo reports the mismatch.

**Step 5: Update `Cargo.lock` if needed**

Run: `cargo check --workspace`
Expected: lockfile/package metadata updates cleanly for `0.1.4`.

**Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/tunnelmux-gui/tauri.conf.json crates/tunnelmux-gui/src/commands.rs
git commit -m "chore: bump version to v0.1.4"
```

### Task 3: Refresh release-facing documentation

**Files:**
- Modify: `CHANGELOG.md`
- Modify: `README.md`
- Modify: `docs/API.md`

**Step 1: Promote unreleased notes into `0.1.4` entry**

Summarize the main shipped changes since `v0.1.3` in `CHANGELOG.md`, including GUI installers, signing/rehearsal workflow improvements, and diagnostics/config reload productization that already landed on `main`.

**Step 2: Update pinned installer example**

Change the example in `README.md` from `v0.1.3` to `v0.1.4`.

**Step 3: Update API version example**

Change the example response in `docs/API.md` from `0.1.3` to `0.1.4`.

**Step 4: Commit**

```bash
git add CHANGELOG.md README.md docs/API.md
git commit -m "docs: prepare v0.1.4 release notes"
```

### Task 4: Verify release prep end-to-end

**Files:**
- Verify: `Cargo.toml`
- Verify: `Cargo.lock`
- Verify: `CHANGELOG.md`
- Verify: `README.md`
- Verify: `docs/API.md`

**Step 1: Verify no stale `0.1.3` release refs remain**

Run: `rg -n '0\\.1\\.3|v0\\.1\\.3' Cargo.toml Cargo.lock crates/tunnelmux-gui README.md docs/API.md CHANGELOG.md`
Expected: no stale release-critical `0.1.3` refs remain, except historical changelog entries.

**Step 2: Verify Rust workspace still resolves**

Run: `cargo check --workspace`
Expected: success.

**Step 3: Verify release workflow can be rehearsed with new version**

Run: `gh workflow run release.yml --ref main -f version=0.1.4 -f macos_signing_required=inherit -f windows_signing_required=inherit`
Expected: a new manual rehearsal run is queued for the `0.1.4` version label.

**Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock crates/tunnelmux-gui/tauri.conf.json crates/tunnelmux-gui/src/commands.rs CHANGELOG.md README.md docs/API.md
git commit -m "chore: finalize v0.1.4 release prep"
```
