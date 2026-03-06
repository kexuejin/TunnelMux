# GUI Release Signing and Notarization Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add CI-ready macOS signing/notarization and Windows signing seams for GUI releases while preserving the current unsigned default path until credentials are provisioned.

**Architecture:** Keep `crates/tunnelmux-gui/tauri.conf.json` as the unsigned-safe base config. Add small helper scripts and workflow preflight so the release pipeline can switch between unsigned mode and signed mode per platform. Use Tauri’s documented macOS environment-variable path and Windows `bundle.windows.signCommand` integration instead of inventing a custom packaging flow.

**Tech Stack:** GitHub Actions, Tauri v2 bundling, shell scripting, `jq`, Rust workspace verification, Markdown maintainer docs.

---

### Task 1: Add Windows signing-config generation helper

**Files:**
- Create: `scripts/write-gui-windows-signing-config.sh`
- Test: `scripts/write-gui-windows-signing-config.sh`

**Step 1: Write the failing smoke check**

Run:

```bash
bash -n scripts/write-gui-windows-signing-config.sh
```

Expected: FAIL because the helper script does not exist yet.

**Step 2: Write the minimal implementation**

Create `scripts/write-gui-windows-signing-config.sh` that:

- accepts an output path,
- requires these environment variables:
  - `WINDOWS_TRUSTED_SIGNING_ENDPOINT`
  - `WINDOWS_TRUSTED_SIGNING_ACCOUNT`
  - `WINDOWS_TRUSTED_SIGNING_PROFILE`
- writes a minimal JSON config overlay containing:

```json
{
  "bundle": {
    "windows": {
      "signCommand": "trusted-signing-cli -e <endpoint> -a <account> -c <profile> -d TunnelMux %1"
    }
  }
}
```

- fails clearly if any required variable is missing.

Keep the script shell-portable and small.

**Step 3: Run syntax and fixture validation**

Run:

```bash
bash -n scripts/write-gui-windows-signing-config.sh
TMP_DIR=$(mktemp -d)
WINDOWS_TRUSTED_SIGNING_ENDPOINT=https://example.codesigning.azure.net \
WINDOWS_TRUSTED_SIGNING_ACCOUNT=TunnelMux \
WINDOWS_TRUSTED_SIGNING_PROFILE=Desktop \
  bash scripts/write-gui-windows-signing-config.sh "$TMP_DIR/windows-signing.json"
jq -r '.bundle.windows.signCommand' "$TMP_DIR/windows-signing.json"
```

Expected: PASS and the rendered command contains `trusted-signing-cli` and `%1`.

**Step 4: Commit**

```bash
git add scripts/write-gui-windows-signing-config.sh
git commit -m "build: add Windows signing config helper"
```

### Task 2: Extend the release workflow with signing-mode preflight

**Files:**
- Modify: `.github/workflows/release.yml`
- Test: `.github/workflows/release.yml`

**Step 1: Write the failing workflow structure check**

Run:

```bash
ruby -e 'require "yaml"; data = YAML.load_file(".github/workflows/release.yml"); text = File.read(".github/workflows/release.yml"); abort("missing macOS signing toggle") unless text.include?("GUI_MACOS_SIGNING_REQUIRED"); abort("missing Windows signing toggle") unless text.include?("GUI_WINDOWS_SIGNING_REQUIRED")'
```

Expected: FAIL because the workflow does not yet model signing modes.

**Step 2: Write the minimal implementation**

Update `.github/workflows/release.yml` so the `gui_bundle` job:

- keeps current unsigned bundle behavior by default,
- adds a preflight step that decides whether signing is required for the current matrix row,
- fails clearly if signing is required but required variables/secrets are missing,
- on macOS signed mode:
  - writes the App Store Connect API private key secret to a temporary file,
  - exports `APPLE_API_KEY_PATH`,
  - passes the documented Apple signing/notarization env vars to the Tauri step,
- on Windows signed mode:
  - installs `trusted-signing-cli`,
  - renders a temporary Tauri config overlay with `scripts/write-gui-windows-signing-config.sh`,
  - passes that config to the Tauri build step,
- leaves Linux `.deb` builds unchanged.

Suggested repository variables/secrets to reference:

- Variables:
  - `GUI_MACOS_SIGNING_REQUIRED`
  - `GUI_WINDOWS_SIGNING_REQUIRED`
  - `WINDOWS_TRUSTED_SIGNING_ENDPOINT`
  - `WINDOWS_TRUSTED_SIGNING_ACCOUNT`
  - `WINDOWS_TRUSTED_SIGNING_PROFILE`
