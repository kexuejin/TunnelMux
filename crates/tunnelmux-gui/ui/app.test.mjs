import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as viewModels from './tunnel-picker-helpers.mjs';

import {
  applyProviderAvailabilitySnapshot,
  classifyRoutesPanel,
  formatCurrentTunnelMeta,
  formatCurrentTunnelUrl,
  formatHomeProviderHint,
  formatTunnelOptionLabel,
  getProviderInstallGuidance,
  resolveCreateTunnelDefaults,
  resolveDashboardPublicUrlActions,
  resolvePreferredCreateTunnelProvider,
  resolveServiceDrawerPrimaryField,
  shouldOpenTunnelAdvanced,
  shouldPassiveDrawerProviderRefresh,
  resolveDashboardStatus,
  resolveRouteFormTitle,
  summarizePassiveDrawerProviderRefresh,
  summarizeShareStatusAction,
  summarizeStartSuccessAction,
  summarizeStartFailureRecovery,
  summarizeZeroServiceHeroAction,
  shouldShowErrorDetailsAction,
  summarizeDaemonRecoveryAction,
  summarizeDaemonUnavailableMessage,
  summarizeDiagnosticsLoadError,
  summarizeDashboardGuidance,
  summarizeDrawerProviderReadiness,
  summarizeHomeTunnelActions,
  summarizeEmptyStateProviderGuidance,
  summarizeProviderAvailability,
  summarizeProviderRecheckFollowThrough,
  summarizeRouteSaveFailure,
  summarizeRouteSaveStatus,
  summarizeStatusMessage,
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

test('formatHomeProviderHint shows install guidance when the selected provider is missing', () => {
  assert.equal(
    formatHomeProviderHint({
      provider: 'ngrok',
      gateway_target_url: 'http://127.0.0.1:58080',
      auto_restart: true,
      provider_availability: {
        binary_name: 'ngrok',
        installed: false,
      },
    }),
    'Install ngrok to start this tunnel. TunnelMux could not find the ngrok command in your PATH.',
  );
});

test('summarizeProviderAvailability returns a current-tunnel warning when the provider is missing', () => {
  assert.deepEqual(
    summarizeProviderAvailability(
      {
        provider: 'cloudflared',
        provider_availability: {
          binary_name: 'cloudflared',
          installed: false,
        },
      },
      'macOS',
    ),
    {
      level: 'warning',
      title: 'Cloudflared Missing',
      message: 'Install cloudflared to start this tunnel. TunnelMux could not find the cloudflared command in your PATH.',
      action_kind: 'install_provider',
      action_label: 'Install cloudflared',
      action_payload: 'cloudflared',
      follow_up_action_kind: 'recheck_provider',
      follow_up_action_label: 'Recheck Provider',
      follow_up_action_payload: 'cloudflared',
    },
  );
});

test('summarizeProviderAvailability returns the same install-first action for missing ngrok', () => {
  assert.deepEqual(
    summarizeProviderAvailability(
      {
        provider: 'ngrok',
        provider_availability: {
          binary_name: 'ngrok',
          installed: false,
        },
      },
      'macOS',
    ),
    {
      level: 'warning',
      title: 'Ngrok Missing',
      message: 'Install ngrok to start this tunnel. TunnelMux could not find the ngrok command in your PATH.',
      action_kind: 'install_provider',
      action_label: 'Install ngrok',
      action_payload: 'ngrok',
      follow_up_action_kind: 'recheck_provider',
      follow_up_action_label: 'Recheck Provider',
      follow_up_action_payload: 'ngrok',
    },
  );
});

test('summarizeProviderAvailability returns ngrok authtoken guidance before start when ngrok is installed', () => {
  assert.deepEqual(
    summarizeProviderAvailability(
      {
        provider: 'ngrok',
        provider_availability: {
          binary_name: 'ngrok',
          installed: true,
        },
        ngrok_authtoken: null,
      },
      'macOS',
    ),
    {
      level: 'warning',
      title: 'ngrok Authtoken Required',
      message: 'Add the ngrok authtoken on this tunnel before starting it.',
      action_kind: 'edit_tunnel',
      action_label: 'Add ngrok Authtoken',
      action_payload: 'ngrok_authtoken',
      follow_up_action_kind: null,
      follow_up_action_label: null,
      follow_up_action_payload: null,
    },
  );
});

test('summarizeProviderAvailability offers Use Installed Provider when the other provider is already installed', () => {
  assert.deepEqual(
    summarizeProviderAvailability(
      {
        provider: 'cloudflared',
        provider_availability: {
          binary_name: 'cloudflared',
          installed: false,
        },
      },
      'macOS',
      {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: true,
        },
      },
    ),
    {
      level: 'warning',
      title: 'Cloudflared Missing',
      message: 'Install cloudflared to start this tunnel. TunnelMux could not find the cloudflared command in your PATH.',
      action_kind: 'install_provider',
      action_label: 'Install cloudflared',
      action_payload: 'cloudflared',
      follow_up_action_kind: 'use_installed_provider',
      follow_up_action_label: 'Use Installed Provider',
      follow_up_action_payload: 'ngrok',
    },
  );
});

test('summarizeHomeTunnelActions blocks hero start and exposes install plus recheck when the provider is missing', () => {
  assert.deepEqual(
    summarizeHomeTunnelActions(
      {
        provider: 'ngrok',
        state: 'stopped',
        provider_availability: {
          binary_name: 'ngrok',
          installed: false,
        },
      },
      'macOS',
    ),
    {
      start_disabled: true,
      start_label: 'Install Provider to Restart',
      action_kind: 'install_provider',
      action_label: 'Install ngrok',
      action_payload: 'ngrok',
      follow_up_action_kind: 'recheck_provider',
      follow_up_action_label: 'Recheck Provider',
      follow_up_action_payload: 'ngrok',
    },
  );
});

test('summarizeHomeTunnelActions surfaces Use Installed Provider when the alternate provider is ready', () => {
  assert.deepEqual(
    summarizeHomeTunnelActions(
      {
        provider: 'cloudflared',
        state: 'stopped',
        provider_availability: {
          binary_name: 'cloudflared',
          installed: false,
        },
      },
      'macOS',
      {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: true,
        },
      },
    ),
    {
      start_disabled: true,
      start_label: 'Install Provider to Restart',
      action_kind: 'install_provider',
      action_label: 'Install cloudflared',
      action_payload: 'cloudflared',
      follow_up_action_kind: 'use_installed_provider',
      follow_up_action_label: 'Use Installed Provider',
      follow_up_action_payload: 'ngrok',
    },
  );
});

test('summarizeHomeTunnelActions disables hero start and points ngrok recovery at the authtoken field', () => {
  assert.deepEqual(
    summarizeHomeTunnelActions(
      {
        provider: 'ngrok',
        state: 'stopped',
        provider_availability: {
          binary_name: 'ngrok',
          installed: true,
        },
        ngrok_authtoken: null,
      },
      'macOS',
    ),
    {
      start_disabled: true,
      start_label: 'Add Authtoken to Restart',
      action_kind: 'edit_tunnel',
      action_label: 'Add ngrok Authtoken',
      action_payload: 'ngrok_authtoken',
      follow_up_action_kind: null,
      follow_up_action_label: null,
      follow_up_action_payload: null,
    },
  );
});

test('applyProviderAvailabilitySnapshot lets the home copy recover after a manual recheck', () => {
  const tunnel = applyProviderAvailabilitySnapshot(
    {
      provider: 'ngrok',
      state: 'stopped',
      gateway_target_url: 'http://127.0.0.1:58080',
      auto_restart: true,
      ngrok_authtoken: 'token',
      provider_availability: {
        binary_name: 'ngrok',
        installed: false,
      },
    },
    {
      ngrok: {
        binary_name: 'ngrok',
        installed: true,
      },
    },
  );

  assert.equal(summarizeProviderAvailability(tunnel, 'macOS'), null);
  assert.deepEqual(
    summarizeHomeTunnelActions(tunnel, 'macOS'),
    {
      start_disabled: false,
      start_label: 'Restart Tunnel',
      action_kind: null,
      action_label: null,
      action_payload: null,
      follow_up_action_kind: null,
      follow_up_action_label: null,
      follow_up_action_payload: null,
    },
  );
  assert.equal(
    formatHomeProviderHint(tunnel),
    'ngrok targets http://127.0.0.1:58080 • auto restart enabled.',
  );
});

test('summarizeEmptyStateProviderGuidance recommends cloudflared install when no provider is ready', () => {
  assert.deepEqual(
    summarizeEmptyStateProviderGuidance(
      {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: false,
        },
      },
      'macOS',
    ),
    {
      message: 'Cloudflared is not installed yet. TunnelMux recommends cloudflared for the quickest first tunnel.',
      action_kind: 'install_provider',
      action_label: 'Install cloudflared',
      action_payload: 'cloudflared',
      follow_up_action_kind: 'recheck_provider',
      follow_up_action_label: 'Recheck Provider',
      follow_up_action_payload: 'cloudflared',
    },
  );
});

