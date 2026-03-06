# GUI Native Bundle Release Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add native GUI installer assets to GitHub Releases while preserving the existing raw binary archive flow for `tunnelmuxd`, `tunnelmux-cli`, and manual GUI users.

**Architecture:** Keep the current archive packaging job and add a parallel GUI-bundle path. Tauri bundle metadata is finalized in `crates/tunnelmux-gui`; CI generates native bundle artifacts for `dmg`, `msi`, and `deb`; the publish job then releases both raw archives and GUI installers together.

**Tech Stack:** GitHub Actions, Tauri v2 bundling, Rust workspace crates, shell scripting for artifact collection if needed, existing repository release docs.

---

### Task 1: Finalize Tauri bundle metadata and icon wiring

**Files:**
- Modify: `crates/tunnelmux-gui/tauri.conf.json`
- Modify: `crates/tunnelmux-gui/README.md`
- Modify or Create: `crates/tunnelmux-gui/icons/*`

**Step 1: Write the failing validation check**

Run:

```bash
python - <<'PY'
import json
from pathlib import Path
config = json.loads(Path('crates/tunnelmux-gui/tauri.conf.json').read_text())
bundle = config.get('bundle', {})
assert bundle.get('icon'), 'bundle.icon must not be empty'
PY
```

Expected: FAIL because `bundle.icon` is currently empty.

**Step 2: Write the minimal implementation**

Update `crates/tunnelmux-gui/tauri.conf.json` so it includes:
- non-empty `bundle.icon` paths,
- stable native bundle metadata needed for installer generation,
- only the minimum additional fields needed for the first installer release.

If the current `crates/tunnelmux-gui/icons/icon.png` is not sufficient for all target bundles, add the smallest required derived icon assets under `crates/tunnelmux-gui/icons/`.

**Step 3: Run the validation again**

Run:

```bash
python - <<'PY'
import json
from pathlib import Path
config = json.loads(Path('crates/tunnelmux-gui/tauri.conf.json').read_text())
bundle = config['bundle']
assert bundle['icon'], 'bundle.icon must not be empty'
print(bundle['icon'])
PY
```

Expected: PASS and prints the configured icon list.

**Step 4: Run a GUI build sanity check**

Run: `cargo check -p tunnelmux-gui`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/tunnelmux-gui/tauri.conf.json crates/tunnelmux-gui/icons crates/tunnelmux-gui/README.md
git commit -m "build: add GUI bundle metadata and icon wiring"
```

### Task 2: Add deterministic GUI bundle artifact collection

**Files:**
- Create: `scripts/collect-gui-bundles.sh`
- Modify: `.github/workflows/release.yml`

**Step 1: Write the failing smoke check**

Run:

```bash
bash -n scripts/collect-gui-bundles.sh
```

Expected: FAIL because the helper script does not exist yet.

**Step 2: Write the minimal implementation**

Create `scripts/collect-gui-bundles.sh` that:
- accepts the bundle output directory and destination directory,
- collects only the expected installer extensions for the current job,
- fails clearly when no matching installer assets are found,
- prints the copied asset paths for CI logs.

Keep the script small and shell-portable.

**Step 3: Run syntax and fixture-based validation**

Run:

```bash
bash -n scripts/collect-gui-bundles.sh
TMP_SRC=$(mktemp -d)
TMP_DST=$(mktemp -d)
touch "$TMP_SRC/TunnelMux_0.1.3_x64.dmg"
bash scripts/collect-gui-bundles.sh "$TMP_SRC" "$TMP_DST" dmg
find "$TMP_DST" -type f | sort
```

Expected: PASS and one `.dmg` file appears in the destination directory.

**Step 4: Wire the helper into the workflow**

Update `.github/workflows/release.yml` so later GUI-bundle steps can call the helper instead of duplicating bundle-collection logic inline.

**Step 5: Commit**

```bash
git add scripts/collect-gui-bundles.sh .github/workflows/release.yml
git commit -m "build: add GUI bundle artifact collection helper"
```

### Task 3: Add a parallel GUI-bundle release job

**Files:**
- Modify: `.github/workflows/release.yml`

**Step 1: Write the failing workflow structure check**

Run:

```bash
ruby -e 'require "yaml"; data = YAML.load_file(".github/workflows/release.yml"); abort("missing gui_bundle") unless data.fetch("jobs").key?("gui_bundle")'
```

Expected: FAIL because the workflow does not yet define `gui_bundle`.

**Step 2: Write the minimal implementation**

Update `.github/workflows/release.yml` to add a `gui_bundle` job that:
- runs on macOS, Windows, and Linux,
- builds native GUI bundles for `dmg`, `msi`, and `deb`,
- installs Linux GUI dependencies as needed,
- uploads bundle outputs as artifacts separate from raw archive artifacts.

Implementation requirements:
- keep the current `build` job for raw archives,
- keep `publish` as the final release publisher,
- make bundle failure messages readable in CI logs,
- do not introduce signing or notarization secrets.

**Step 3: Update publish aggregation**

Modify `publish` so it downloads both artifact classes and publishes them together with a single `SHA256SUMS` file.

**Step 4: Run the structure check again**

Run:

```bash
ruby -e 'require "yaml"; data = YAML.load_file(".github/workflows/release.yml"); jobs = data.fetch("jobs"); abort("missing gui_bundle") unless jobs.key?("gui_bundle"); abort("publish missing gui_bundle need") unless Array(jobs.fetch("publish").fetch("needs")).include?("gui_bundle")'
```

Expected: PASS.

**Step 5: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add native GUI bundle release job"
```

