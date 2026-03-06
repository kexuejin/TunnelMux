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
3. Package binaries and upload assets to GitHub Release
4. Upload `SHA256SUMS` for integrity verification

Each platform archive now contains:
- `tunnelmuxd`
- `tunnelmux-cli`
- `tunnelmux-gui`
- `README.md`
- `LICENSE`
- `CHANGELOG.md`

Asset naming:

- `tunnelmux-<version>-x86_64-unknown-linux-gnu.tar.gz`
- `tunnelmux-<version>-x86_64-apple-darwin.tar.gz`
- `tunnelmux-<version>-aarch64-apple-darwin.tar.gz`
- `tunnelmux-<version>-x86_64-pc-windows-msvc.zip`
- `SHA256SUMS`

## GUI Build Notes

`crates/tunnelmux-gui` is currently shipped as a raw desktop binary inside the platform package.

Current automation does **not** build native GUI installers such as:
- `.app`
- `.dmg`
- `.msi`
- `.deb`

That keeps the release pipeline simple while the GUI MVP stabilizes.

## Linux GUI Build Dependencies

The Linux release job installs Tauri/WebKitGTK dependencies before compiling `tunnelmux-gui`.

Current CI package list:

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  librsvg2-dev \
  patchelf
```

If you build the GUI locally on Linux, install the equivalent packages first.

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