test('summarizeEmptyStateProviderGuidance keeps ngrok-only onboarding honest and exposes the cloudflared escape hatch', () => {
  assert.deepEqual(
    summarizeEmptyStateProviderGuidance(
      {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: true,
        },
      },
      'macOS',
    ),
    {
      message: 'Ngrok is installed. Create Tunnel will preselect it, but you will need an authtoken before Start Tunnel works.',
      action_kind: 'install_provider',
      action_label: 'Install cloudflared',
      action_payload: 'cloudflared',
      follow_up_action_kind: null,
      follow_up_action_label: null,
      follow_up_action_payload: null,
    },
  );
});

test('summarizeEmptyStateProviderGuidance keeps recovery actions hidden once providers are already ready', () => {
  assert.deepEqual(
    summarizeEmptyStateProviderGuidance(
      {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: true,
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: true,
        },
      },
      'macOS',
    ),
    {
      message: 'Cloudflared and ngrok are installed. Create Tunnel lets you choose the provider.',
      action_kind: null,
      action_label: null,
      action_payload: null,
      follow_up_action_kind: null,
      follow_up_action_label: null,
      follow_up_action_payload: null,
    },
  );
});


test('summarizeStartFailureRecovery preserves provider recovery for missing-provider start errors', () => {
  assert.deepEqual(
    summarizeStartFailureRecovery({
      message: 'Install cloudflared to start this tunnel. TunnelMux could not find the cloudflared command in your PATH.',
      settings: {
        base_url: 'http://127.0.0.1:4765',
      },
      tunnel: {
        provider: 'cloudflared',
      },
    }),
    {
      preservesProviderRecovery: true,
      recoveryTarget: null,
      statusAction: null,
    },
  );
});

test('summarizeStartFailureRecovery reuses daemon recovery actions for start failures', () => {
  assert.deepEqual(
    summarizeStartFailureRecovery({
      message: 'request failed: error sending request for url (http://127.0.0.1:4765/v1/tunnel/start)',
      settings: {
        base_url: 'http://127.0.0.1:4765',
      },
      tunnel: {
        provider: 'cloudflared',
      },
    }),
    {
      preservesProviderRecovery: false,
      recoveryTarget: null,
      statusAction: {
        kind: 'retry_local_daemon',
        label: 'Retry Local Daemon',
        payload: null,
      },
    },
  );

  assert.deepEqual(
    summarizeStartFailureRecovery({
      message: 'request failed: error sending request for url (http://127.0.0.1:9900/v1/tunnel/start)',
      settings: {
        base_url: 'http://127.0.0.1:9900',
      },
      tunnel: {
        provider: 'cloudflared',
      },
    }),
    {
      preservesProviderRecovery: false,
      recoveryTarget: null,
      statusAction: {
        kind: 'open_settings',
        label: 'Open Settings',
        payload: null,
      },
    },
  );
});

test('summarizeStartFailureRecovery routes daemon provider-path mismatches back to daemon recovery', () => {
  assert.deepEqual(
    summarizeStartFailureRecovery({
      message: 'TunnelMux found cloudflared, but the connected daemon could not use that binary path. Retry the local daemon or check whether another TunnelMux daemon is already using this port.',
      settings: {
        base_url: 'http://127.0.0.1:4765',
      },
      tunnel: {
        provider: 'cloudflared',
      },
    }),
    {
      preservesProviderRecovery: false,
      recoveryTarget: null,
      statusAction: {
        kind: 'retry_local_daemon',
        label: 'Retry Local Daemon',
        payload: null,
      },
    },
  );
});

test('summarizeStartFailureRecovery routes known config failures back to edit targets', () => {
  assert.deepEqual(
    summarizeStartFailureRecovery({
      message: 'Invalid Local Service URL. Review it on this tunnel, then retry.',
      settings: {
        base_url: 'http://127.0.0.1:4765',
      },
      tunnel: {
        provider: 'cloudflared',
      },
    }),
    {
      preservesProviderRecovery: false,
      recoveryTarget: 'gateway_target_url',
      statusAction: {
        kind: 'edit_tunnel',
        label: 'Review Local Service URL',
        payload: 'gateway_target_url',
      },
    },
  );

  assert.deepEqual(
    summarizeStartFailureRecovery({
      message: 'Add the ngrok authtoken on this tunnel, then retry.',
      settings: {
        base_url: 'http://127.0.0.1:4765',
      },
      tunnel: {
        provider: 'ngrok',
      },
    }),
    {
      preservesProviderRecovery: false,
      recoveryTarget: 'ngrok_authtoken',
      statusAction: {
        kind: 'edit_tunnel',
        label: 'Add ngrok Authtoken',
        payload: 'ngrok_authtoken',
      },
    },
  );

  assert.deepEqual(
    summarizeStartFailureRecovery({
      message: 'Review the reserved domain on this tunnel. Use only the hostname, like demo.ngrok.app, without https://, paths, query strings, or ports.',
      settings: {
        base_url: 'http://127.0.0.1:4765',
      },
      tunnel: {
        provider: 'ngrok',
      },
    }),
    {
      preservesProviderRecovery: false,
      recoveryTarget: 'ngrok_domain',
      statusAction: {
        kind: 'edit_tunnel',
        label: 'Review ngrok Domain',
        payload: 'ngrok_domain',
      },
    },
  );
});

test('summarizeStartFailureRecovery routes named-cloudflared token and auth failures back to the tunnel drawer', () => {
  const expected = {
    preservesProviderRecovery: false,
    recoveryTarget: 'cloudflared_tunnel_token',
    statusAction: {
      kind: 'edit_tunnel',
      label: 'Review Cloudflare Tunnel Token',
      payload: 'cloudflared_tunnel_token',
    },
  };

  assert.deepEqual(
    summarizeStartFailureRecovery({
      message: 'Review the Cloudflare Tunnel Token on this tunnel, then retry.',
      settings: {
        base_url: 'http://127.0.0.1:4765',
      },
      tunnel: {
        provider: 'cloudflared',
        cloudflared_tunnel_token: 'cf-token',
      },
    }),
    expected,
  );

  assert.deepEqual(
    summarizeStartFailureRecovery({
      message: 'Unauthorized: Provided Tunnel token is not valid.',
      settings: {
        base_url: 'http://127.0.0.1:4765',
      },
      tunnel: {
        provider: 'cloudflared',
        cloudflared_tunnel_token: 'cf-token',
      },
    }),
    expected,
  );
});

test('summarizeProviderRecheckFollowThrough returns Start Tunnel for home recovery when starting is next', () => {
  assert.deepEqual(
    summarizeProviderRecheckFollowThrough({
      source: 'home',
      tunnel_state: 'stopped',
    }),
    {
      kind: 'start_tunnel',
      label: 'Start Tunnel',
    },
  );
});

test('summarizeProviderRecheckFollowThrough returns Save and Start for drawer recovery when starting is next', () => {
  assert.deepEqual(
    summarizeProviderRecheckFollowThrough({
      source: 'drawer',
      tunnel_state: 'idle',
    }),
    {
      kind: 'save_and_start_tunnel',
      label: 'Save and Start',
    },
  );
});

test('summarizeProviderRecheckFollowThrough returns Create Tunnel for empty-state recovery when creating is next', () => {
  assert.deepEqual(
    summarizeProviderRecheckFollowThrough({
      source: 'empty',
      tunnel_state: 'offline',
    }),
    {
      kind: 'create_tunnel',
      label: 'Create Tunnel',
    },
  );
});

test('summarizeProviderRecheckFollowThrough stays text-only for already-running tunnels', () => {
  assert.equal(
    summarizeProviderRecheckFollowThrough({
      source: 'home',
      tunnel_state: 'running',
    }),
    null,
  );
});

test('resolvePreferredCreateTunnelProvider prefers the sole installed provider and otherwise falls back to cloudflared', () => {
  assert.equal(
    resolvePreferredCreateTunnelProvider({
      cloudflared: {
        binary_name: 'cloudflared',
        installed: false,
      },
      ngrok: {
        binary_name: 'ngrok',
        installed: true,
      },
    }),
    'ngrok',
  );

  assert.equal(
    resolvePreferredCreateTunnelProvider({
      cloudflared: {
        binary_name: 'cloudflared',
        installed: true,
      },
      ngrok: {
        binary_name: 'ngrok',
        installed: true,
      },
    }),
    'cloudflared',
  );

  assert.equal(
    resolvePreferredCreateTunnelProvider({
      cloudflared: {
        binary_name: 'cloudflared',
        installed: false,
      },
      ngrok: {
        binary_name: 'ngrok',
        installed: false,
      },
    }),
    'cloudflared',
  );
});

