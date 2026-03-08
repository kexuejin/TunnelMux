#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "==> Verifier drift guard"
node --test scripts/verify-easy-path.test.mjs

echo "==> GUI helper tests (empty-state recovery, passive provider refresh, service-save recovery, startup recovery, pending-live handoff, save-flow momentum)"
node --test crates/tunnelmux-gui/ui/app.test.mjs

echo "==> GUI app syntax"
node --check crates/tunnelmux-gui/ui/app.js

echo "==> Provider availability Rust tests"
cargo test -p tunnelmux-gui provider_availability

echo "==> Settings save reconnect Rust tests"
cargo test -p tunnelmux-gui settings_save_reconnect
cargo test -p tunnelmux-gui startup_reconnect_mode
cargo test -p tunnelmux-gui probe_connection_reports
cargo test -p tunnelmux-gui daemon_status_snapshot_from_connection
cargo test -p tunnelmux-gui daemon_status_snapshot_reports_bootstrapping_state
cargo test -p tunnelmux-gui missing_binary_clearly

echo "==> Start preflight guards"
cargo test -p tunnelmux-gui commands::tests::start_tunnel_returns_friendly_error_when_provider_is_missing -- --exact
cargo test -p tunnelmux-gui commands::tests::start_tunnel_returns_friendly_error_when_ngrok_authtoken_is_missing -- --exact
cargo test -p tunnelmux-gui friendly_start_error_
cargo test -p tunnelmux-gui friendly_route_save_error_

echo "==> Provider recovery Rust tests"
cargo test -p tunnelmux-gui provider_status_summary
