export function titleCase(value) {
  if (!value) {
    return '';
  }
  return String(value)
    .split(/[_\s-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

export function formatCurrentTunnelMeta(tunnel) {
  const provider = tunnel?.provider ?? 'unknown';
  const stateLabel = titleCase(tunnel?.state ?? 'idle');
  const routeCount = Number(tunnel?.route_count ?? 0);
  const enabledRouteCount = Number(tunnel?.enabled_route_count ?? 0);
  return `${provider} • ${stateLabel} • ${enabledRouteCount}/${routeCount} services live`;
}

export function formatHomeProviderHint(tunnel) {
  const missingProviderSummary = summarizeProviderAvailability(tunnel);
  if (missingProviderSummary) {
    return missingProviderSummary.message;
  }

  const provider = tunnel?.provider ?? DEFAULT_CREATE_TUNNEL_PROVIDER;
  const gatewayTarget = tunnel?.gateway_target_url ?? DEFAULT_GUI_GATEWAY_TARGET_URL;
  const restartLabel = tunnel?.auto_restart ? 'enabled' : 'disabled';
  const cloudflaredMode = tunnel?.cloudflared_tunnel_token ? 'named tunnel' : 'quick tunnel';

  if (provider === 'cloudflared') {
    return `${provider} ${cloudflaredMode} targets ${gatewayTarget} • auto restart ${restartLabel}.`;
  }

  return `${provider} targets ${gatewayTarget} • auto restart ${restartLabel}.`;
}

export function formatTunnelOptionLabel(tunnel) {
  const stateLabel = titleCase(tunnel?.state ?? 'idle');
  const routeCount = Number(tunnel?.route_count ?? 0);
  const enabledRouteCount = Number(tunnel?.enabled_route_count ?? 0);
  return `${tunnel?.name ?? 'Tunnel'} • ${stateLabel} • ${enabledRouteCount}/${routeCount}`;
}

export function formatCurrentTunnelUrl(tunnel) {
  const publicBaseUrl = tunnel?.public_base_url ?? '';
  if (publicBaseUrl) {
    return publicBaseUrl;
  }
  if (tunnel?.provider === 'cloudflared' && tunnel?.state === 'running') {
    return 'Managed in Cloudflare';
  }
  return '';
}

export function applyProviderAvailabilitySnapshot(tunnel, providerAvailabilitySnapshot) {
  if (!tunnel) {
    return tunnel;
  }

  const provider = tunnel.provider;
  if (!provider) {
    return tunnel;
  }

  const availability = providerAvailabilitySnapshot?.[provider] ?? null;
  if (!availability) {
    return tunnel;
  }

  return {
    ...tunnel,
    provider_availability: availability,
  };
}

export function tunnelPickerRowClass(tunnel, selected) {
  const state = tunnel?.state ?? 'idle';
  return `tunnel-picker-item${selected ? ' selected' : ''} ${state}`.trim();
}

export function getProviderInstallGuidance(provider, platform = detectInstallPlatform()) {
  const os = detectInstallPlatform(platform);

  if (provider === 'cloudflared') {
    switch (os) {
      case 'macos':
        return { platform: os, command: 'brew install cloudflared' };
      case 'windows':
        return { platform: os, command: 'winget install --id Cloudflare.cloudflared' };
      default:
        return {
          platform: os,
          command: "curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg | sudo gpg --yes --dearmor --output /usr/share/keyrings/cloudflare-main.gpg && echo 'deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared any main' | sudo tee /etc/apt/sources.list.d/cloudflared.list && sudo apt-get update && sudo apt-get install cloudflared",
        };
    }
  }

  switch (os) {
    case 'macos':
      return { platform: os, command: 'brew install ngrok/ngrok/ngrok' };
    case 'windows':
      return { platform: os, command: 'winget install --id ngrok.ngrok' };
    default:
      return {
        platform: os,
        command: "curl -fsSL https://ngrok-agent.s3.amazonaws.com/ngrok.asc | sudo tee /etc/apt/trusted.gpg.d/ngrok.asc >/dev/null && echo 'deb https://ngrok-agent.s3.amazonaws.com buster main' | sudo tee /etc/apt/sources.list.d/ngrok.list && sudo apt update && sudo apt install ngrok",
      };
  }
}

export function summarizeProviderAvailability(
  tunnel,
  platform = detectInstallPlatform(),
  providerAvailabilitySnapshot = null,
) {
  const availability = tunnel?.provider_availability;
  if (availability?.installed === false) {
    const providerName = tunnel?.provider ?? availability.binary_name ?? 'provider';
    const binaryName = availability.binary_name ?? providerName;
    const guidance = getProviderInstallGuidance(providerName, platform);
    const alternateProvider = resolveInstalledAlternativeProvider(
      providerName,
      providerAvailabilitySnapshot,
    );
    return {
      level: 'warning',
      title: `${titleCase(providerName)} Missing`,
      message: `Install ${binaryName} to start this tunnel. TunnelMux could not find the ${binaryName} command in your PATH.`,
      action_kind: 'copy_install_command',
      action_label: 'Copy Install Command',
      action_payload: guidance.command,
      follow_up_action_kind: alternateProvider ? 'use_installed_provider' : 'recheck_provider',
      follow_up_action_label: alternateProvider ? 'Use Installed Provider' : 'Recheck Provider',
      follow_up_action_payload: alternateProvider ?? providerName,
    };
  }

  if (isNgrokAuthtokenMissing(tunnel)) {
    return {
      level: 'warning',
      title: 'ngrok Authtoken Required',
      message: 'Add the ngrok authtoken on this tunnel before starting it.',
      action_kind: 'edit_tunnel',
      action_label: 'Add ngrok Authtoken',
      action_payload: 'ngrok_authtoken',
      follow_up_action_kind: null,
      follow_up_action_label: null,
      follow_up_action_payload: null,
    };
  }

  return null;
}

export function summarizeHomeTunnelActions(
  tunnel,
  platform = detectInstallPlatform(),
  providerAvailabilitySnapshot = null,
) {
  const providerSummary = summarizeProviderAvailability(
    tunnel,
    platform,
    providerAvailabilitySnapshot,
  );
  if (providerSummary) {
    const startLabel = providerSummary.action_kind === 'edit_tunnel'
      ? shouldUseRestartLabel(tunnel?.state)
        ? 'Add Authtoken to Restart'
        : 'Add Authtoken to Start'
      : shouldUseRestartLabel(tunnel?.state)
        ? 'Install Provider to Restart'
        : 'Install Provider to Start';

    return {
      start_disabled: true,
      start_label: startLabel,
      action_kind: providerSummary.action_kind,
      action_label: providerSummary.action_label,
      action_payload: providerSummary.action_payload,
      follow_up_action_kind: providerSummary.follow_up_action_kind,
      follow_up_action_label: providerSummary.follow_up_action_label,
      follow_up_action_payload: providerSummary.follow_up_action_payload,
    };
  }

  return {
    start_disabled: false,
    start_label: shouldUseRestartLabel(tunnel?.state) ? 'Restart Tunnel' : 'Start Tunnel',
    action_kind: null,
    action_label: null,
    action_payload: null,
    follow_up_action_kind: null,
    follow_up_action_label: null,
    follow_up_action_payload: null,
  };
}

export function summarizeProviderRecheckFollowThrough({ source, tunnel_state }) {
  if (source === 'empty') {
    return {
      kind: 'create_tunnel',
      label: 'Create Tunnel',
    };
  }

  if (tunnel_state === 'running' || tunnel_state === 'starting') {
    return null;
  }

  if (source === 'drawer') {
    return {
      kind: 'save_and_start_tunnel',
      label: 'Save and Start',
    };
  }

  if (source === 'home' || source === 'provider_status') {
    return {
      kind: 'start_tunnel',
      label: 'Start Tunnel',
    };
  }

  return null;
}

function resolveInstalledAlternativeProvider(currentProvider, providerAvailabilitySnapshot) {
  if (!currentProvider || !providerAvailabilitySnapshot) {
    return null;
  }

  return SUPPORTED_CREATE_TUNNEL_PROVIDERS.find(
    (provider) => provider !== currentProvider && providerAvailabilitySnapshot?.[provider]?.installed,
  ) ?? null;
}

export function shouldOpenTunnelAdvanced(tunnel, recoveryTarget = null) {
  if (ADVANCED_TUNNEL_RECOVERY_TARGETS.has(recoveryTarget ?? '')) {
    return true;
  }

  if (isNgrokAuthtokenMissing(tunnel)) {
    return true;
  }

  return Boolean(tunnel?.cloudflared_tunnel_token)
    || Boolean(tunnel?.ngrok_authtoken)
    || Boolean(tunnel?.ngrok_domain);
}

export function summarizeDaemonUnavailableMessage(message) {
  const text = String(message ?? '').trim();
  const lower = text.toLowerCase();

  if (!text) {
    return '';
  }

  if (
    lower.includes('tunnelmuxd binary could not be found')
    || lower.includes('tunnelmuxd component is unavailable')
  ) {
    return 'TunnelMux could not start its local daemon because the tunnelmuxd component is unavailable. Reinstall the TunnelMux app, or install tunnelmuxd separately and make sure it is on your PATH.';
  }

  return text;
}

export function summarizeDaemonRecoveryAction(snapshot, settings) {
  if (snapshot?.connected || snapshot?.bootstrapping) {
    return null;
  }

  const baseUrl = normalizeDaemonBaseUrl(settings?.base_url ?? '');
  if (baseUrl && baseUrl !== DEFAULT_DAEMON_BASE_URL) {
    return {
      kind: 'open_settings',
      label: 'Open Settings',
    };
  }

  return {
    kind: 'retry_local_daemon',
    label: 'Retry Local Daemon',
  };
}

export function summarizeStartFailureRecovery({
  message,
  settings,
  tunnel,
}) {
  const lower = String(message ?? '').toLowerCase();

  if (lower.startsWith('install ') && lower.includes('could not find') && lower.includes('path')) {
    return {
      preservesProviderRecovery: true,
      recoveryTarget: null,
      statusAction: null,
    };
  }

  if (
    lower.includes('request failed')
    || lower.includes('error sending request')
    || lower.includes('connection refused')
    || lower.includes('failed to parse successful response body')
  ) {
    const action = summarizeDaemonRecoveryAction({ connected: false }, settings);
    return {
      preservesProviderRecovery: false,
      recoveryTarget: null,
      statusAction: action
        ? {
          ...action,
          payload: null,
        }
        : null,
    };
  }

  if (
    lower.includes('invalid local service url')
    || lower.includes('invalid gateway target url')
    || (lower.includes('invalid') && lower.includes('target url'))
  ) {
    return {
      preservesProviderRecovery: false,
      recoveryTarget: 'gateway_target_url',
      statusAction: {
        kind: 'edit_tunnel',
        label: 'Review Local Service URL',
        payload: 'gateway_target_url',
      },
    };
  }

  if (lower.includes('ngrok authtoken') || lower.includes('authtoken') || lower.includes('err_ngrok_4018')) {
    return {
      preservesProviderRecovery: false,
      recoveryTarget: 'ngrok_authtoken',
      statusAction: {
        kind: 'edit_tunnel',
        label: 'Add ngrok Authtoken',
        payload: 'ngrok_authtoken',
      },
    };
  }

  if ((tunnel?.provider === 'ngrok' && lower.includes('domain')) || lower.includes('reserved domain')) {
    return {
      preservesProviderRecovery: false,
      recoveryTarget: 'ngrok_domain',
      statusAction: {
        kind: 'edit_tunnel',
        label: 'Review ngrok Domain',
        payload: 'ngrok_domain',
      },
    };
  }

  return {
    preservesProviderRecovery: false,
    recoveryTarget: null,
    statusAction: null,
  };
}

export function resolveDashboardPublicUrlActions({
  public_url,
  enabled_services,
  tunnel_state,
  named_cloudflared,
}) {
  const hasShareablePublicUrl = Boolean(public_url) && Number(enabled_services ?? 0) > 0;

  return {
    show_copy_public_url: hasShareablePublicUrl,
    show_open_public_url: hasShareablePublicUrl,
    show_manage_provider: tunnel_state === 'running' && Boolean(named_cloudflared) && !public_url,
  };
}

export function summarizeShareStatusAction({
  public_url,
  enabled_services,
}) {
  if (!public_url || Number(enabled_services ?? 0) === 0) {
    return null;
  }

  return {
    kind: 'copy_public_url',
    label: 'Copy URL',
  };
}

export function summarizeStartReadyStatusAction(tunnel) {
  if (!tunnel || summarizeProviderAvailability(tunnel)) {
    return null;
  }

  if (tunnel.state === 'running' || tunnel.state === 'starting') {
    return null;
  }

  return {
    kind: 'start_tunnel',
    label: 'Start Tunnel',
  };
}

export function summarizeStartSuccessAction({
  public_url,
  enabled_services,
  route_count,
  named_cloudflared,
  tunnel_state,
}) {
  const addServiceAction = summarizeZeroServiceHeroAction({
    connected: true,
    tunnel_state,
    enabled_services,
    route_count,
  });
  if (addServiceAction) {
    return {
      ...addServiceAction,
      payload: null,
    };
  }

  if (named_cloudflared || !public_url) {
    return null;
  }

  const shareAction = summarizeShareStatusAction({
    public_url,
    enabled_services,
  });
  return shareAction
    ? {
      ...shareAction,
      payload: null,
    }
    : null;
}

export function summarizeDashboardGuidance({
  connected,
  public_url,
  tunnel_state,
  enabled_services,
  route_count,
  named_cloudflared,
  message,
}) {
  if (!connected) {
    return {
      home_public_url_meta: 'TunnelMux is not ready yet. Use the recovery action above or open Settings.',
      dashboard_message: message ?? 'Unable to reach the local daemon.',
    };
  }

  if (public_url) {
    return enabled_services > 0
      ? {
        home_public_url_meta: 'Your tunnel is live and ready to share.',
        dashboard_message: 'Live now.',
      }
      : Number(route_count ?? 0) > 0
        ? {
          home_public_url_meta: 'Review or enable a saved service before sharing this URL. Until then, visitors only see the default welcome page.',
          dashboard_message: 'Review services before sharing.',
        }
        : {
          home_public_url_meta: 'Add a service before sharing this URL. Until then, visitors only see the default welcome page.',
          dashboard_message: 'Add a service before sharing.',
        };
  }

  if (tunnel_state === 'running' && named_cloudflared) {
    return {
      home_public_url_meta: 'Your named Cloudflare tunnel is connected. Public hostname and Access are managed in Cloudflare.',
      dashboard_message: message ?? 'Named tunnel running.',
    };
  }

  if (tunnel_state === 'running') {
    return {
      home_public_url_meta: 'TunnelMux is waiting for the provider to publish a shareable URL.',
      dashboard_message: 'Waiting for a shareable URL.',
    };
  }

  if (tunnel_state === 'stopped' || tunnel_state === 'error') {
    return {
      home_public_url_meta: 'The previous tunnel is no longer running. Start it again to restore a public URL.',
      dashboard_message: message ?? 'Tunnel not running.',
    };
  }

  return {
    home_public_url_meta: 'TunnelMux is connected. Start the tunnel to get a public URL.',
    dashboard_message: message ?? 'Connected, but not live yet.',
  };
}

export function summarizeZeroServiceHeroAction({
  connected,
  tunnel_state,
  enabled_services,
  route_count,
}) {
  if (!connected || tunnel_state !== 'running' || Number(enabled_services ?? 0) !== 0) {
    return null;
  }

  return Number(route_count ?? 0) > 0
    ? {
      kind: 'review_services',
      label: 'Review Services',
    }
    : {
      kind: 'add_service',
      label: 'Add Service',
    };
}

export function resolveRouteFormTitle({ editing_route_id, route_count }) {
  if (editing_route_id) {
    return `Edit Service: ${editing_route_id}`;
  }

  return Number(route_count ?? 0) === 0 ? 'Add First Service' : 'Add Service';
}

export function resolveServiceDrawerPrimaryField({ editing_route_id, route_count }) {
  if (editing_route_id || Number(route_count ?? 0) === 0) {
    return 'route-upstream-url';
  }

  return 'route-id';
}

export function summarizeEmptyStateProviderGuidance(
  providerAvailabilitySnapshot,
  platform = detectInstallPlatform(),
) {
  if (!providerAvailabilitySnapshot) {
    return null;
  }

  const installedProviders = resolveInstalledCreateTunnelProviders(providerAvailabilitySnapshot);

  if (installedProviders.length === 0) {
    return {
      message: 'Cloudflared is not installed yet. TunnelMux recommends cloudflared for the quickest first tunnel.',
      action_kind: 'copy_install_command',
      action_label: 'Copy Install Command',
      action_payload: getProviderInstallGuidance(DEFAULT_CREATE_TUNNEL_PROVIDER, platform).command,
      follow_up_action_kind: 'recheck_provider',
      follow_up_action_label: 'Recheck Provider',
      follow_up_action_payload: DEFAULT_CREATE_TUNNEL_PROVIDER,
    };
  }

  if (installedProviders.length === 1) {
    if (installedProviders[0] === 'ngrok') {
      return {
        message: 'Ngrok is installed. Create Tunnel will preselect it, but you will need an authtoken before Start Tunnel works.',
        action_kind: 'copy_install_command',
        action_label: 'Copy cloudflared Install Command',
        action_payload: getProviderInstallGuidance(DEFAULT_CREATE_TUNNEL_PROVIDER, platform).command,
        follow_up_action_kind: null,
        follow_up_action_label: null,
        follow_up_action_payload: null,
      };
    }

    return {
      message: `${titleCase(installedProviders[0])} is installed. Create Tunnel will preselect it.`,
      action_kind: null,
      action_label: null,
      action_payload: null,
      follow_up_action_kind: null,
      follow_up_action_label: null,
      follow_up_action_payload: null,
    };
  }

  return {
    message: 'Cloudflared and ngrok are installed. Create Tunnel lets you choose the provider.',
    action_kind: null,
    action_label: null,
    action_payload: null,
    follow_up_action_kind: null,
    follow_up_action_label: null,
    follow_up_action_payload: null,
  };
}

export function resolvePreferredCreateTunnelProvider(providerAvailabilitySnapshot) {
  const installedProviders = resolveInstalledCreateTunnelProviders(providerAvailabilitySnapshot);

  return installedProviders.length === 1
    ? installedProviders[0]
    : DEFAULT_CREATE_TUNNEL_PROVIDER;
}

export function resolveCreateTunnelDefaults(providerAvailabilitySnapshot) {
  return {
    name: DEFAULT_TUNNEL_NAME,
    provider: resolvePreferredCreateTunnelProvider(providerAvailabilitySnapshot),
    gateway_target_url: DEFAULT_GUI_GATEWAY_TARGET_URL,
    auto_restart: true,
    cloudflared_tunnel_token: null,
    ngrok_authtoken: null,
    ngrok_domain: null,
  };
}

export function summarizeRouteSaveStatus({
  tunnel_state,
  public_url,
  previous_enabled_services,
  next_enabled_services,
  message,
}) {
  if (tunnel_state === 'running'
    && Number(previous_enabled_services ?? 0) === 0
    && Number(next_enabled_services ?? 0) > 0) {
    return public_url
      ? 'Service saved. Copy URL to share.'
      : 'Service saved. Waiting for public URL to share.';
  }

  return message ?? 'Service saved.';
}

export function summarizeRouteSaveFailure({ message }) {
  if (message === 'Invalid Local Service URL. Review it in this service, then save again.') {
    return {
      message,
      recoveryTarget: 'route-upstream-url',
    };
  }

  if (message === 'Invalid Fallback Local URL. Review it in this service, then save again.') {
    return {
      message,
      recoveryTarget: 'route-fallback-upstream-url',
      openAdvanced: true,
    };
  }

  if (message === 'Health Check Path must be a slash path like /healthz. Remove any ?query or #fragment, then save again.') {
    return {
      message,
      recoveryTarget: 'route-health-check-path',
      openAdvanced: true,
    };
  }

  if (String(message ?? '').startsWith('Service Name ') && String(message ?? '').includes('already in use for this tunnel.')) {
    return {
      message,
      recoveryTarget: 'route-id',
    };
  }

  if (message === 'Add a Local Service URL or enter a Service Name before saving.') {
    return { message };
  }

  return null;
}

export function summarizeDrawerProviderReadiness(
  tunnelOrProvider,
  providerAvailabilitySnapshot,
  platform = detectInstallPlatform(),
) {
  const tunnel = typeof tunnelOrProvider === 'string'
    ? null
    : (tunnelOrProvider ?? {});
  const providerName = typeof tunnelOrProvider === 'string'
    ? tunnelOrProvider
    : (tunnel?.provider ?? 'provider');
  const availability = providerAvailabilitySnapshot?.[providerName] ?? {
    binary_name: providerName,
    installed: true,
  };
  const binaryName = availability.binary_name ?? providerName;
  const alternateProvider = resolveInstalledAlternativeProvider(
    providerName,
    providerAvailabilitySnapshot,
  );

  if (availability.installed === false) {
    const quickTunnelNote = providerName === 'cloudflared'
      ? ' Cloudflared is still the recommended quick-tunnel path for the common case, and quick tunnels work without a Cloudflare Tunnel Token.'
      : '';

    return {
      level: 'warning',
      title: `${titleCase(providerName)} Missing`,
      message: `Install ${binaryName} before using Save and Start. TunnelMux could not find the ${binaryName} command in your PATH.${quickTunnelNote}`,
      action_kind: 'copy_install_command',
      action_label: 'Copy Install Command',
      action_payload: getProviderInstallGuidance(providerName, platform).command,
      follow_up_action_kind: alternateProvider ? 'use_installed_provider' : 'recheck_provider',
      follow_up_action_label: alternateProvider ? 'Use Installed Provider' : 'Recheck Provider',
      follow_up_action_payload: alternateProvider ?? providerName,
      start_disabled: true,
      start_label: 'Install Provider to Start',
    };
  }

  if (tunnel && isNgrokAuthtokenMissing(tunnel)) {
    return {
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
    };
  }

  return {
    level: 'info',
    title: `${titleCase(providerName)} Ready`,
    message: `${binaryName} is installed. Save and Start will launch this tunnel after saving.`,
    action_kind: null,
    action_label: null,
    action_payload: null,
    follow_up_action_kind: null,
    follow_up_action_label: null,
    follow_up_action_payload: null,
    start_disabled: false,
    start_label: 'Save and Start',
  };
}

export function shouldPassiveDrawerProviderRefresh({
  tunnelDrawerOpen,
  busy,
  visibilityState,
  provider,
  providerAvailabilitySnapshot,
}) {
  if (!tunnelDrawerOpen || busy || visibilityState === 'hidden') {
    return false;
  }

  return Boolean(
    summarizeDrawerProviderReadiness(provider, providerAvailabilitySnapshot).start_disabled,
  );
}

export function shouldPassiveEmptyStateProviderRefresh({
  hasCurrentTunnel,
  tunnelDrawerOpen,
  busy,
  visibilityState,
  providerAvailabilitySnapshot,
}) {
  if (hasCurrentTunnel || tunnelDrawerOpen || busy || visibilityState === 'hidden') {
    return false;
  }

  return Boolean(
    summarizeEmptyStateProviderGuidance(providerAvailabilitySnapshot)?.follow_up_action_kind,
  );
}

export function shouldPassiveCurrentTunnelProviderRefresh({
  currentTunnel,
  tunnelDrawerOpen,
  busy,
  visibilityState,
}) {
  if (!currentTunnel || tunnelDrawerOpen || busy || visibilityState === 'hidden') {
    return false;
  }

  return Boolean(summarizeProviderAvailability(currentTunnel));
}

export function summarizePassiveDrawerProviderRefresh({
  provider,
  previousReadiness,
  nextReadiness,
}) {
  if (!previousReadiness?.start_disabled || nextReadiness?.start_disabled) {
    return null;
  }

  return {
    message: `${titleCase(provider ?? 'provider')} is ready. Save and Start is available again.`,
    isError: false,
  };
}

export function summarizePassiveEmptyStateProviderRefresh({
  previousGuidance,
  nextGuidance,
  nextProviderAvailabilitySnapshot,
}) {
  if (!previousGuidance?.follow_up_action_kind || nextGuidance?.follow_up_action_kind) {
    return null;
  }

  const installedProviders = resolveInstalledCreateTunnelProviders(nextProviderAvailabilitySnapshot);
  if (!installedProviders.length) {
    return null;
  }

  return {
    message: installedProviders.length === 1
      ? `${titleCase(installedProviders[0])} is ready. Create Tunnel is available.`
      : 'Providers are ready. Create Tunnel is available.',
    isError: false,
    statusAction: {
      kind: 'create_tunnel',
      label: 'Create Tunnel',
    },
  };
}

export function summarizePassiveCurrentTunnelProviderRefresh({
  previousTunnel,
  nextTunnel,
}) {
  if (!summarizeProviderAvailability(previousTunnel) || summarizeProviderAvailability(nextTunnel)) {
    return null;
  }

  const statusAction = summarizeProviderRecheckFollowThrough({
    source: 'home',
    tunnel_state: nextTunnel?.state ?? 'offline',
  });
  if (!statusAction) {
    return null;
  }

  return {
    message: `${titleCase(nextTunnel?.provider ?? 'provider')} is ready. Start Tunnel is available again.`,
    isError: false,
    statusAction,
  };
}

const DEFAULT_DAEMON_BASE_URL = 'http://127.0.0.1:4765';
const DEFAULT_GUI_GATEWAY_TARGET_URL = 'http://127.0.0.1:48080';
const DEFAULT_TUNNEL_NAME = 'Main Tunnel';
const DEFAULT_CREATE_TUNNEL_PROVIDER = 'cloudflared';
const SUPPORTED_CREATE_TUNNEL_PROVIDERS = ['cloudflared', 'ngrok'];
const ADVANCED_TUNNEL_RECOVERY_TARGETS = new Set([
  'gateway_target_url',
  'cloudflared_tunnel_token',
  'ngrok_authtoken',
  'ngrok_domain',
]);

function shouldUseRestartLabel(tunnelState) {
  return tunnelState === 'stopped' || tunnelState === 'error';
}

function isNgrokAuthtokenMissing(tunnel) {
  return tunnel?.provider === 'ngrok' && !String(tunnel?.ngrok_authtoken ?? '').trim();
}

function resolveInstalledCreateTunnelProviders(providerAvailabilitySnapshot) {
  return SUPPORTED_CREATE_TUNNEL_PROVIDERS.filter(
    (provider) => providerAvailabilitySnapshot?.[provider]?.installed === true,
  );
}

function normalizeDaemonBaseUrl(value) {
  const trimmed = String(value ?? '').trim().replace(/\/$/, '');
  if (!trimmed) {
    return DEFAULT_DAEMON_BASE_URL;
  }
  if (/^https?:\/\//i.test(trimmed)) {
    return trimmed;
  }
  return `http://${trimmed}`;
}

function detectInstallPlatform(platform = defaultPlatformString()) {
  const value = String(platform ?? '').toLowerCase();
  if (value.includes('mac') || value.includes('darwin')) {
    return 'macos';
  }
  if (value.includes('win')) {
    return 'windows';
  }
  return 'linux';
}

function defaultPlatformString() {
  return globalThis?.navigator?.userAgentData?.platform
    ?? globalThis?.navigator?.platform
    ?? globalThis?.navigator?.userAgent
    ?? '';
}

export function resolveDashboardStatus(snapshot) {
  if (snapshot?.connected) {
    return null;
  }
  return {
    message: `Daemon unavailable: ${snapshot?.message ?? 'check Settings'}`,
    isError: true,
  };
}

export function shouldShowErrorDetailsAction({ isError }) {
  return Boolean(isError);
}

export function summarizeStatusMessage(message, isError) {
  const text = String(message ?? '').trim();
  if (!text) {
    return '';
  }
  if (!isError) {
    return text;
  }

  if (text.includes('failed to parse successful response body')) {
    return 'Daemon response format mismatch. Restart the latest tunnelmuxd.';
  }

  if (text.length > 140) {
    return `${text.slice(0, 137)}…`;
  }

  return text;
}

export function classifyRoutesPanel(snapshot, previousRoutesCount = 0) {
  const message = typeof snapshot?.message === 'string' ? snapshot.message : '';
  const routes = Array.isArray(snapshot?.routes) ? snapshot.routes : [];
  const isLoadError = message.startsWith('Failed to load services:');

  if (isLoadError) {
    return {
      mode: previousRoutesCount > 0 ? 'stale' : 'error',
      notice: previousRoutesCount > 0
        ? 'Could not refresh services. Showing the last known list.'
        : 'Could not load services right now.',
    };
  }

  if (routes.length === 0) {
    return {
      mode: 'empty',
      notice: '',
    };
  }

  return {
    mode: 'list',
    notice: '',
  };
}