### Task 4: Update release and user-facing documentation

**Files:**
- Modify: `docs/RELEASING.md`
- Modify: `README.md`
- Modify: `crates/tunnelmux-gui/README.md`

**Step 1: Write the failing documentation check**

Run:

```bash
rg -n "\.dmg|\.msi|\.deb|native GUI installer" docs/RELEASING.md README.md crates/tunnelmux-gui/README.md
```

Expected: FAIL or return incomplete matches because native installer distribution is not documented yet.

**Step 2: Write the minimal implementation**

Update documentation to explain:
- which native GUI installer assets are expected,
- that raw archives still exist,
- that first-release installers are unsigned,
- what platform dependencies/build expectations remain for local bundle builds.

**Step 3: Run the documentation check again**

Run:

```bash
rg -n "\.dmg|\.msi|\.deb|native GUI installer|unsigned" docs/RELEASING.md README.md crates/tunnelmux-gui/README.md
```

Expected: PASS with matches in all updated docs.

**Step 4: Re-read for consistency**

Manually verify that the docs do not promise signing, notarization, or auto-update behavior that is still out of scope.

**Step 5: Commit**

```bash
git add docs/RELEASING.md README.md crates/tunnelmux-gui/README.md
git commit -m "docs: document GUI installer assets"
```

### Task 5: Run final verification for the release-path change

**Files:**
- Modify if needed: any files touched above to fix validation issues

**Step 1: Run formatting and static checks**

Run: `cargo fmt`
Expected: PASS.

Run: `bash -n scripts/collect-gui-bundles.sh`
Expected: PASS.

Run: `python - <<'PY'
import json
from pathlib import Path
json.loads(Path('crates/tunnelmux-gui/tauri.conf.json').read_text())
print('tauri config ok')
PY`
Expected: PASS.

**Step 2: Run project verification**

Run: `cargo check -p tunnelmux-gui`
Expected: PASS.

Run: `cargo test --workspace --quiet`
Expected: PASS.

**Step 3: Run a host-platform bundle smoke check**

If `cargo tauri --help` already works, run:

```bash
cargo tauri build --bundles dmg -c crates/tunnelmux-gui/tauri.conf.json
```

Expected on macOS: PASS and a `.dmg` appears under the Tauri bundle output directory.

If `cargo tauri` is unavailable, install the CLI first:

```bash
cargo install tauri-cli --locked --version '^2.0.0'
```

Then rerun the same bundle command.

**Step 4: Verify git state**

Run: `git status --short`
Expected: clean working tree.

**Step 5: Commit any final fixes**

```bash
git add .
git commit -m "chore: finalize GUI native bundle release pipeline"
```

Only create this final fixup commit if verification forces a real follow-up change.