test('resolveCreateTunnelDefaults reuses the easy-path defaults and preferred provider', () => {
  assert.deepEqual(
    resolveCreateTunnelDefaults({
      cloudflared: {
        binary_name: 'cloudflared',
        installed: false,
      },
      ngrok: {
        binary_name: 'ngrok',
        installed: true,
      },
    }),
    {
      name: 'Main Tunnel',
      provider: 'ngrok',
      gateway_target_url: 'http://127.0.0.1:48080',
      auto_restart: true,
      cloudflared_tunnel_token: null,
      ngrok_authtoken: null,
      ngrok_domain: null,
    },
  );
});

test('summarizeDrawerProviderReadiness disables save and start when the selected provider is missing', () => {
  assert.deepEqual(
    summarizeDrawerProviderReadiness(
      'cloudflared',
      {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
      },
      'macOS',
    ),
    {
      level: 'warning',
      title: 'Cloudflared Missing',
      message: 'Install cloudflared before using Save and Start. TunnelMux could not find the cloudflared command in your PATH. Cloudflared is still the recommended quick-tunnel path for the common case, and quick tunnels work without a Cloudflare Tunnel Token.',
      action_kind: 'install_provider',
      action_label: 'Install cloudflared',
      action_payload: 'cloudflared',
      follow_up_action_kind: 'recheck_provider',
      follow_up_action_label: 'Recheck Provider',
      follow_up_action_payload: 'cloudflared',
      start_disabled: true,
      start_label: 'Install Provider to Start',
    },
  );
});

test('summarizeDrawerProviderReadiness keeps install guidance visible and offers Use Installed Provider when the alternate provider is ready', () => {
  assert.deepEqual(
    summarizeDrawerProviderReadiness(
      'cloudflared',
      {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: true,
        },
      },
      'macOS',
    ),
    {
      level: 'warning',
      title: 'Cloudflared Missing',
      message: 'Install cloudflared before using Save and Start. TunnelMux could not find the cloudflared command in your PATH. Cloudflared is still the recommended quick-tunnel path for the common case, and quick tunnels work without a Cloudflare Tunnel Token.',
      action_kind: 'install_provider',
      action_label: 'Install cloudflared',
      action_payload: 'cloudflared',
      follow_up_action_kind: 'use_installed_provider',
      follow_up_action_label: 'Use Installed Provider',
      follow_up_action_payload: 'ngrok',
      start_disabled: true,
      start_label: 'Install Provider to Start',
    },
  );
});

test('summarizeDrawerProviderReadiness gives missing ngrok the same install-first action', () => {
  assert.deepEqual(
    summarizeDrawerProviderReadiness(
      'ngrok',
      {
        ngrok: {
          binary_name: 'ngrok',
          installed: false,
        },
      },
      'macOS',
    ),
    {
      level: 'warning',
      title: 'Ngrok Missing',
      message: 'Install ngrok before using Save and Start. TunnelMux could not find the ngrok command in your PATH.',
      action_kind: 'install_provider',
      action_label: 'Install ngrok',
      action_payload: 'ngrok',
      follow_up_action_kind: 'recheck_provider',
      follow_up_action_label: 'Recheck Provider',
      follow_up_action_payload: 'ngrok',
      start_disabled: true,
      start_label: 'Install Provider to Start',
    },
  );
});

test('summarizeDrawerProviderReadiness disables save and start when ngrok authtoken is missing', () => {
  assert.deepEqual(
    summarizeDrawerProviderReadiness(
      {
        provider: 'ngrok',
        ngrok_authtoken: null,
      },
      {
        ngrok: {
          binary_name: 'ngrok',
          installed: true,
        },
      },
      'macOS',
    ),
    {
      level: 'warning',
      title: 'ngrok Authtoken Required',
      message: 'Add the ngrok authtoken on this tunnel before using Save and Start.',
      action_kind: null,
      action_label: null,
      action_payload: null,
      follow_up_action_kind: null,
      follow_up_action_label: null,
      follow_up_action_payload: null,
      start_disabled: true,
      start_label: 'Add Authtoken to Start',
    },
  );
});

test('summarizeDrawerProviderReadiness keeps save and start available when the selected provider is installed', () => {
  assert.deepEqual(
    summarizeDrawerProviderReadiness(
      'ngrok',
      {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: true,
        },
      },
      'Windows NT',
    ),
    {
      level: 'info',
      title: 'Ngrok Ready',
      message: 'ngrok is installed. Save and Start will launch this tunnel after saving.',
      action_kind: null,
      action_label: null,
      action_payload: null,
      follow_up_action_kind: null,
      follow_up_action_label: null,
      follow_up_action_payload: null,
      start_disabled: false,
      start_label: 'Save and Start',
    },
  );
});

test('summarizeDrawerProviderReadiness explains when a provider was installed for TunnelMux locally', () => {
  assert.deepEqual(
    summarizeDrawerProviderReadiness(
      'cloudflared',
      {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: true,
          source: 'local_tools',
          resolved_path: '/Users/demo/Library/Application Support/com.tunnelmux.gui/tools/bin/cloudflared',
        },
      },
      'macOS',
    ),
    {
      level: 'info',
      title: 'Cloudflared Ready',
      message: 'cloudflared is installed for TunnelMux. Save and Start will launch this tunnel after saving.',
      action_kind: null,
      action_label: null,
      action_payload: null,
      follow_up_action_kind: null,
      follow_up_action_label: null,
      follow_up_action_payload: null,
      start_disabled: false,
      start_label: 'Save and Start',
    },
  );
});

test('summarizeDrawerProviderReadiness shows installing state while TunnelMux is downloading a provider', () => {
  assert.deepEqual(
    summarizeDrawerProviderReadiness(
      'cloudflared',
      {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
          source: 'missing',
          resolved_path: null,
          install_state: 'downloading',
          install_error: null,
          install_version: '2026.2.0',
        },
      },
      'macOS',
    ),
    {
      level: 'info',
      title: 'Cloudflared Installing',
      message: 'Installing cloudflared for TunnelMux. Save and Start will unlock when the download finishes.',
      action_kind: null,
      action_label: null,
      action_payload: null,
      follow_up_action_kind: null,
      follow_up_action_label: null,
      follow_up_action_payload: null,
      start_disabled: true,
      start_label: 'Installing…',
    },
  );
});

test('summarizeDrawerProviderReadiness offers retry install when a local install failed', () => {
  assert.deepEqual(
    summarizeDrawerProviderReadiness(
      'cloudflared',
      {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
          source: 'missing',
          resolved_path: null,
          install_state: 'failed',
          install_error: 'download exploded',
          install_version: '2026.2.0',
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: true,
          source: 'system_path',
          resolved_path: '/opt/homebrew/bin/ngrok',
          install_state: null,
          install_error: null,
          install_version: null,
        },
      },
      'macOS',
    ),
    {
      level: 'warning',
      title: 'Cloudflared Install Failed',
      message: 'TunnelMux could not finish installing cloudflared: download exploded',
      action_kind: 'install_provider',
      action_label: 'Retry Install',
      action_payload: 'cloudflared',
      follow_up_action_kind: 'use_installed_provider',
      follow_up_action_label: 'Use Installed Provider',
      follow_up_action_payload: 'ngrok',
      start_disabled: true,
      start_label: 'Retry Install to Start',
    },
  );
});

test('tunnel drawer keeps provider-change readiness wiring and adds a recheck action', () => {
  const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(html, /id="tunnel-provider-recheck-action"/);
  assert.match(appJs, /elements\.tunnelProvider\?\.addEventListener\('change', syncTunnelProviderFields\);/);
  assert.match(appJs, /elements\.tunnelProviderRecheckAction\?\.addEventListener\('click', \(\) => void withBusy\(recheckTunnelDrawerProvider\)\);/);
  assert.match(appJs, /async function recheckTunnelDrawerProvider\(\) \{[\s\S]*await runProviderUiAction\([\s\S]*readiness\.follow_up_action_kind,[\s\S]*readiness\.follow_up_action_payload,[\s\S]*'drawer',[\s\S]*\);[\s\S]*\}/);
});

test('shouldPassiveDrawerProviderRefresh only runs when the open drawer is blocked on a missing provider', () => {
  assert.equal(
    shouldPassiveDrawerProviderRefresh({
      tunnelDrawerOpen: true,
      busy: false,
      visibilityState: 'visible',
      provider: 'cloudflared',
      providerAvailabilitySnapshot: {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
      },
    }),
    true,
  );

  assert.equal(
    shouldPassiveDrawerProviderRefresh({
      tunnelDrawerOpen: false,
      busy: false,
      visibilityState: 'visible',
      provider: 'cloudflared',
      providerAvailabilitySnapshot: {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
      },
    }),
    false,
  );

  assert.equal(
    shouldPassiveDrawerProviderRefresh({
      tunnelDrawerOpen: true,
      busy: false,
      visibilityState: 'visible',
      provider: 'ngrok',
      providerAvailabilitySnapshot: {
        ngrok: {
          binary_name: 'ngrok',
          installed: true,
        },
      },
    }),
    false,
  );
});

