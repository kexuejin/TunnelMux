# Releasing TunnelMux

## Versioning

- Use semantic version tags: `vX.Y.Z`
- Example: `v0.1.0`

## GitHub Release Packages (Automated)

This repository includes `.github/workflows/release.yml`.

When you push a tag like `v0.2.0`, GitHub Actions will:

1. Build `tunnelmuxd` and `tunnelmux-cli`
2. Target platforms:
   - `x86_64-unknown-linux-gnu`
   - `x86_64-apple-darwin`
   - `aarch64-apple-darwin`
   - `x86_64-pc-windows-msvc`
3. Package binaries and upload assets to GitHub Release
4. Upload `SHA256SUMS` for integrity verification

Asset naming:

- `tunnelmux-<version>-x86_64-unknown-linux-gnu.tar.gz`
- `tunnelmux-<version>-x86_64-apple-darwin.tar.gz`
- `tunnelmux-<version>-aarch64-apple-darwin.tar.gz`
- `tunnelmux-<version>-x86_64-pc-windows-msvc.zip`
- `SHA256SUMS`

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
2. `tunnelmux-cli`
3. `tunnelmuxd`

Notes:

- `tunnelmux-core` should be published first.
- Wait for crates.io index propagation before publishing dependent crates.
- If you only distribute binaries via GitHub Releases, this step can be skipped.

Use:

```bash
cargo publish -p tunnelmux-core
cargo publish -p tunnelmux-cli
cargo publish -p tunnelmuxd
```
