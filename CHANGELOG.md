# Changelog

All notable changes to this project should be documented in this file.

## [Unreleased]

- Add `scripts/install.sh` for one-command install from GitHub Releases (macOS/Linux).
- Add CI shell syntax check for install script.
- Update README with quick install examples.

## [0.1.2] - 2026-03-05

- Sync `Cargo.lock` with workspace version bump so `--locked` release builds pass.
- Keep GitHub Actions release matrix compatible with current macOS Intel runner labels.

## [0.1.1] - 2026-03-05

- Add open-source repository baseline (governance docs and issue/PR templates).
- Add GitHub Actions CI and tag-based release packaging workflows.
- Add checksum (`SHA256SUMS`) generation for release assets.
- Fix macOS Intel release runner label (`macos-15-intel`).
- Expand release and installation documentation.

## [0.1.0] - 2026-03-05

- Initial public release of TunnelMux core/daemon/CLI.
- Tunnel lifecycle API (`start`, `stop`, `status`).
- Route management and gateway forwarding.
- Provider supervision and restart strategy.
- Third-party integration documentation.