test('shouldPassiveEmptyStateProviderRefresh only runs when no-tunnel install guidance is visible', () => {
  assert.equal(
    viewModels.shouldPassiveEmptyStateProviderRefresh?.({
      hasCurrentTunnel: false,
      tunnelDrawerOpen: false,
      busy: false,
      visibilityState: 'visible',
      providerAvailabilitySnapshot: {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: false,
        },
      },
    }),
    true,
  );

  assert.equal(
    viewModels.shouldPassiveEmptyStateProviderRefresh?.({
      hasCurrentTunnel: true,
      tunnelDrawerOpen: false,
      busy: false,
      visibilityState: 'visible',
      providerAvailabilitySnapshot: {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: false,
        },
      },
    }),
    false,
  );

  assert.equal(
    viewModels.shouldPassiveEmptyStateProviderRefresh?.({
      hasCurrentTunnel: false,
      tunnelDrawerOpen: true,
      busy: false,
      visibilityState: 'visible',
      providerAvailabilitySnapshot: {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: false,
        },
      },
    }),
    false,
  );

  assert.equal(
    viewModels.shouldPassiveEmptyStateProviderRefresh?.({
      hasCurrentTunnel: false,
      tunnelDrawerOpen: false,
      busy: false,
      visibilityState: 'visible',
      providerAvailabilitySnapshot: {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: true,
        },
      },
    }),
    false,
  );
});

test('shouldPassiveCurrentTunnelProviderRefresh only runs when the selected tunnel is blocked on a missing provider', () => {
  assert.equal(
    viewModels.shouldPassiveCurrentTunnelProviderRefresh?.({
      currentTunnel: {
        provider: 'ngrok',
        state: 'stopped',
        provider_availability: {
          binary_name: 'ngrok',
          installed: false,
        },
      },
      tunnelDrawerOpen: false,
      busy: false,
      visibilityState: 'visible',
    }),
    true,
  );

  assert.equal(
    viewModels.shouldPassiveCurrentTunnelProviderRefresh?.({
      currentTunnel: null,
      tunnelDrawerOpen: false,
      busy: false,
      visibilityState: 'visible',
    }),
    false,
  );

  assert.equal(
    viewModels.shouldPassiveCurrentTunnelProviderRefresh?.({
      currentTunnel: {
        provider: 'ngrok',
        state: 'stopped',
        ngrok_authtoken: 'token',
        provider_availability: {
          binary_name: 'ngrok',
          installed: true,
        },
      },
      tunnelDrawerOpen: false,
      busy: false,
      visibilityState: 'visible',
    }),
    false,
  );

  assert.equal(
    viewModels.shouldPassiveCurrentTunnelProviderRefresh?.({
      currentTunnel: {
        provider: 'ngrok',
        state: 'stopped',
        provider_availability: {
          binary_name: 'ngrok',
          installed: false,
        },
      },
      tunnelDrawerOpen: true,
      busy: false,
      visibilityState: 'visible',
    }),
    false,
  );
});

test('summarizePassiveDrawerProviderRefresh only reports a status message when readiness changes', () => {
  const previousReadiness = summarizeDrawerProviderReadiness(
    'cloudflared',
    {
      cloudflared: {
        binary_name: 'cloudflared',
        installed: false,
      },
    },
    'macOS',
  );

  const nextReadiness = summarizeDrawerProviderReadiness(
    'cloudflared',
    {
      cloudflared: {
        binary_name: 'cloudflared',
        installed: true,
      },
    },
    'macOS',
  );

  assert.deepEqual(
    summarizePassiveDrawerProviderRefresh({
      provider: 'cloudflared',
      previousReadiness,
      nextReadiness,
    }),
    {
      message: 'Cloudflared is ready. Save and Start is available again.',
      isError: false,
    },
  );

  assert.equal(
    summarizePassiveDrawerProviderRefresh({
      provider: 'cloudflared',
      previousReadiness,
      nextReadiness: previousReadiness,
    }),
    null,
  );
});

test('summarizePassiveEmptyStateProviderRefresh only reports a handoff when provider readiness changes', () => {
  const previousGuidance = summarizeEmptyStateProviderGuidance(
    {
      cloudflared: {
        binary_name: 'cloudflared',
        installed: false,
      },
      ngrok: {
        binary_name: 'ngrok',
        installed: false,
      },
    },
    'macOS',
  );
  const nextSnapshot = {
    cloudflared: {
      binary_name: 'cloudflared',
      installed: false,
    },
    ngrok: {
      binary_name: 'ngrok',
      installed: true,
    },
  };
  const nextGuidance = summarizeEmptyStateProviderGuidance(nextSnapshot, 'macOS');

  assert.deepEqual(
    viewModels.summarizePassiveEmptyStateProviderRefresh?.({
      previousGuidance,
      nextGuidance,
      nextProviderAvailabilitySnapshot: nextSnapshot,
    }),
    {
      message: 'Ngrok is ready. Create Tunnel is available.',
      isError: false,
      statusAction: {
        kind: 'create_tunnel',
        label: 'Create Tunnel',
      },
    },
  );

  assert.equal(
    viewModels.summarizePassiveEmptyStateProviderRefresh?.({
      previousGuidance,
      nextGuidance: previousGuidance,
      nextProviderAvailabilitySnapshot: {
        cloudflared: {
          binary_name: 'cloudflared',
          installed: false,
        },
        ngrok: {
          binary_name: 'ngrok',
          installed: false,
        },
      },
    }) ?? null,
    null,
  );
});

test('summarizePassiveCurrentTunnelProviderRefresh only reports Start Tunnel when missing-provider recovery makes the tunnel startable again', () => {
  const previousTunnel = {
    provider: 'ngrok',
    state: 'stopped',
    provider_availability: {
      binary_name: 'ngrok',
      installed: false,
    },
  };
  const nextTunnel = {
    provider: 'ngrok',
    state: 'stopped',
    ngrok_authtoken: 'token',
    provider_availability: {
      binary_name: 'ngrok',
      installed: true,
    },
  };

  assert.deepEqual(
    viewModels.summarizePassiveCurrentTunnelProviderRefresh?.({
      previousTunnel,
      nextTunnel,
    }),
    {
      message: 'Ngrok is ready. Start Tunnel is available again.',
      isError: false,
      statusAction: {
        kind: 'start_tunnel',
        label: 'Start Tunnel',
      },
    },
  );

  assert.equal(
    viewModels.summarizePassiveCurrentTunnelProviderRefresh?.({
      previousTunnel,
      nextTunnel: {
        ...nextTunnel,
        state: 'running',
      },
    }) ?? null,
    null,
  );
});

test('app wires passive provider refresh on focus and visibility return for empty-state and drawer recovery', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /async function refreshProviderAvailabilityOnForeground\(\) \{[\s\S]*await refreshEmptyStateProviderAvailabilityOnForeground\(\);[\s\S]*await refreshCurrentTunnelProviderAvailabilityOnForeground\(\);[\s\S]*await refreshDrawerProviderAvailabilityOnForeground\(\);[\s\S]*\}/);
  assert.match(appJs, /window\.addEventListener\('focus', \(\) => void refreshProviderAvailabilityOnForeground\(\)\);/);
  assert.match(appJs, /document\.addEventListener\('visibilitychange', \(\) => \{[\s\S]*document\.visibilityState !== 'visible'[\s\S]*refreshProviderAvailabilityOnForeground\(\)/);
});

test('current-tunnel passive provider recovery reuses the Start Tunnel follow-through after readiness changes', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /async function refreshCurrentTunnelProviderAvailabilityOnForeground\(\) \{[\s\S]*const previousTunnel = getCurrentTunnelDetails\(\);[\s\S]*const statusUpdate = summarizePassiveCurrentTunnelProviderRefresh\(\{[\s\S]*renderStatus\(statusUpdate\.message, statusUpdate\.isError, statusUpdate\.statusAction\);[\s\S]*\}/);
});

test('shouldOpenTunnelAdvanced keeps provider fields collapsed by default and opens them for recovery targets', () => {
  assert.equal(
    shouldOpenTunnelAdvanced(
      {
        provider: 'ngrok',
        cloudflared_tunnel_token: null,
        ngrok_authtoken: null,
        ngrok_domain: null,
      },
      null,
    ),
    true,
  );

  assert.equal(
    shouldOpenTunnelAdvanced(
      {
        provider: 'ngrok',
        cloudflared_tunnel_token: null,
        ngrok_authtoken: null,
        ngrok_domain: null,
      },
      'ngrok_authtoken',
    ),
    true,
  );

  assert.equal(
    shouldOpenTunnelAdvanced(
      {
        provider: 'cloudflared',
        cloudflared_tunnel_token: null,
        ngrok_authtoken: null,
        ngrok_domain: null,
      },
      'cloudflared_tunnel_token',
    ),
    true,
  );

  assert.equal(
    shouldOpenTunnelAdvanced(
      {
        provider: 'ngrok',
        cloudflared_tunnel_token: null,
        ngrok_authtoken: null,
        ngrok_domain: 'demo.ngrok.app',
      },
      null,
    ),
    true,
  );
});

