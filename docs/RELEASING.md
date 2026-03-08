# Releasing TunnelMux

## Versioning

- Use semantic version tags: `vX.Y.Z`
- Example: `v0.1.0`

## GitHub Release Packages (Automated)

This repository includes `.github/workflows/release.yml`.

When you push a tag like `v0.2.0`, GitHub Actions will:

1. Build `tunnelmuxd`, `tunnelmux-cli`, and `tunnelmux-gui`
2. Target platforms:
   - `x86_64-unknown-linux-gnu`
   - `x86_64-apple-darwin`
   - `aarch64-apple-darwin`
   - `x86_64-pc-windows-msvc`
3. Package raw binaries and upload assets to GitHub Release
4. Build native GUI installer assets for supported platforms
5. Upload `SHA256SUMS` for integrity verification

Each platform archive contains:
- `tunnelmuxd`
- `tunnelmux-cli`
- `tunnelmux-gui`
- `README.md`
- `LICENSE`
- `CHANGELOG.md`

Native GUI installer assets are also published:
- macOS: `.dmg`
- Windows: `.msi`
- Linux: `.deb`

Asset naming:

- `tunnelmux-<version>-x86_64-unknown-linux-gnu.tar.gz`
- `tunnelmux-<version>-x86_64-apple-darwin.tar.gz`
- `tunnelmux-<version>-aarch64-apple-darwin.tar.gz`
- `tunnelmux-<version>-x86_64-pc-windows-msvc.zip`
- `SHA256SUMS`

## GUI Build Notes

`crates/tunnelmux-gui` is shipped in two ways:
- as a raw desktop binary inside the platform archive package
- as a native GUI installer asset where supported

GUI installers also embed `tunnelmuxd` as a bundled external binary so the desktop app can auto-start a local daemon for installer users.

Current first-release native GUI installers include:
- `.dmg`
- `.msi`
- `.deb`

Unsigned mode remains the default release posture today. The repository now prepares an opt-in signed GUI release path for macOS and Windows, but public installers may still be unsigned until maintainers enable the signing toggles and provision credentials. Linux `.deb` remains unsigned in this iteration, and this work still does **not** include auto-update metadata.

## Linux GUI Build Dependencies

The Linux release job installs Tauri/WebKitGTK dependencies before compiling `tunnelmux-gui`.

Current CI package list:

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libappindicator3-dev \
  librsvg2-dev \
  patchelf \
  fakeroot
```

If you build the GUI locally on Linux, install the equivalent packages first.

## Local Native Bundle Smoke Check

On a machine with Tauri bundling prerequisites installed, you can build a local native installer with:

```bash
cargo build --release -p tunnelmuxd -p tunnelmux-cli -p tunnelmux-gui

cd crates/tunnelmux-gui
cargo tauri build --bundles dmg -c tauri.conf.json
```

Build the release binaries first if you want the local raw archive and GUI bundle validation to reflect the current workspace version. `cargo tauri build` only guarantees the GUI bundle path; it does not replace a stale local `target/release/tunnelmuxd` or `target/release/tunnelmux-cli`.

Adjust `--bundles` for your platform (`msi` on Windows, `deb` on Linux). On headless macOS shells, prefix `CI=true` to skip Finder prettification during DMG creation.

After a local GUI bundle build, verify the actual bundle files on disk instead of carrying forward a prior filename. The release workflow checks `target/<target>/release/bundle` first and then `target/release/bundle`, so use `scripts/collect-gui-bundles.sh` against whichever directory actually contains the host-platform bundle you just built.

During local macOS smoke checks, `cargo tauri build` may also normalize `crates/tunnelmux-gui/Cargo.toml` by adding empty `features = []` on `tauri` and `tauri-build`. That change is tooling noise and should be reverted before committing.

## Local Raw Archive Smoke Check

To mirror the release workflow's raw archive layout on the current host with the current workspace version and host target:

```bash
scripts/package-local-release-archive.sh /tmp/tunnelmux-local-release
```

Expected result:

- one raw archive named like `tunnelmux-<current-version>-<host-target>.tar.gz`
- one `SHA256SUMS`
- archive contents include `tunnelmuxd`, `tunnelmux-cli`, `tunnelmux-gui`, `README.md`, `README.zh-CN.md`, `LICENSE`, and `CHANGELOG.md`

## GUI Easy-Path Smoke Check

Run this after you have a host-platform GUI artifact that reflects the current workspace. Use `cloudflared` quick tunnels for the happy path because they do not require account setup.

### Prerequisites

- a fresh enough GUI state that you can still rehearse `Create Tunnel` without stale runtime noise;
- the current host-platform GUI artifact (`target/release/tunnelmux-gui` is fine for local rehearsal);
- a matching `tunnelmuxd` binary available next to the GUI binary or bundled in the app;
- one local test service on `http://127.0.0.1:3000`.

