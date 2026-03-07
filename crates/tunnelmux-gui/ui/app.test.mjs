import test from 'node:test';
import assert from 'node:assert/strict';

import {
  formatCurrentTunnelMeta,
  formatCurrentTunnelUrl,
  formatTunnelOptionLabel,
  resolveDashboardStatus,
  shouldShowErrorDetailsAction,
  tunnelPickerRowClass,
} from './tunnel-picker-helpers.mjs';

test('formatCurrentTunnelMeta includes provider state and service counts', () => {
  const text = formatCurrentTunnelMeta({
    provider: 'cloudflared',
    state: 'running',
    route_count: 4,
    enabled_route_count: 3,
  });

  assert.equal(text, 'cloudflared • Running • 3/4 services live');
});

test('formatTunnelOptionLabel includes tunnel name state and service counts', () => {
  const text = formatTunnelOptionLabel({
    name: 'API Tunnel',
    state: 'stopped',
    route_count: 2,
    enabled_route_count: 1,
  });

  assert.equal(text, 'API Tunnel • Stopped • 1/2');
});

test('formatCurrentTunnelUrl prefers public url and falls back to provider-specific copy', () => {
  assert.equal(
    formatCurrentTunnelUrl({
      public_base_url: 'https://demo.trycloudflare.com',
    }),
    'https://demo.trycloudflare.com',
  );

  assert.equal(
    formatCurrentTunnelUrl({
      provider: 'cloudflared',
      state: 'running',
      public_base_url: null,
    }),
    'Managed in Cloudflare',
  );

  assert.equal(
    formatCurrentTunnelUrl({
      provider: 'ngrok',
      state: 'stopped',
      public_base_url: null,
    }),
    '',
  );
});

test('tunnelPickerRowClass distinguishes selected and runtime states', () => {
  assert.equal(tunnelPickerRowClass({ state: 'running' }, true), 'tunnel-picker-item selected running');
  assert.equal(tunnelPickerRowClass({ state: 'starting' }, false), 'tunnel-picker-item starting');
  assert.equal(tunnelPickerRowClass({ state: 'error' }, false), 'tunnel-picker-item error');
});

test('resolveDashboardStatus only surfaces daemon errors for passive refresh', () => {
  assert.deepEqual(
    resolveDashboardStatus({ connected: true, message: 'running fine' }),
    null,
  );

  assert.deepEqual(
    resolveDashboardStatus({ connected: false, message: 'connection refused' }),
    {
      message: 'Daemon unavailable: connection refused',
      isError: true,
    },
  );
});

test('shouldShowErrorDetailsAction only enables the action for error states', () => {
  assert.equal(shouldShowErrorDetailsAction({ isError: true }), true);
  assert.equal(shouldShowErrorDetailsAction({ isError: false }), false);
});