test('getProviderInstallGuidance returns OS-aware commands for common platforms', () => {
  assert.equal(
    getProviderInstallGuidance('cloudflared', 'macOS').command,
    'brew install cloudflared',
  );

  assert.match(
    getProviderInstallGuidance('cloudflared', 'Linux').command,
    /apt-get install cloudflared/,
  );

  assert.equal(
    getProviderInstallGuidance('ngrok', 'Windows NT').command,
    'winget install --id ngrok.ngrok',
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


test('summarizeDaemonRecoveryAction prefers retry for the default local daemon path', () => {
  assert.deepEqual(
    summarizeDaemonRecoveryAction(
      {
        connected: false,
        ownership: 'unavailable',
        message: 'Could not start local TunnelMux: tunnelmuxd binary could not be found in PATH',
      },
      {
        base_url: 'http://127.0.0.1:4765',
      },
    ),
    {
      kind: 'retry_local_daemon',
      label: 'Retry Local Daemon',
    },
  );
});

test('summarizeDaemonUnavailableMessage translates missing tunnelmuxd startup failures into install guidance', () => {
  assert.equal(
    summarizeDaemonUnavailableMessage('Could not start local TunnelMux: tunnelmuxd binary could not be found in bundled resources or PATH'),
    'TunnelMux could not start its local daemon because the tunnelmuxd component is unavailable. Reinstall the TunnelMux app, or install tunnelmuxd separately and make sure it is on your PATH.',
  );
});

test('summarizeDiagnosticsLoadError maps daemon request failures to friendly diagnostics copy', () => {
  assert.equal(
    summarizeDiagnosticsLoadError(
      'upstream health',
      'request failed:\n  http://127.0.0.1:4765/v1/upstreams/health',
    ),
    'Local TunnelMux daemon is unavailable, so upstream health is unavailable right now.',
  );

  assert.equal(
    summarizeDiagnosticsLoadError(
      'recent logs',
      'failed to parse successful response body: {"ok":true}',
    ),
    'Failed to load recent logs: failed to parse successful response body: {"ok":true}',
  );
});

test('summarizeDaemonRecoveryAction prefers settings when the daemon base URL is custom', () => {
  assert.deepEqual(
    summarizeDaemonRecoveryAction(
      {
        connected: false,
        ownership: 'unavailable',
        message: 'connection refused',
      },
      {
        base_url: 'http://127.0.0.1:9900',
      },
    ),
    {
      kind: 'open_settings',
      label: 'Open Settings',
    },
  );
});

test('summarizeDaemonRecoveryAction stays hidden while the managed local daemon is bootstrapping', () => {
  assert.equal(
    summarizeDaemonRecoveryAction(
      {
        connected: false,
        ownership: 'unavailable',
        bootstrapping: true,
        message: 'Starting local TunnelMux…',
      },
      {
        base_url: 'http://127.0.0.1:4765',
      },
    ),
    null,
  );
});

test('summarizeDashboardGuidance keeps add-service guidance when no services exist yet', () => {
  assert.deepEqual(
    summarizeDashboardGuidance({
      connected: true,
      public_url: 'https://demo.trycloudflare.com',
      tunnel_state: 'running',
      enabled_services: 0,
      route_count: 0,
      named_cloudflared: false,
      message: 'Connected, but waiting for services.',
    }),
    {
      home_public_url_meta: 'Add a service before sharing this URL. Until then, visitors only see the default welcome page.',
      dashboard_message: 'Add a service before sharing.',
    },
  );
});

test('summarizeDashboardGuidance points back to saved disabled services before sharing a live URL', () => {
  assert.deepEqual(
    summarizeDashboardGuidance({
      connected: true,
      public_url: 'https://demo.trycloudflare.com',
      tunnel_state: 'running',
      enabled_services: 0,
      route_count: 2,
      named_cloudflared: false,
      message: 'Connected, but waiting for services.',
    }),
    {
      home_public_url_meta: 'Review or enable a saved service before sharing this URL. Until then, visitors only see the default welcome page.',
      dashboard_message: 'Review services before sharing.',
    },
  );
});

test('summarizeDashboardGuidance treats a running quick tunnel without a public URL as pending publish', () => {
  assert.deepEqual(
    summarizeDashboardGuidance({
      connected: true,
      public_url: '',
      tunnel_state: 'running',
      enabled_services: 1,
      named_cloudflared: false,
      message: 'Tunnel started.',
    }),
    {
      home_public_url_meta: 'TunnelMux is waiting for the provider to publish a shareable URL.',
      dashboard_message: 'Waiting for a shareable URL.',
    },
  );
});

test('resolveRouteFormTitle uses first-service copy only for create mode with zero services', () => {
  assert.equal(
    resolveRouteFormTitle({ editing_route_id: null, route_count: 0 }),
    'Add First Service',
  );

  assert.equal(
    resolveRouteFormTitle({ editing_route_id: null, route_count: 2 }),
    'Add Service',
  );

  assert.equal(
    resolveRouteFormTitle({ editing_route_id: 'docs', route_count: 0 }),
    'Edit Service: docs',
  );
});

test('resolveServiceDrawerPrimaryField focuses Local Service URL for first-service create and edit flows', () => {
  assert.equal(
    resolveServiceDrawerPrimaryField({ editing_route_id: null, route_count: 0 }),
    'route-upstream-url',
  );

  assert.equal(
    resolveServiceDrawerPrimaryField({ editing_route_id: null, route_count: 2 }),
    'route-id',
  );

  assert.equal(
    resolveServiceDrawerPrimaryField({ editing_route_id: 'docs', route_count: 0 }),
    'route-upstream-url',
  );
});

test('summarizeRouteSaveStatus nudges sharing only after the first enabled service goes live', () => {
  assert.equal(
    summarizeRouteSaveStatus({
      tunnel_state: 'running',
      public_url: 'https://demo.trycloudflare.com',
      previous_enabled_services: 0,
      next_enabled_services: 1,
      message: 'Route saved.',
    }),
    'Service saved. Copy URL to share.',
  );

  assert.equal(
    summarizeRouteSaveStatus({
      tunnel_state: 'running',
      public_url: '',
      previous_enabled_services: 0,
      next_enabled_services: 1,
      message: 'Route saved.',
    }),
    'Service saved. Waiting for public URL to share.',
  );

  assert.equal(
    summarizeRouteSaveStatus({
      tunnel_state: 'running',
      public_url: 'https://demo.trycloudflare.com',
      previous_enabled_services: 1,
      next_enabled_services: 2,
      message: 'Route saved.',
    }),
    'Route saved.',
  );

  assert.equal(
    summarizeRouteSaveStatus({
      tunnel_state: 'stopped',
      public_url: '',
      previous_enabled_services: 0,
      next_enabled_services: 1,
      message: 'Route saved.',
    }),
    'Route saved.',
  );
});

test('summarizeRouteSaveFailure maps friendly invalid Local Service URL guidance back to the service drawer', () => {
  assert.deepEqual(
    summarizeRouteSaveFailure({
      message: 'Invalid Local Service URL. Review it in this service, then save again.',
    }),
    {
      message: 'Invalid Local Service URL. Review it in this service, then save again.',
      recoveryTarget: 'route-upstream-url',
    },
  );

  assert.deepEqual(
    summarizeRouteSaveFailure({
      message: 'Invalid Fallback Local URL. Review it in this service, then save again.',
    }),
    {
      message: 'Invalid Fallback Local URL. Review it in this service, then save again.',
      recoveryTarget: 'route-fallback-upstream-url',
      openAdvanced: true,
    },
  );

  assert.deepEqual(
    summarizeRouteSaveFailure({
      message: 'Health Check Path must be a slash path like /healthz. Remove any ?query or #fragment, then save again.',
    }),
    {
      message: 'Health Check Path must be a slash path like /healthz. Remove any ?query or #fragment, then save again.',
      recoveryTarget: 'route-health-check-path',
      openAdvanced: true,
    },
  );

  assert.deepEqual(
    summarizeRouteSaveFailure({
      message: 'Service Name "local-3000" is already in use for this tunnel. Rename it and save again.',
    }),
    {
      message: 'Service Name "local-3000" is already in use for this tunnel. Rename it and save again.',
      recoveryTarget: 'route-id',
    },
  );

  assert.deepEqual(
    summarizeRouteSaveFailure({
      message: 'Add a Local Service URL or enter a Service Name before saving.',
    }),
    {
      message: 'Add a Local Service URL or enter a Service Name before saving.',
    },
  );

  assert.equal(
    summarizeRouteSaveFailure({
      message: 'route request failed hard',
    }),
    null,
  );
});

test('summarizeStartSuccessAction offers Add Service before Copy URL on zero-service starts', () => {
  assert.deepEqual(
    summarizeStartSuccessAction({
      public_url: '',
      enabled_services: 0,
      route_count: 0,
      named_cloudflared: false,
      tunnel_state: 'running',
    }),
    {
      kind: 'add_service',
      label: 'Add Service',
      payload: null,
    },
  );

  assert.deepEqual(
    summarizeStartSuccessAction({
      public_url: 'https://demo.trycloudflare.com',
      enabled_services: 1,
      route_count: 1,
      named_cloudflared: false,
      tunnel_state: 'running',
    }),
    {
      kind: 'copy_public_url',
      label: 'Copy URL',
      payload: null,
    },
  );
});

test('summarizeStartSuccessAction offers Review Services when saved services are all disabled', () => {
  assert.deepEqual(
    summarizeStartSuccessAction({
      public_url: '',
      enabled_services: 0,
      route_count: 2,
      named_cloudflared: false,
      tunnel_state: 'running',
    }),
    {
      kind: 'review_services',
      label: 'Review Services',
      payload: null,
    },
  );
});

test('summarizeStartSuccessAction keeps named-cloudflare and not-yet-live states text-only once services exist', () => {
  assert.equal(
    summarizeStartSuccessAction({
      public_url: '',
      enabled_services: 1,
      named_cloudflared: true,
      tunnel_state: 'running',
    }),
    null,
  );

  assert.equal(
    summarizeStartSuccessAction({
      public_url: '',
      enabled_services: 1,
      named_cloudflared: false,
      tunnel_state: 'running',
    }),
    null,
  );
});

test('summarizeShareStatusAction only exposes Copy URL for live shareable states', () => {
  assert.deepEqual(
    summarizeShareStatusAction({
      public_url: 'https://demo.trycloudflare.com',
      enabled_services: 1,
    }),
    {
      kind: 'copy_public_url',
      label: 'Copy URL',
    },
  );

  assert.equal(
    summarizeShareStatusAction({
      public_url: '',
      enabled_services: 1,
    }),
    null,
  );

  assert.equal(
    summarizeShareStatusAction({
      public_url: 'https://demo.trycloudflare.com',
      enabled_services: 0,
    }),
    null,
  );
});

test('summarizeStartReadyStatusAction only exposes Start Tunnel for provider-ready non-running tunnels', () => {
  assert.deepEqual(
    viewModels.summarizeStartReadyStatusAction?.({
      state: 'stopped',
      provider: 'ngrok',
      ngrok_authtoken: 'token',
      provider_availability: {
        binary_name: 'ngrok',
        installed: true,
      },
    }),
    {
      kind: 'start_tunnel',
      label: 'Start Tunnel',
    },
  );

  assert.equal(
    viewModels.summarizeStartReadyStatusAction?.({
      state: 'running',
      provider: 'ngrok',
      ngrok_authtoken: 'token',
      provider_availability: {
        binary_name: 'ngrok',
        installed: true,
      },
    }) ?? null,
    null,
  );

  assert.equal(
    viewModels.summarizeStartReadyStatusAction?.({
      state: 'stopped',
      provider: 'ngrok',
      provider_availability: {
        binary_name: 'ngrok',
        installed: false,
      },
    }) ?? null,
    null,
  );
});

test('resolveDashboardPublicUrlActions only exposes dashboard share actions for live shareable states', () => {
  assert.deepEqual(
    resolveDashboardPublicUrlActions({
      public_url: 'https://demo.trycloudflare.com',
      enabled_services: 1,
      tunnel_state: 'running',
      named_cloudflared: false,
    }),
    {
      show_copy_public_url: true,
      show_open_public_url: true,
      show_manage_provider: false,
    },
  );

  assert.deepEqual(
    resolveDashboardPublicUrlActions({
      public_url: 'https://demo.trycloudflare.com',
      enabled_services: 0,
      tunnel_state: 'running',
      named_cloudflared: false,
    }),
    {
      show_copy_public_url: false,
      show_open_public_url: false,
      show_manage_provider: false,
    },
  );

  assert.deepEqual(
    resolveDashboardPublicUrlActions({
      public_url: '',
      enabled_services: 1,
      tunnel_state: 'running',
      named_cloudflared: false,
    }),
    {
      show_copy_public_url: false,
      show_open_public_url: false,
      show_manage_provider: false,
    },
  );

  assert.deepEqual(
    resolveDashboardPublicUrlActions({
      public_url: '',
      enabled_services: 0,
      tunnel_state: 'running',
      named_cloudflared: true,
    }),
    {
      show_copy_public_url: false,
      show_open_public_url: false,
      show_manage_provider: true,
    },
  );
});

test('summarizeZeroServiceHeroAction distinguishes missing services from disabled ones', () => {
  assert.deepEqual(
    summarizeZeroServiceHeroAction({
      connected: true,
      tunnel_state: 'running',
      enabled_services: 0,
      route_count: 0,
    }),
    {
      kind: 'add_service',
      label: 'Add Service',
    },
  );

  assert.deepEqual(
    summarizeZeroServiceHeroAction({
      connected: true,
      tunnel_state: 'running',
      enabled_services: 0,
      route_count: 2,
    }),
    {
      kind: 'review_services',
      label: 'Review Services',
    },
  );

  assert.equal(
    summarizeZeroServiceHeroAction({
      connected: true,
      tunnel_state: 'running',
      enabled_services: 1,
      route_count: 1,
    }),
    null,
  );

  assert.equal(
    summarizeZeroServiceHeroAction({
      connected: false,
      tunnel_state: 'running',
      enabled_services: 0,
      route_count: 0,
    }),
    null,
  );
});

test('hero service handoff keeps add-service reset wiring and reuses services review highlighting', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /elements\.heroAddService\?\.addEventListener\('click',[\s\S]*state\.heroActionKind === 'review_services'[\s\S]*highlightServicesPanel\(\);[\s\S]*return;[\s\S]*resetRouteForm\(\);[\s\S]*openServiceDrawer\(\);/);
  assert.match(appJs, /elements\.heroAddService\?\.addEventListener\('click',[\s\S]*resetRouteForm\(\);[\s\S]*openServiceDrawer\(\);/);
  assert.match(appJs, /elements\.newRouteEmpty\?\.addEventListener\('click',[\s\S]*resetRouteForm\(\);[\s\S]*openServiceDrawer\(\);/);
});

test('openServiceDrawer schedules autofocus for the primary field', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /function openServiceDrawer\(\) \{[\s\S]*requestAnimationFrame\(\(\) => focusServiceDrawerPrimaryField\(\)\);[\s\S]*\}/);
  assert.match(appJs, /function focusServiceDrawerPrimaryField\(\) \{/);
  assert.match(appJs, /resolveServiceDrawerPrimaryField\(\{[\s\S]*editing_route_id: state\.editingOriginalId,[\s\S]*route_count: currentRouteCount\(\),[\s\S]*\}\)/);
});

test('openTunnelDrawer schedules create-name focus and preserves recovery-target focus', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /function openTunnelDrawer\(\{ mode, recoveryTarget = null \}\) \{[\s\S]*requestAnimationFrame\(\(\) => focusTunnelDrawerPrimaryField\(\{ mode, recoveryTarget \}\)\);[\s\S]*\}/);
  assert.match(appJs, /function focusTunnelDrawerPrimaryField\(\{ mode, recoveryTarget \}\) \{[\s\S]*const field = resolveTunnelRecoveryField\(recoveryTarget\) \|\| \(mode === 'create' \? elements\.tunnelName : null\);/);
});

test('status action handler supports Add Service follow-through after tunnel start', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /case 'add_service':[\s\S]*resetRouteForm\(\);[\s\S]*openServiceDrawer\(\);/);
  assert.match(appJs, /case 'review_services':[\s\S]*highlightServicesPanel\(\);/);
  assert.match(appJs, /const statusAction = summarizeStartSuccessAction\([\s\S]*renderStatus\(statusAction\?\.kind === 'add_service' \? 'Tunnel started\. Add Service to keep going\.' : statusAction\?\.kind === 'review_services' \? 'Tunnel started\. Review Services to enable one\.' : statusAction\?\.kind === 'copy_public_url' \? 'Tunnel started\. Copy URL to share\.' : 'Tunnel started\.', false, statusAction\);/);
});

test('status action handler supports Copy URL follow-through and wires it after start and save', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /case 'copy_public_url':[\s\S]*await copyPublicUrl\(\);/);
  assert.match(appJs, /const statusAction = summarizeStartSuccessAction\([\s\S]*renderStatus\(statusAction\?\.kind === 'add_service' \? 'Tunnel started\. Add Service to keep going\.' : statusAction\?\.kind === 'review_services' \? 'Tunnel started\. Review Services to enable one\.' : statusAction\?\.kind === 'copy_public_url' \? 'Tunnel started\. Copy URL to share\.' : 'Tunnel started\.', false, statusAction\);/);
  assert.match(appJs, /const shareAction = summarizeShareStatusAction\([\s\S]*renderStatus\(summarizeRouteSaveStatus\([\s\S]*\), false, shareAction\);/);
});

test('save flows reuse Start Tunnel follow-through only when saving makes starting the next safe step', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /const statusAction = summarizeStartReadyStatusAction\(getCurrentTunnelDetails\(\)\);[\s\S]*renderStatus\(statusAction \? 'Tunnel saved\. Start Tunnel is available\.' : 'Tunnel saved\.', false, statusAction\);/);
  assert.match(appJs, /const startAction = previousEnabledServices === 0 && nextEnabledServices > 0[\s\S]*summarizeStartReadyStatusAction\(getCurrentTunnelDetails\(\)\)[\s\S]*if \(startAction\) \{[\s\S]*renderStatus\('Service saved\. Start Tunnel to keep going\.', false, startAction\);[\s\S]*\}[\s\S]*const shareAction = summarizeShareStatusAction\([\s\S]*public_url: getCurrentTunnelDetails\(\)\?\.public_base_url \?\? '',[\s\S]*renderStatus\(summarizeRouteSaveStatus\([\s\S]*public_url: getCurrentTunnelDetails\(\)\?\.public_base_url \?\? '',[\s\S]*\), false, shareAction\);/);
});


