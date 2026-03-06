# Hot Config Reload Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a separate declarative config file with automatic hot reload, while preserving current runtime-state persistence and existing manual reload behavior.

**Architecture:** `tunnelmuxd` keeps `state.json` as a daemon-owned runtime snapshot and introduces a separate `config.json` as operator-owned desired state. A lightweight polling loop detects config file changes and applies only declarative fields (`routes` and `health_check`) into memory, while diagnostics surfaces reload health and the CLI continues to use the existing `settings reload` and `diagnostics` commands.

**Tech Stack:** Rust 2024 workspace, `axum`, `tokio`, `clap`, `serde`, Markdown docs.

---

### Task 1: Add declarative config file loading primitives

**Files:**
- Modify: `crates/tunnelmuxd/src/persistence.rs`
- Modify: `crates/tunnelmuxd/src/main.rs`
- Test: `crates/tunnelmuxd/src/main.rs`

**Step 1: Write the failing tests**

Add tests in `crates/tunnelmuxd/src/main.rs` for:

```rust
#[tokio::test]
async fn load_config_file_reads_routes_and_health_check() {}

#[tokio::test]
async fn load_config_file_returns_none_when_missing() {}
```

**Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tunnelmuxd load_config_file_reads_routes_and_health_check -- --exact`
Expected: FAIL because no declarative config loader exists yet.

**Step 3: Write the minimal implementation**

Add in `crates/tunnelmuxd/src/persistence.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeclarativeConfigFile {
    routes: Vec<RouteRule>,
    health_check: Option<HealthCheckSettings>,
}

async fn load_config_file(path: &Path) -> anyhow::Result<Option<DeclarativeConfigFile>>
```

Also add `default_config_file()` and the new daemon args in `crates/tunnelmuxd/src/main.rs`:

```rust
#[arg(long)]
config_file: Option<PathBuf>,

#[arg(long, default_value_t = 1_000)]
config_reload_interval_ms: u64,
```

**Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tunnelmuxd load_config_file_reads_routes_and_health_check -- --exact`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmuxd/src/main.rs crates/tunnelmuxd/src/persistence.rs
git commit -m "feat: add declarative config file primitives"
```

### Task 2: Apply config file at startup and on manual reload

**Files:**
- Modify: `crates/tunnelmuxd/src/api.rs`
- Modify: `crates/tunnelmuxd/src/main.rs`
- Modify: `crates/tunnelmuxd/src/persistence.rs`
- Modify: `crates/tunnelmux-core/src/lib.rs`
- Test: `crates/tunnelmuxd/src/main.rs`

**Step 1: Write the failing tests**

Add tests for:

```rust
#[tokio::test]
async fn startup_prefers_config_file_over_persisted_routes() {}

#[tokio::test]
async fn settings_reload_endpoint_prefers_config_file_when_present() {}
```

**Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tunnelmuxd startup_prefers_config_file_over_persisted_routes -- --exact`
Expected: FAIL because startup/manual reload still only depend on `state.json`.

**Step 3: Write the minimal implementation**

- Add daemon-local config reload state tracking
- Load `config.json` during startup and overlay routes/health-check
- Update `reload_settings` to prefer `config.json` when available, while preserving the current state-file fallback

**Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tunnelmuxd startup_prefers_config_file_over_persisted_routes -- --exact`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-core/src/lib.rs crates/tunnelmuxd/src/main.rs crates/tunnelmuxd/src/api.rs crates/tunnelmuxd/src/persistence.rs
git commit -m "feat: prefer declarative config on startup and reload"
```

### Task 3: Add automatic config polling and diagnostics fields

**Files:**
- Modify: `crates/tunnelmux-core/src/lib.rs`
- Modify: `crates/tunnelmuxd/src/runtime.rs`
- Modify: `crates/tunnelmuxd/src/api.rs`
- Modify: `crates/tunnelmuxd/src/main.rs`
- Test: `crates/tunnelmuxd/src/main.rs`

**Step 1: Write the failing tests**

Add tests for:

```rust
#[tokio::test]
async fn config_poll_applies_changed_routes() {}

#[tokio::test]
async fn config_poll_keeps_last_good_config_on_parse_error() {}

#[tokio::test]
async fn diagnostics_endpoint_reports_config_reload_status() {}
```

**Step 2: Run the targeted tests to verify they fail**

Run: `cargo test -p tunnelmuxd config_poll_applies_changed_routes -- --exact`
Expected: FAIL because no config polling loop exists yet.

**Step 3: Write the minimal implementation**

- Add a polling helper that hashes file contents and reloads only when changed
- Start a `monitor_config_file(...)` task from daemon startup
- Extend `DiagnosticsResponse` with:

```rust
pub config_file: String,
pub config_reload_enabled: bool,
pub config_reload_interval_ms: u64,
pub last_config_reload_at: Option<String>,
pub last_config_reload_error: Option<String>,
```

**Step 4: Run the targeted tests to verify they pass**

Run: `cargo test -p tunnelmuxd config_poll_applies_changed_routes -- --exact`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-core/src/lib.rs crates/tunnelmuxd/src/main.rs crates/tunnelmuxd/src/runtime.rs crates/tunnelmuxd/src/api.rs
git commit -m "feat: add automatic config polling and diagnostics"
```

### Task 4: Update docs and verify the full workspace

**Files:**
- Modify: `README.md`
- Modify: `docs/ROADMAP.md`
- Modify: `docs/ARCHITECTURE.md`
- Test: workspace verification commands

**Step 1: Write the failing consistency checks**

Run: `rg -n 'hot configuration reload|operational audit and diagnostics' docs/ROADMAP.md`
Expected: unchecked items remain.

**Step 2: Write the minimal docs update**

- document `config.json` / `state.json` boundary
- document auto reload behavior in README or architecture docs
- mark roadmap items complete

**Step 3: Run full verification**

Run: `cargo fmt`
Expected: no formatting errors.

Run: `cargo test --workspace --quiet`
Expected: all tests pass.

**Step 4: Commit**

```bash
git add README.md docs/ROADMAP.md docs/ARCHITECTURE.md
git commit -m "docs: document declarative config hot reload"
```

## Suggested Execution Order

1. Task 1 — add config file primitives.
2. Task 2 — apply config at startup and manual reload.
3. Task 3 — add automatic polling and diagnostics.
4. Task 4 — update docs and run full verification.

## Verification Checklist

- `cargo fmt`
- `cargo test --workspace --quiet`
- smoke test startup with `--config-file`
- manually edit `config.json` and observe route reload via `tunnelmux diagnostics`