Example local test service:

```bash
python3 -m http.server 3000
```

Windows PowerShell:

```powershell
py -m http.server 3000
```

If `cloudflared` is already installed and you want a deterministic missing-provider pass, temporarily launch the GUI with a trimmed `PATH` so `cloudflared` is hidden while the colocated `tunnelmuxd` binary is still discoverable:

```bash
env PATH="/usr/bin:/bin:/usr/sbin:/sbin" target/release/tunnelmux-gui
```

Windows PowerShell:

```powershell
$env:Path = 'C:\Windows\System32;C:\Windows'
.\target\release\tunnelmux-gui.exe
```

Run the automated easy-path verifier first:

```bash
scripts/verify-easy-path.sh
```

### Checklist

1. **Managed daemon bundle and stalled-boot recovery**
   - Quit any already-running local `tunnelmuxd` and keep the default local daemon URL.
   - Temporarily rename or remove the bundled/colocated `tunnelmuxd` binary from the GUI artifact, then launch the app.
   - Expect the status area to explain that the local `tunnelmuxd` component is unavailable and to recommend reinstalling the app or putting `tunnelmuxd` on `PATH`, instead of surfacing a raw binary lookup error.
   - Restore the binary and relaunch the GUI.
   - Expect the status area to show `Starting local TunnelMux…` while the GUI-managed daemon is still booting.
   - If you deliberately block the default local daemon port before launch, expect startup to escalate to `Starting local TunnelMux is taking longer than expected. Retry the local daemon or check whether another app is already using this port.` with `Retry Local Daemon` instead of spinning forever.

2. **Missing-provider warning**
   - Open the app and create a tunnel with provider `cloudflared`.
   - Leave the Cloudflare tunnel token empty so the tunnel stays on the quick-tunnel path.
   - Click `Start Tunnel`.
   - Expect the `Provider Status` card to show `Cloudflared Missing` instead of a raw spawn error.
   - Expect the message to explain that TunnelMux could not find `cloudflared` in `PATH`.

3. **Install guidance copy**
   - Click `Copy Install Command` from the provider-status card.
   - Paste the clipboard contents into a scratch buffer.
   - Expect the copied command to match the host OS:
     - macOS: `brew install cloudflared`
     - Windows: `winget install --id Cloudflare.cloudflared`
     - Linux: command includes `apt-get install cloudflared`

4. **Provider recovery and successful start**
   - Install `cloudflared`, or restore it to `PATH`.
   - Click `Recheck Provider` from the provider warning state.
   - Click `Start Tunnel` again.
   - Expect status text `Tunnel started. Add Service to keep going.`
   - Expect `Public URL` to switch from `Not running` to a real `https://...trycloudflare.com` URL.
   - Expect the primary next step to stay `Add Service`.
   - Expect `Copy URL` and `Open` to stay hidden until at least one service is enabled.

5. **Add one service from the main flow**
   - Click `Add Service`.
   - Save a service with:
     - `Service Name`: `demo-web`
     - `Local Service URL`: `http://127.0.0.1:3000`
     - `Public Path`: `/`
   - Expect a `Service saved.` confirmation.
   - Expect one enabled service in the list and the dashboard count to update.

6. **Copy/share verification**
   - Click `Copy URL` and expect `Public URL copied.`
   - Paste the copied URL into a browser or scratch buffer.
   - Click `Open` and confirm the public URL resolves to the local test service.

7. **`ngrok` authtoken preflight**
   - Edit the current tunnel and switch the provider to `ngrok`.
   - Leave `ngrok Authtoken` empty.
   - Expect the home hint and `Provider Status` card to show `ngrok Authtoken Required` before any launch attempt.
   - Expect the dashboard `Start Tunnel` button to stay disabled until the token is added.
   - Expect the tunnel drawer to keep `Save` available, disable `Save and Start`, and expose the `ngrok Authtoken` field immediately.

8. **`ngrok`-only first-run escape hatch**
   - Launch the app with `ngrok` available but `cloudflared` hidden from `PATH`, and make sure there is no existing tunnel selected.
   - Expect the empty state to say `Create Tunnel` will preselect `ngrok`, but first start still needs an authtoken.
   - Expect a secondary `Copy cloudflared Install Command` button to stay visible in that same empty state.
   - Click it and expect the copied command to match the current OS-specific `cloudflared` install guidance.