test('renderDashboard shows a pending public-url label while a quick tunnel is publishing', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /elements\.publicUrl\.textContent = publicUrl \|\| \([\s\S]*tunnelState === 'running' && namedCloudflared[\s\S]*\? 'Managed in Cloudflare'[\s\S]*: tunnelState === 'running'[\s\S]*\? 'Waiting for public URL…'[\s\S]*: 'Not running'[\s\S]*\);/);
});


test('saveTunnel keeps tunnel save validation in-drawer and reuses start recovery wiring', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /async function saveTunnel\(\{ startNow \}\) \{[\s\S]*catch \(error\) \{[\s\S]*const message = formatError\(error\);[\s\S]*const failure = summarizeStartFailureRecovery\(\{[\s\S]*message,[\s\S]*settings: state\.settings,[\s\S]*tunnel: collectTunnelProfile\(\),[\s\S]*\}\);[\s\S]*if \(failure\.recoveryTarget\) \{[\s\S]*renderStatus\(message, true\);[\s\S]*requestAnimationFrame\(\(\) => focusTunnelDrawerPrimaryField\(\{[\s\S]*mode: state\.tunnelEditorMode,[\s\S]*recoveryTarget: failure\.recoveryTarget \?\? null,[\s\S]*\}\)\);[\s\S]*return;[\s\S]*\}[\s\S]*renderStatus\(`Failed to save tunnel: \$\{message\}`, true\);[\s\S]*\}/);
});