- Secrets:
  - `APPLE_CERTIFICATE`
  - `APPLE_CERTIFICATE_PASSWORD`
  - `APPLE_SIGNING_IDENTITY`
  - `APPLE_API_ISSUER`
  - `APPLE_API_KEY`
  - `APPLE_API_PRIVATE_KEY`
  - `AZURE_CLIENT_ID`
  - `AZURE_CLIENT_SECRET`
  - `AZURE_TENANT_ID`

**Step 3: Re-run the workflow structure check**

Run:

```bash
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml"); text = File.read(".github/workflows/release.yml"); abort("missing macOS signing toggle") unless text.include?("GUI_MACOS_SIGNING_REQUIRED"); abort("missing Windows signing toggle") unless text.include?("GUI_WINDOWS_SIGNING_REQUIRED"); abort("missing Apple API key path setup") unless text.include?("APPLE_API_KEY_PATH"); abort("missing trusted-signing-cli") unless text.include?("trusted-signing-cli"); puts "workflow signing preflight ok"'
```

Expected: PASS.

**Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add GUI signing preflight"
```

### Task 3: Document signed-release setup for maintainers

**Files:**
- Modify: `docs/RELEASING.md`
- Modify: `crates/tunnelmux-gui/README.md`
- Optionally Modify: `README.md`

**Step 1: Write the failing documentation check**

Run:

```bash
rg -n 'GUI_MACOS_SIGNING_REQUIRED|GUI_WINDOWS_SIGNING_REQUIRED|APPLE_API_KEY_PATH|trusted-signing-cli' docs/RELEASING.md crates/tunnelmux-gui/README.md README.md
```

Expected: FAIL or incomplete matches because signing setup is not documented yet.

**Step 2: Write the minimal implementation**

Update docs to explain:

- unsigned mode remains the default until signing toggles are enabled,
- which GitHub variables and secrets are required for macOS and Windows signed builds,
- how the workflow derives `APPLE_API_KEY_PATH`,
- that Linux `.deb` remains unsigned in this iteration,
- that current public releases may still show unsigned trust prompts until maintainers enable signed mode.

Keep user-facing wording careful: do not promise that current releases are already signed.

**Step 3: Re-run the documentation check**

Run:

```bash
rg -n 'GUI_MACOS_SIGNING_REQUIRED|GUI_WINDOWS_SIGNING_REQUIRED|APPLE_API_KEY_PATH|trusted-signing-cli|unsigned mode' docs/RELEASING.md crates/tunnelmux-gui/README.md README.md
```

Expected: PASS with matches in the updated maintainer docs.

**Step 4: Commit**

```bash
git add docs/RELEASING.md crates/tunnelmux-gui/README.md README.md
git commit -m "docs: explain GUI release signing setup"
```

### Task 4: Run final verification for the CI-ready signing path

**Files:**
- Modify if needed: any files touched above to fix validation issues

**Step 1: Run static verification**

Run:

```bash
cargo fmt --all
bash -n scripts/collect-gui-bundles.sh
bash -n scripts/write-gui-windows-signing-config.sh
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml"); puts "workflow ok"'
```

Expected: PASS.

**Step 2: Run helper smoke validation**

Run:

```bash
TMP_DIR=$(mktemp -d)
WINDOWS_TRUSTED_SIGNING_ENDPOINT=https://example.codesigning.azure.net \
WINDOWS_TRUSTED_SIGNING_ACCOUNT=TunnelMux \
WINDOWS_TRUSTED_SIGNING_PROFILE=Desktop \
  bash scripts/write-gui-windows-signing-config.sh "$TMP_DIR/windows-signing.json"
jq -e '.bundle.windows.signCommand | contains("trusted-signing-cli")' "$TMP_DIR/windows-signing.json"
```

Expected: PASS.

**Step 3: Re-run the existing release-path checks**

Run:

```bash
python - <<'PY'
import json
from pathlib import Path
json.loads(Path('crates/tunnelmux-gui/tauri.conf.json').read_text())
print('tauri config ok')
PY
cargo check -p tunnelmux-gui
cargo test --workspace --quiet
```

Expected: PASS.

**Step 4: Commit any last fixes**

```bash
git add .
git commit -m "chore: finalize GUI signing release wiring"
```
