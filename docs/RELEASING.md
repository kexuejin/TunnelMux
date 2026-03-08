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
cd crates/tunnelmux-gui
cargo tauri build --bundles dmg -c tauri.conf.json
```

Adjust `--bundles` for your platform (`msi` on Windows, `deb` on Linux). On headless macOS shells, prefix `CI=true` to skip Finder prettification during DMG creation.

During local macOS smoke checks, `cargo tauri build` may also normalize `crates/tunnelmux-gui/Cargo.toml` by adding empty `features = []` on `tauri` and `tauri-build`. That change is tooling noise and should be reverted before committing.

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