### Pass Criteria

- Missing or stalled GUI-managed daemon startup stays on installer-aware or retryable recovery copy instead of surfacing a raw lookup error or spinning forever.
- Missing-provider guidance appears before raw provider launch failures.
- Missing `ngrok` authtoken guidance appears before any failed `ngrok` launch attempt.
- `ngrok`-only onboarding keeps the authtoken warning visible and preserves the `cloudflared` install escape hatch.
- Copied install guidance matches the current OS.
- The quick-tunnel path works after provider recovery without advanced configuration.
- A service can be added from the main screen.
- Share actions stay gated until a service is enabled.
- The public URL can be copied/opened and reaches the local test service.

## macOS First-launch Trust Prompts

Unsigned macOS GUI builds can still trigger Gatekeeper warnings on first launch.

Recommended sequence when the app source is trusted:

1. Right-click `TunnelMux.app`
2. Click `Open`
3. Confirm the prompt

If macOS still blocks the app:

1. Open `System Settings`
2. Go to `Privacy & Security`
3. Find the blocked app notice near the bottom
4. Click `Open Anyway`

Last resort, only for trusted downloads:

```bash
xattr -dr com.apple.quarantine /Applications/TunnelMux.app
```

## Signed GUI Release Mode

The GUI release workflow now supports an explicit signed mode for macOS and Windows while keeping unsigned mode as the default.

Repository variables:

- `GUI_MACOS_SIGNING_REQUIRED`
- `GUI_WINDOWS_SIGNING_REQUIRED`
- `WINDOWS_TRUSTED_SIGNING_ENDPOINT`
- `WINDOWS_TRUSTED_SIGNING_ACCOUNT`
- `WINDOWS_TRUSTED_SIGNING_PROFILE`

Repository secrets:

- `APPLE_CERTIFICATE`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_SIGNING_IDENTITY`
- `APPLE_API_ISSUER`
- `APPLE_API_KEY`
- `APPLE_API_PRIVATE_KEY`
- `AZURE_CLIENT_ID`
- `AZURE_CLIENT_SECRET`
- `AZURE_TENANT_ID`

macOS signed mode writes the App Store Connect private key secret to a temporary file and exports it as `APPLE_API_KEY_PATH` during the workflow run.

Windows signed mode renders a temporary Tauri config overlay that uses `trusted-signing-cli` through `bundle.windows.signCommand`.

If either signing toggle is left unset or set to anything other than `true`, the corresponding GUI bundle continues to build in unsigned mode.

## Manual Release Rehearsal

The same `release.yml` workflow also supports `workflow_dispatch` for non-publishing rehearsals.

Manual rehearsal inputs:

- `version` — required and must match the checked-out project version in `Cargo.toml` / `tauri.conf.json`
- `macos_signing_required` — optional override with `inherit`, `true`, or `false`
- `windows_signing_required` — optional override with `inherit`, `true`, or `false`

Rehearsal runs:

- build the same raw archives and GUI bundles as the tagged release path,
- apply the same signing preflight logic,
- generate `SHA256SUMS`,
- upload the merged output as the `rehearsal-dist` workflow artifact,
- do **not** publish a GitHub Release.

Use this path before cutting a real tag when you want to validate packaging, signing preflight, or artifact layout without touching the public release page.


## First-time GitHub publish

```bash
git remote add origin git@github.com:<your-org-or-user>/TunnelMux.git
git branch -M main
git push -u origin main
```

## Maintainer Release Steps

```bash
# 1) update version in workspace Cargo.toml (and lockfile if needed)
# 2) update CHANGELOG.md

git add .
git commit -m "chore: release v0.2.0"
git tag v0.2.0
git push origin main --tags
```

After workflow success, verify release assets in GitHub Releases page.

## Optional: Publish crates to crates.io

You can publish crates separately from binary releases. GitHub Release binaries do not depend on crates.io publishing.

Typical order:

1. `tunnelmux-core`
2. `tunnelmux-control-client`
3. `tunnelmux-cli`
4. `tunnelmuxd`

Notes:

- `tunnelmux-core` should be published first.
- Wait for crates.io index propagation before publishing dependent crates.
- If you only distribute binaries via GitHub Releases, this step can be skipped.
- `tunnelmux-gui` is currently primarily distributed through GitHub Release binaries rather than crates.io.

Use:

```bash
cargo publish -p tunnelmux-core
cargo publish -p tunnelmux-control-client
cargo publish -p tunnelmux-cli
cargo publish -p tunnelmuxd
```
