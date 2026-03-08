import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('verify-easy-path script covers focused GUI readiness, ngrok start preflight, service-save recovery, startup recovery, pending-live handoff, settings reconnect, first-service, and recovery checks', () => {
  const script = readFileSync(new URL('./verify-easy-path.sh', import.meta.url), 'utf8');

  assert.match(script, /set -euo pipefail/);
  assert.match(script, /node --test scripts\/verify-easy-path\.test\.mjs/);
  assert.match(script, /GUI helper tests \(empty-state recovery, passive provider refresh, service-save recovery, startup recovery, pending-live handoff, save-flow momentum\)/);
  assert.match(script, /node --test crates\/tunnelmux-gui\/ui\/app\.test\.mjs/);
  assert.match(script, /node --check crates\/tunnelmux-gui\/ui\/app\.js/);
  assert.match(script, /cargo test -p tunnelmux-gui provider_availability/);
  assert.match(script, /cargo test -p tunnelmux-gui settings_save_reconnect/);
  assert.match(script, /cargo test -p tunnelmux-gui startup_reconnect_mode/);
  assert.match(script, /cargo test -p tunnelmux-gui probe_connection_reports/);
  assert.match(script, /cargo test -p tunnelmux-gui daemon_status_snapshot_from_connection/);
  assert.match(script, /cargo test -p tunnelmux-gui daemon_status_snapshot_reports_bootstrapping_state/);
  assert.match(script, /cargo test -p tunnelmux-gui missing_binary_clearly/);
  assert.match(
    script,
    /cargo test -p tunnelmux-gui commands::tests::start_tunnel_returns_friendly_error_when_provider_is_missing -- --exact/,
  );
  assert.match(
    script,
    /cargo test -p tunnelmux-gui commands::tests::start_tunnel_returns_friendly_error_when_ngrok_authtoken_is_missing -- --exact/,
  );
  assert.match(script, /cargo test -p tunnelmux-gui friendly_start_error_/);
  assert.match(script, /cargo test -p tunnelmux-gui friendly_route_save_error_/);
  assert.match(script, /cargo test -p tunnelmux-gui provider_status_summary/);
});