test('saveSettings refreshes daemon status and dependent views after a successful save', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /async function saveSettings\(\) \{[\s\S]*const result = await invoke\('save_settings', \{ settings: collectSettingsForm\(\) \}\);[\s\S]*populateSettingsFields\(result\.settings\);[\s\S]*renderDaemonStatus\(result\.daemon_status\);[\s\S]*await refreshAll\(\);[\s\S]*closeSettingsDrawer\(\);[\s\S]*\}/);
  assert.doesNotMatch(appJs, /async function saveSettings\(\) \{[\s\S]*renderStatus\('App settings saved\.'/);
});

test('renderDaemonStatus keeps managed-daemon bootstrapping on a pending path and suppresses refresh churn', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /daemonBootstrapping: false,/);
  assert.match(appJs, /daemonConnected: false,/);
  assert.match(appJs, /state\.daemonBootstrapping = Boolean\(snapshot\?\.bootstrapping\);[\s\S]*if \(state\.daemonBootstrapping\) \{[\s\S]*renderStatus\(message \|\| 'Starting local TunnelMux…', false, null\);[\s\S]*return;[\s\S]*\}/);
  assert.match(appJs, /function renderDaemonStatus\(snapshot\) \{[\s\S]*state\.daemonConnected = connected;[\s\S]*renderHomeProviderActions\(\);/);
  assert.match(appJs, /async function refreshTunnelWorkspace\(\) \{[\s\S]*catch \(error\) \{[\s\S]*if \(state\.daemonBootstrapping\) \{[\s\S]*return;[\s\S]*\}[\s\S]*renderStatus\(`Failed to load tunnels: \$\{formatError\(error\)\}`, true\);/);
  assert.match(appJs, /async function refreshDashboard\(\) \{[\s\S]*catch \(error\) \{[\s\S]*if \(state\.daemonBootstrapping\) \{[\s\S]*return;[\s\S]*\}[\s\S]*renderStatus\(`Failed to refresh dashboard: \$\{formatError\(error\)\}`, true\);/);
  assert.match(appJs, /async function refreshRoutes\(\) \{[\s\S]*catch \(error\) \{[\s\S]*if \(state\.daemonBootstrapping\) \{[\s\S]*return;[\s\S]*\}[\s\S]*renderRoutes\(\{ routes: \[], message: `Failed to load services: \$\{formatError\(error\)\}` \}\);/);
});

test('startup polls daemon_connection_state and escalates stalled managed boots', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /window\.addEventListener\('DOMContentLoaded', async \(\) => \{[\s\S]*const daemon = await invoke\('daemon_connection_state'\);[\s\S]*if \(daemon\?\.bootstrapping\) \{[\s\S]*renderDaemonStatus\(daemon\);[\s\S]*await waitForManagedDaemonBootstrap\(\);[\s\S]*\} else \{[\s\S]*await ensureLocalDaemonAndRefresh\(\);[\s\S]*\}/);
  assert.match(appJs, /async function waitForManagedDaemonBootstrap\(\) \{[\s\S]*for \(let attempt = 0; attempt < 48; attempt \+= 1\) \{[\s\S]*const daemon = await invoke\('daemon_connection_state'\);[\s\S]*renderDaemonStatus\(daemon\);[\s\S]*if \(!daemon\?\.bootstrapping\) \{[\s\S]*await refreshAll\(\);[\s\S]*return;[\s\S]*\}[\s\S]*await new Promise\(\(resolve\) => setTimeout\(resolve, 250\)\);[\s\S]*\}[\s\S]*renderDaemonStatus\(\{[\s\S]*connected: false,[\s\S]*ownership: 'unavailable',[\s\S]*message: 'Starting local TunnelMux is taking longer than expected\. Retry the local daemon or check whether another app is already using this port\.',[\s\S]*\}\);[\s\S]*\}/);
});


test('save-and-start failures keep the drawer open and reuse status-action payload recovery', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /state\.statusActionPayload = statusAction\?\.payload \?\? null;/);
  assert.match(appJs, /case 'edit_tunnel':[\s\S]*openTunnelDrawer\(\{ mode: 'edit', recoveryTarget: state\.statusActionPayload \?\? null \}\);/);
  assert.match(appJs, /const startResult = await startTunnel\(\);/);
  assert.match(appJs, /if \(!startResult\.ok\) \{/);
  assert.match(appJs, /if \(startResult\.recoveryTarget\) \{/);
  assert.match(appJs, /openTunnelDrawer\(\{ mode: 'edit', recoveryTarget: startResult\.recoveryTarget \?\? null \}\);/);
  assert.match(appJs, /if \(!startResult\.ok\) \{[\s\S]*if \(startResult\.recoveryTarget\) \{[\s\S]*openTunnelDrawer\(\{ mode: 'edit', recoveryTarget: startResult\.recoveryTarget \?\? null \}\);[\s\S]*return;[\s\S]*\}[\s\S]*closeTunnelDrawer\(\);[\s\S]*return;[\s\S]*\}/);
  assert.match(appJs, /const failure = summarizeStartFailureRecovery\(\{/);
  assert.match(appJs, /renderStatus\(`Failed to start tunnel: \$\{message\}`, true, failure\.statusAction\);/);
});

test('startTunnel ensures the local daemon before requesting tunnel start and aborts when the daemon stays unavailable', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /async function startTunnel\(\) \{[\s\S]*await ensureLocalDaemonAndRefresh\(\);[\s\S]*if \(state\.daemonBootstrapping \|\| !state\.daemonConnected\) \{[\s\S]*return \{[\s\S]*ok: false,[\s\S]*recoveryTarget: null,[\s\S]*\};[\s\S]*\}[\s\S]*const snapshot = await invoke\('start_tunnel'/);
});

test('home start action stays disabled while the daemon is unavailable or still bootstrapping', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /elements\.startTunnel\.disabled = state\.busy \|\| Boolean\(actionState\.start_disabled\) \|\| state\.daemonBootstrapping \|\| !state\.daemonConnected;/);
});

test('provider install actions invoke the backend installer instead of copy-only clipboard flows', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /case 'install_provider':[\s\S]*await invoke\('install_provider', \{ provider: actionPayload \}\);/);
  assert.match(appJs, /case 'install_provider':[\s\S]*const installStatus = await invoke\('install_provider', \{ provider: actionPayload \}\);/);
  assert.match(appJs, /case 'install_provider':[\s\S]*installStatus\?\.state === 'installed' && installStatus\?\.source === 'local_tools'/);
  assert.match(appJs, /case 'install_provider':[\s\S]*renderStatus\(`Installed \$\{actionPayload\} for TunnelMux\.[\s\S]*\);/);
  assert.match(appJs, /case 'install_provider':[\s\S]*renderStatus\(`Opened the \$\{actionPayload\} system installer\.[\s\S]*\);/);
});

test('provider-status actions edit a single unreachable service directly and highlight multi-service recovery', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /case 'edit_service':\s*[\s\S]*openServiceEditorForRoute\(actionPayload\);/);
  assert.match(appJs, /case 'review_services':\s*[\s\S]*highlightServicesPanel\(\);/);
});

test('provider-status actions keep Cloudflare setup handoffs available', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /case 'open_cloudflare':\s*[\s\S]*openCloudflareDashboard\(\);/);
  assert.match(appJs, /case 'open_cloudflare_docs':\s*[\s\S]*openCloudflareDocs\(\);/);
  assert.match(appJs, /function resolveTunnelRecoveryField\(recoveryTarget\) \{[\s\S]*case 'cloudflared_tunnel_token':[\s\S]*return elements\.tunnelCloudflaredTunnelToken;/);
});

