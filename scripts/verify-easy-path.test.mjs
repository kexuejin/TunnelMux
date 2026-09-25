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
  assert.match(script, /node scripts\/gui-e2e\.mjs/);
  assert.match(script, /scripts\/verify-release-manifest\.test\.mjs/);
  assert.match(script, /cargo test -p tunnelmux-gui provider_availability/);
  assert.match(script, /cargo test -p tunnelmux-gui settings_save_reconnect/);
  assert.match(script, /cargo test -p tunnelmux-gui startup_reconnect_mode/);
  assert.match(script, /cargo test -p tunnelmux-gui probe_connection_reports/);
  assert.match(script, /cargo test -p tunnelmux-gui daemon_status_snapshot_from_connection/);
  assert.match(script, /cargo test -p tunnelmux-gui daemon_status_snapshot_reports_bootstrapping_state/);
  assert.match(script, /cargo test -p tunnelmux-gui daemon_manager_marks_/);
  assert.match(script, /cargo test -p tunnelmux-gui listen_addr_is_derived_/);
  assert.match(script, /cargo test -p tunnelmux-gui shutdown_leaves_an_adopted_daemon_alone/);
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

test('release and installer configuration stays fail-closed and embedded-daemon-only', () => {
  const ci = readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8');
  const release = readFileSync(new URL('../.github/workflows/release.yml', import.meta.url), 'utf8');
  const tauriConfig = JSON.parse(
    readFileSync(new URL('../crates/tunnelmux-gui/tauri.conf.json', import.meta.url), 'utf8'),
  );
  const installer = readFileSync(new URL('./install.sh', import.meta.url), 'utf8');

  assert.doesNotMatch(release, /externalBin|tauri\.gui\.daemon\.bundle/);
  for (const packageName of ['libappindicator3-dev', 'libxdo-dev', 'libdbus-1-dev', 'pkg-config']) {
    assert.match(ci, new RegExp(packageName.replace(/[.-]/g, '\\$&')));
    assert.ok(release.includes(packageName), `release workflow should install ${packageName}`);
  }
  assert.equal((release.match(/libxdo-dev/g) ?? []).length, 2);
  assert.match(release, /tunnelmux-updater\.exe/);
  assert.match(release, /tunnelmux-updater" "\$\{PKG_DIR\}\/"/);
  assert.match(release, /cp README\.md README\.zh-CN\.md LICENSE CHANGELOG\.md/);
  assert.match(release, /tag = 'v' \+ version/);
  assert.equal(tauriConfig.bundle?.externalBin, undefined);
  assert.match(installer, /refusing to install without checksum verification/);
  assert.match(installer, /exit 1/);
});
