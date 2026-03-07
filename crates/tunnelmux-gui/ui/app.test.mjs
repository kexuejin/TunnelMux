import test from 'node:test';
import assert from 'node:assert/strict';

import { formatCurrentTunnelMeta, formatTunnelOptionLabel } from './tunnel-picker-helpers.mjs';

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