test('saveRoute keeps the service drawer open and refocuses Local Service URL for recoverable save errors', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /function focusServiceDrawerField\(recoveryTarget = null\) \{[\s\S]*resolveServiceRecoveryField\(recoveryTarget\) \|\| resolveServiceDrawerPrimaryField/);
  assert.match(appJs, /const failure = summarizeRouteSaveFailure\(\{ message: formatError\(error\) \}\);[\s\S]*if \(failure\) \{[\s\S]*renderStatus\(failure\.message, true\);[\s\S]*requestAnimationFrame\(\(\) => focusServiceDrawerField\(failure\.recoveryTarget\)\);[\s\S]*return;[\s\S]*\}[\s\S]*renderStatus\(`Failed to save service: \$\{formatError\(error\)\}`, true\);/);
});

test('saveRoute opens advanced service fields before focusing recoverable fallback URL errors', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /function resolveServiceRecoveryField\(recoveryTarget\) \{[\s\S]*case 'route-fallback-upstream-url':[\s\S]*return 'route-fallback-upstream-url';/);
  assert.match(appJs, /if \(failure\.openAdvanced\) \{[\s\S]*elements\.serviceAdvanced\.open = true;[\s\S]*\}/);
});

test('saveRoute can refocus Service Name for duplicate-name recovery', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /function resolveServiceRecoveryField\(recoveryTarget\) \{[\s\S]*case 'route-id':[\s\S]*return 'route-id';/);
});

test('saveRoute routes recoverable health check errors to the advanced health field', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /function resolveServiceRecoveryField\(recoveryTarget\) \{[\s\S]*case 'route-health-check-path':[\s\S]*return 'route-health-check-path';/);
  assert.match(appJs, /const field = resolvedFieldId === 'route-upstream-url'[\s\S]*resolvedFieldId === 'route-fallback-upstream-url'[\s\S]*elements\.routeHealthCheckPath/);
});

test('provider recheck success wires explicit start follow-through actions', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /case 'start_tunnel':[\s\S]*await startTunnel\(\);/);
  assert.match(appJs, /case 'save_and_start_tunnel':[\s\S]*await saveTunnel\(\{ startNow: true \}\);/);
  assert.match(appJs, /const statusAction = summarizeProviderRecheckFollowThrough\([\s\S]*renderStatus\([\s\S]*statusAction\);/);
});

test('empty-state provider recovery reuses the Create Tunnel handoff and status action wiring', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /if \(source === 'empty'\) \{[\s\S]*const guidance = summarizeEmptyStateProviderGuidance\(snapshot\);[\s\S]*const statusAction = summarizeProviderRecheckFollowThrough\([\s\S]*renderStatus\(statusMessage, false, statusAction\);[\s\S]*\}/);
  assert.match(appJs, /case 'create_tunnel':[\s\S]*openTunnelDrawer\(\{ mode: 'create' \}\);/);
});

test('installed-provider recovery passes availability snapshots through home and provider-status wiring', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /const actionState = summarizeHomeTunnelActions\(currentTunnel, undefined, state\.providerAvailabilitySnapshot\);/);
  assert.match(appJs, /const actionState = summarizeHomeTunnelActions\(getCurrentTunnelDetails\(\), undefined, state\.providerAvailabilitySnapshot\);/);
  assert.match(appJs, /const providerAvailabilitySummary = summarizeProviderAvailability\(currentTunnel, undefined, state\.providerAvailabilitySnapshot\);/);
  assert.match(appJs, /case 'use_installed_provider':[\s\S]*openTunnelDrawer\(\{ mode: 'edit', recoveryTarget: actionPayload === 'ngrok' \? 'ngrok_authtoken' : null \}\);[\s\S]*elements\.tunnelProvider\.value = actionPayload;[\s\S]*syncTunnelProviderFields\(\);/);
});

test('collectTunnelProfile reuses create defaults instead of falling back to Untitled Tunnel', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.doesNotMatch(appJs, /Untitled Tunnel/);
  assert.match(appJs, /const defaults = state\.tunnelEditorMode === 'create'[\s\S]*resolveCreateTunnelDefaults\(state\.providerAvailabilitySnapshot\)/);
  assert.match(appJs, /name: elements\.tunnelName\.value\.trim\(\) \|\| defaults\.name,/);
  assert.match(appJs, /gateway_target_url: elements\.tunnelGatewayTargetUrl\.value\.trim\(\) \|\| defaults\.gateway_target_url,/);
});

test('index empty-state copy follows the easy path', () => {
  const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(
    html,
    /Create Tunnel → Install provider if needed → Start Tunnel → Add Service → Share URL/,
  );
  assert.match(html, /id="empty-provider-copy"/);
  assert.match(html, /id="empty-provider-action"/);
  assert.match(html, /id="empty-provider-follow-up-action"/);
  assert.match(html, /id="hero-add-service"/);
  assert.match(appJs, /heroAddService\?\.addEventListener\('click',[\s\S]*openServiceDrawer\(\)/);
  assert.match(appJs, /emptyProviderAction\?\.addEventListener\('click', \(\) => withBusy\(handleEmptyProviderAction\)\)/);
  assert.match(appJs, /emptyProviderFollowUpAction\?\.addEventListener\('click', \(\) => withBusy\(handleEmptyProviderFollowUpAction\)\)/);
  assert.match(appJs, /await runProviderUiAction\(summary\.follow_up_action_kind, summary\.follow_up_action_payload, 'empty'\);/);
  assert.match(appJs, /renderEmptyStateProviderGuidance\(\);/);
});

test('services empty state gives the Add Service action more vertical separation', () => {
  const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
  const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');

  assert.match(html, /<div id="routes-empty" class="empty-state">[\s\S]*<div class="actions">[\s\S]*id="new-route-empty"/);
  assert.match(css, /#routes-empty \.actions \{[\s\S]*margin-top: 16px;/);
});

test('diagnostics panels route request errors through the shared diagnostics copy helper', () => {
  const appJs = readFileSync(new URL('./app.js', import.meta.url), 'utf8');

  assert.match(appJs, /renderDiagnosticsSummaryMeta\(summarizeDiagnosticsLoadError\('runtime summary', error\), true\);/);
  assert.match(appJs, /renderUpstreamsMeta\(summarizeDiagnosticsLoadError\('upstream health', error\), true\);/);
  assert.match(appJs, /renderLogsMeta\(summarizeDiagnosticsLoadError\('recent logs', error\), true\);/);
});

test('shouldShowErrorDetailsAction only enables the action for error states', () => {
  assert.equal(shouldShowErrorDetailsAction({ isError: true }), true);
  assert.equal(shouldShowErrorDetailsAction({ isError: false }), false);
});

test('summarizeStatusMessage compresses protocol mismatch errors', () => {
  const text = summarizeStatusMessage(
    'Daemon unavailable: failed to parse successful response body: {"tunnel":{"state":"running"}}',
    true,
  );

  assert.equal(text, 'Daemon response format mismatch. Restart the latest tunnelmuxd.');
});

test('classifyRoutesPanel distinguishes empty from request failure', () => {
  assert.deepEqual(
    classifyRoutesPanel({ routes: [], message: 'No services yet.' }, 0),
    { mode: 'empty', notice: '' },
  );

  assert.deepEqual(
    classifyRoutesPanel({ routes: [], message: 'Failed to load services: request failed' }, 0),
    { mode: 'error', notice: 'Could not load services right now.' },
  );

  assert.deepEqual(
    classifyRoutesPanel({ routes: [], message: 'Failed to load services: request failed' }, 2),
    { mode: 'stale', notice: 'Could not refresh services. Showing the last known list.' },
  );
});
