const isTauri = typeof window.__TAURI__ !== 'undefined' && window.__TAURI__.core;
const invoke = (command, payload = {}) => {
  if (!isTauri) {
    return Promise.reject(new Error('Tauri bridge is unavailable in preview mode.'));
  }
  return window.__TAURI__.core.invoke(command, payload);
};

const CLOUDFLARE_DASHBOARD_URL = 'https://one.dash.cloudflare.com/';
const CLOUDFLARE_TUNNEL_DOCS_URL = 'https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/get-started/create-remote-tunnel/';

const elements = {};
const state = {
  busy: false,
  routeCache: [],
  editingOriginalId: null,
  settingsDrawerOpen: false,
  serviceDrawerOpen: false,
  providerStatusAction: null,
  diagnostics: {
    logLines: 100,
    summary: null,
    upstreams: [],
    logTail: null,
    summaryUpdatedAt: null,
    upstreamsUpdatedAt: null,
    logsUpdatedAt: null,
    inFlight: {
      summary: false,
      upstreams: false,
      logs: false,
    },
  },
};

window.addEventListener('DOMContentLoaded', async () => {
  bindElements();
  bindEvents();
  resetRouteForm();

  if (!isTauri) {
    renderStatus('Preview shell loaded outside Tauri. Open the desktop app to enable commands.', true);
    renderDashboard({
      connected: false,
      tunnel: null,
      message: 'Preview mode only.',
    });
    renderRoutes({ routes: [], message: 'Services preview unavailable outside Tauri.' });
    renderProviderStatusSummary(null);
    renderDiagnosticsOverview('Troubleshooting preview unavailable outside Tauri.', true);
    renderDiagnosticsSummaryMeta('Preview mode only.', true);
    renderUpstreamsMeta('Preview mode only.', true);
    renderLogsMeta('Preview mode only.', true);
    renderRecentLogs({ requested_lines: state.diagnostics.logLines, lines: [] });
    return;
  }

  await loadSettings();
  await ensureLocalDaemonAndRefresh();
});

function bindElements() {
  elements.status = document.getElementById('app-status');
  elements.retryConnection = document.getElementById('retry-connection');
  elements.openSettings = document.getElementById('open-settings');

  elements.publicUrl = document.getElementById('dashboard-public-url');
  elements.stateBadge = document.getElementById('dashboard-state-badge');
  elements.homePublicUrlMeta = document.getElementById('home-public-url-meta');
  elements.dashboardConnected = document.getElementById('dashboard-connected');
  elements.dashboardProvider = document.getElementById('dashboard-provider');
  elements.servicesEnabledCount = document.getElementById('services-enabled-count');
  elements.dashboardMessage = document.getElementById('dashboard-message');
  elements.homeProviderHint = document.getElementById('home-provider-hint');
  elements.providerStatusCard = document.getElementById('provider-status-card');
  elements.providerStatusTitle = document.getElementById('provider-status-title');
  elements.providerStatusMessage = document.getElementById('provider-status-message');
  elements.providerStatusBadge = document.getElementById('provider-status-badge');
  elements.providerStatusAction = document.getElementById('provider-status-action');

  elements.startTunnel = document.getElementById('start-tunnel');
  elements.stopTunnel = document.getElementById('stop-tunnel');
  elements.copyPublicUrl = document.getElementById('copy-public-url');
  elements.openPublicUrl = document.getElementById('open-public-url');
  elements.manageProvider = document.getElementById('manage-provider');

  elements.routesMessage = document.getElementById('routes-message');
  elements.routesEmpty = document.getElementById('routes-empty');
  elements.routesEmptyTitle = document.getElementById('routes-empty-title');
  elements.routesEmptyCopy = document.getElementById('routes-empty-copy');
  elements.routesList = document.getElementById('routes-list');
  elements.newRoute = document.getElementById('new-route');
  elements.newRouteEmpty = document.getElementById('new-route-empty');
  elements.servicesShell = document.getElementById('services-shell');

  elements.serviceBackdrop = document.getElementById('service-backdrop');
  elements.serviceDrawer = document.getElementById('service-drawer');
  elements.cancelRouteEdit = document.getElementById('cancel-route-edit');
  elements.routeFormTitle = document.getElementById('route-form-title');
  elements.routeId = document.getElementById('route-id');
  elements.routeUpstreamUrl = document.getElementById('route-upstream-url');
  elements.routeMatchPathPrefix = document.getElementById('route-match-path-prefix');
  elements.routeEnabled = document.getElementById('route-enabled');
  elements.routeMatchHost = document.getElementById('route-match-host');
  elements.routeHealthCheckPath = document.getElementById('route-health-check-path');
  elements.routeFallbackUpstreamUrl = document.getElementById('route-fallback-upstream-url');
  elements.serviceAdvanced = document.getElementById('service-advanced');
  elements.serviceExposureMode = document.getElementById('service-exposure-mode');
  elements.serviceHostField = document.getElementById('service-host-field');
  elements.saveRoute = document.getElementById('save-route');

  elements.settingsBackdrop = document.getElementById('settings-backdrop');
  elements.settingsDrawer = document.getElementById('settings-drawer');
  elements.closeSettings = document.getElementById('close-settings');
  elements.baseUrl = document.getElementById('settings-base-url');
  elements.token = document.getElementById('settings-token');
  elements.settingsDefaultProvider = document.getElementById('settings-default-provider');
  elements.settingsAdvanced = document.getElementById('settings-advanced');
  elements.settingsGatewayTargetUrl = document.getElementById('settings-gateway-target-url');
  elements.settingsAutoRestart = document.getElementById('settings-auto-restart');
  elements.settingsCloudflaredFields = document.getElementById('settings-cloudflared-fields');
  elements.settingsCloudflaredTunnelToken = document.getElementById('settings-cloudflared-tunnel-token');
  elements.settingsCloudflaredNote = document.getElementById('settings-cloudflared-note');
  elements.openCloudflareDashboard = document.getElementById('open-cloudflare-dashboard');
  elements.openCloudflareDocs = document.getElementById('open-cloudflare-docs');
  elements.settingsNgrokFields = document.getElementById('settings-ngrok-fields');
  elements.settingsNgrokAuthtoken = document.getElementById('settings-ngrok-authtoken');
  elements.settingsNgrokDomain = document.getElementById('settings-ngrok-domain');
  elements.saveSettings = document.getElementById('save-settings');

  elements.troubleshootingDetails = document.getElementById('troubleshooting-details');
  elements.refreshDiagnostics = document.getElementById('refresh-diagnostics');
  elements.refreshLogs = document.getElementById('refresh-logs');
  elements.clearLogs = document.getElementById('clear-logs');
  elements.diagnosticsOverview = document.getElementById('diagnostics-overview');
  elements.diagnosticsSummaryMeta = document.getElementById('diagnostics-summary-meta');
  elements.diagnosticsTunnelState = document.getElementById('diagnostics-tunnel-state');
  elements.diagnosticsPendingRestart = document.getElementById('diagnostics-pending-restart');
  elements.diagnosticsRouteCount = document.getElementById('diagnostics-route-count');
  elements.diagnosticsEnabledRouteCount = document.getElementById('diagnostics-enabled-route-count');
  elements.diagnosticsConfigReload = document.getElementById('diagnostics-config-reload');
  elements.diagnosticsConfigReloadInterval = document.getElementById('diagnostics-config-reload-interval');
  elements.diagnosticsLastReloadAt = document.getElementById('diagnostics-last-reload-at');
  elements.diagnosticsProviderLogFile = document.getElementById('diagnostics-provider-log-file');
  elements.diagnosticsLastReloadError = document.getElementById('diagnostics-last-reload-error');
  elements.upstreamsMeta = document.getElementById('upstreams-meta');
  elements.upstreamsEmpty = document.getElementById('upstreams-empty');
  elements.upstreamsList = document.getElementById('upstreams-list');
  elements.logsMeta = document.getElementById('logs-meta');
  elements.logLinesSelect = document.getElementById('log-lines-select');
  elements.recentLogs = document.getElementById('recent-logs');
}

function bindEvents() {
  elements.retryConnection?.addEventListener('click', () => withBusy(ensureLocalDaemonAndRefresh));
  elements.openSettings?.addEventListener('click', () => openSettingsDrawer());
  elements.closeSettings?.addEventListener('click', closeSettingsDrawer);
  elements.settingsBackdrop?.addEventListener('click', closeSettingsDrawer);

  elements.startTunnel?.addEventListener('click', () => withBusy(startTunnel));
  elements.stopTunnel?.addEventListener('click', () => withBusy(stopTunnel));
  elements.copyPublicUrl?.addEventListener('click', () => withBusy(copyPublicUrl));
  elements.openPublicUrl?.addEventListener('click', () => withBusy(openPublicUrl));
  elements.manageProvider?.addEventListener('click', openCloudflareDashboard);

  elements.newRoute?.addEventListener('click', () => {
    resetRouteForm();
    openServiceDrawer();
  });
  elements.newRouteEmpty?.addEventListener('click', () => {
    resetRouteForm();
    openServiceDrawer();
  });
  elements.cancelRouteEdit?.addEventListener('click', closeServiceDrawer);
  elements.serviceBackdrop?.addEventListener('click', closeServiceDrawer);
  elements.saveRoute?.addEventListener('click', () => withBusy(saveRoute));
  elements.serviceExposureMode?.addEventListener('change', applyExposureMode);

  elements.saveSettings?.addEventListener('click', () => withBusy(saveSettings));
  elements.settingsDefaultProvider?.addEventListener('change', syncProviderHints);
  elements.openCloudflareDashboard?.addEventListener('click', openCloudflareDashboard);
  elements.openCloudflareDocs?.addEventListener('click', openCloudflareDocs);
  elements.providerStatusAction?.addEventListener('click', handleProviderStatusAction);

  elements.refreshDiagnostics?.addEventListener('click', () => withBusy(() => refreshDiagnosticsWorkspace({ manual: true })));
  elements.refreshLogs?.addEventListener('click', () => withBusy(() => refreshRecentLogs({ manual: true })));
  elements.clearLogs?.addEventListener('click', clearDisplayedLogs);
  elements.logLinesSelect?.addEventListener('change', () => {
    state.diagnostics.logLines = Number(elements.logLinesSelect.value) || 100;
    if (elements.troubleshootingDetails?.open) {
      void refreshRecentLogs({ manual: false });
    }
  });
  elements.troubleshootingDetails?.addEventListener('toggle', () => {
    if (elements.troubleshootingDetails.open) {
      void refreshDiagnosticsWorkspace({ manual: false });
    }
  });
}

function openSettingsDrawer() {
  state.settingsDrawerOpen = true;
  elements.settingsBackdrop.hidden = false;
  elements.settingsDrawer.hidden = false;
}

function closeSettingsDrawer() {
  state.settingsDrawerOpen = false;
  elements.settingsBackdrop.hidden = true;
  elements.settingsDrawer.hidden = true;
}

function openServiceDrawer() {
  state.serviceDrawerOpen = true;
  elements.serviceBackdrop.hidden = false;
  elements.serviceDrawer.hidden = false;
}

function closeServiceDrawer() {
  state.serviceDrawerOpen = false;
  elements.serviceBackdrop.hidden = true;
  elements.serviceDrawer.hidden = true;
}

async function withBusy(fn) {
  if (state.busy) {
    return;
  }
  state.busy = true;
  setBusyState(true);
  try {
    await fn();
  } finally {
    state.busy = false;
    setBusyState(false);
  }
}

function setBusyState(nextBusy) {
  const controls = [
    elements.retryConnection,
    elements.openSettings,
    elements.startTunnel,
    elements.stopTunnel,
    elements.copyPublicUrl,
    elements.openPublicUrl,
    elements.newRoute,
    elements.newRouteEmpty,
    elements.cancelRouteEdit,
    elements.saveRoute,
    elements.closeSettings,
    elements.saveSettings,
    elements.refreshDiagnostics,
    elements.refreshLogs,
    elements.clearLogs,
    elements.logLinesSelect,
  ];

  controls.filter(Boolean).forEach((element) => {
    element.disabled = nextBusy;
  });

  document.querySelectorAll('[data-route-action]').forEach((button) => {
    button.disabled = nextBusy;
  });
}

function collectSettingsForm() {
  return {
    base_url: elements.baseUrl.value,
    token: elements.token.value || null,
    default_provider: elements.settingsDefaultProvider.value,
    gateway_target_url: elements.settingsGatewayTargetUrl.value,
    auto_restart: elements.settingsAutoRestart.checked,
    cloudflared_tunnel_token: elements.settingsCloudflaredTunnelToken.value || null,
    ngrok_authtoken: elements.settingsNgrokAuthtoken.value || null,
    ngrok_domain: elements.settingsNgrokDomain.value || null,
  };
}

function populateSettingsFields(settings) {
  elements.baseUrl.value = settings.base_url ?? '';
  elements.token.value = settings.token ?? '';
  elements.settingsDefaultProvider.value = settings.default_provider ?? 'cloudflared';
  elements.settingsGatewayTargetUrl.value = settings.gateway_target_url ?? 'http://127.0.0.1:48080';
  elements.settingsAutoRestart.checked = Boolean(settings.auto_restart);
  elements.settingsCloudflaredTunnelToken.value = settings.cloudflared_tunnel_token ?? '';
  elements.settingsNgrokAuthtoken.value = settings.ngrok_authtoken ?? '';
  elements.settingsNgrokDomain.value = settings.ngrok_domain ?? '';
  const shouldOpenAdvanced =
    Boolean(elements.settingsCloudflaredTunnelToken.value) ||
    elements.settingsDefaultProvider.value === 'ngrok' ||
    Boolean(elements.settingsNgrokAuthtoken.value) ||
    Boolean(elements.settingsNgrokDomain.value);
  if (elements.settingsAdvanced) {
    elements.settingsAdvanced.open = shouldOpenAdvanced;
  }
  syncProviderHints();
}

function syncProviderHints() {
  const provider = elements.settingsDefaultProvider.value || 'cloudflared';
  const gatewayTarget = elements.settingsGatewayTargetUrl.value || 'http://127.0.0.1:48080';
  const restartLabel = elements.settingsAutoRestart.checked ? 'enabled' : 'disabled';
  const cloudflaredMode = elements.settingsCloudflaredTunnelToken?.value ? 'named tunnel' : 'quick tunnel';
  if (elements.settingsCloudflaredFields) {
    elements.settingsCloudflaredFields.hidden = provider !== 'cloudflared';
  }
  if (elements.settingsNgrokFields) {
    elements.settingsNgrokFields.hidden = provider !== 'ngrok';
  }
  if (elements.settingsCloudflaredNote) {
    elements.settingsCloudflaredNote.hidden = provider !== 'cloudflared';
  }
  if (provider === 'cloudflared') {
    elements.homeProviderHint.textContent = `${provider} ${cloudflaredMode} targets ${gatewayTarget} • auto restart ${restartLabel}.`;
    return;
  }
  elements.homeProviderHint.textContent = `${provider} targets ${gatewayTarget} • auto restart ${restartLabel}.`;
}

async function loadSettings() {
  try {
    const settings = await invoke('load_settings');
    populateSettingsFields(settings);
  } catch (error) {
    renderStatus(`Failed to load settings: ${formatError(error)}`, true);
  }
}

async function saveSettings() {
  try {
    const settings = await invoke('save_settings', { settings: collectSettingsForm() });
    populateSettingsFields(settings);
    renderStatus('Settings saved. Reconnecting to local TunnelMux…');
    await ensureLocalDaemonAndRefresh();
    closeSettingsDrawer();
  } catch (error) {
    renderStatus(`Failed to save settings: ${formatError(error)}`, true);
  }
}

async function ensureLocalDaemonAndRefresh() {
  try {
    const daemon = await invoke('ensure_local_daemon');
    renderDaemonStatus(daemon);
  } catch (error) {
    renderDaemonStatus({
      connected: false,
      ownership: 'unavailable',
      message: `Could not start local TunnelMux: ${formatError(error)}`,
    });
  }

  await refreshAll();
}

async function refreshAll() {
  await refreshDashboard();
  await refreshRoutes();
  await refreshProviderStatusSummary();
}

async function refreshProviderStatusSummary() {
  try {
    const summary = await invoke('load_provider_status_summary');
    renderProviderStatusSummary(summary);
  } catch {
    renderProviderStatusSummary(null);
  }
}

function renderDaemonStatus(snapshot) {
  const ownership = snapshot?.ownership ?? 'unavailable';
  const connected = Boolean(snapshot?.connected);
  const message = snapshot?.message ?? '';

  if (connected && ownership === 'managed') {
    renderStatus(message || 'Connected to a GUI-managed local TunnelMux daemon.');
    return;
  }

  if (connected && ownership === 'external') {
    renderStatus(message || 'Using an existing local TunnelMux daemon.');
    return;
  }

  renderStatus(message || 'Local TunnelMux is unavailable.', true);
}

async function refreshDashboard() {
  try {
    const snapshot = await invoke('refresh_dashboard');
    renderDashboard(snapshot);
  } catch (error) {
    renderStatus(`Failed to refresh dashboard: ${formatError(error)}`, true);
  }
}

async function refreshRoutes() {
  try {
    const snapshot = await invoke('list_routes');
    renderRoutes(snapshot);
  } catch (error) {
    renderRoutes({ routes: [], message: `Failed to load services: ${formatError(error)}` });
  }
}

async function startTunnel() {
  try {
    const settings = collectSettingsForm();
    const snapshot = await invoke('start_tunnel', {
      input: {
        provider: settings.default_provider,
        target_url: settings.gateway_target_url,
        auto_restart: settings.auto_restart,
      },
    });
    renderDashboard(snapshot);
    renderStatus('Tunnel started.');
  } catch (error) {
    renderStatus(`Failed to start tunnel: ${formatError(error)}`, true);
  }
}

async function stopTunnel() {
  try {
    const snapshot = await invoke('stop_tunnel');
    renderDashboard(snapshot);
    renderStatus('Tunnel stopped.');
  } catch (error) {
    renderStatus(`Failed to stop tunnel: ${formatError(error)}`, true);
  }
}

async function saveRoute() {
  try {
    const exposureMode = elements.serviceExposureMode.value;
    const snapshot = await invoke('save_route', {
      form: {
        original_id: state.editingOriginalId,
        id: elements.routeId.value.trim(),
        match_host: exposureMode === 'subdomain' ? elements.routeMatchHost.value : '',
        match_path_prefix: ensurePath(elements.routeMatchPathPrefix.value),
        strip_path_prefix: '',
        upstream_url: elements.routeUpstreamUrl.value,
        fallback_upstream_url: elements.routeFallbackUpstreamUrl.value,
        health_check_path: elements.routeHealthCheckPath.value,
        enabled: elements.routeEnabled.checked,
      },
    });
    renderRoutes(snapshot);
    renderStatus(snapshot.message ?? 'Service saved.');
    resetRouteForm();
    closeServiceDrawer();
  } catch (error) {
    renderStatus(`Failed to save service: ${formatError(error)}`, true);
  }
}

async function deleteRoute(id) {
  if (!window.confirm(`Delete service '${id}'?`)) {
    return;
  }

  try {
    const snapshot = await invoke('delete_route', { id });
    renderRoutes(snapshot);
    renderStatus(snapshot.message ?? 'Service deleted.');
    if (state.editingOriginalId === id) {
      resetRouteForm();
      closeServiceDrawer();
    }
  } catch (error) {
    renderStatus(`Failed to delete service: ${formatError(error)}`, true);
  }
}

async function toggleRouteEnabled(id) {
  const route = state.routeCache.find((item) => item.id === id);
  if (!route) {
    return;
  }

  try {
    const snapshot = await invoke('save_route', {
      form: {
        original_id: route.id,
        id: route.id,
        match_host: route.match_host ?? '',
        match_path_prefix: route.match_path_prefix ?? '/',
        strip_path_prefix: '',
        upstream_url: route.upstream_url,
        fallback_upstream_url: route.fallback_upstream_url ?? '',
        health_check_path: route.health_check_path ?? '',
        enabled: !route.enabled,
      },
    });
    renderRoutes(snapshot);
    renderStatus(`Service ${route.enabled ? 'turned off' : 'turned on'}.`);
  } catch (error) {
    renderStatus(`Failed to update service: ${formatError(error)}`, true);
  }
}

function renderDashboard(snapshot) {
  const tunnel = snapshot?.tunnel ?? null;
  const connected = Boolean(snapshot?.connected);
  const publicUrl = tunnel?.public_base_url ?? '';
  const tunnelState = tunnel?.state ?? (connected ? 'idle' : 'offline');
  const enabledServices = state.routeCache.filter((route) => route.enabled).length;
  const namedCloudflared =
    tunnel?.provider === 'cloudflared' &&
    Boolean(snapshot?.settings?.cloudflared_tunnel_token);

  elements.publicUrl.textContent = publicUrl || (
    tunnelState === 'running' && namedCloudflared
      ? 'Managed in Cloudflare'
      : 'Not running'
  );
  elements.dashboardConnected.textContent = connected ? 'Yes' : 'No';
  elements.dashboardProvider.textContent = tunnel?.provider ?? collectSettingsForm().default_provider ?? '—';
  elements.servicesEnabledCount.textContent = `${enabledServices} enabled`;
  elements.copyPublicUrl.disabled = !publicUrl;
  elements.openPublicUrl.disabled = !publicUrl;
  elements.manageProvider.disabled = !namedCloudflared;
  elements.stopTunnel.disabled = tunnelState !== 'running';
  elements.startTunnel.textContent = tunnelState === 'stopped' || tunnelState === 'error'
    ? 'Restart Tunnel'
    : 'Start Tunnel';
  elements.startTunnel.hidden = tunnelState === 'running';
  elements.copyPublicUrl.hidden = !publicUrl;
  elements.openPublicUrl.hidden = !publicUrl;
  elements.manageProvider.hidden = !(tunnelState === 'running' && namedCloudflared && !publicUrl);
  elements.stopTunnel.hidden = tunnelState !== 'running';

  elements.stateBadge.textContent = titleCase(tunnelState);
  elements.stateBadge.className = `status-pill ${escapeClassName(tunnelState)}`;

  if (!connected) {
    elements.homePublicUrlMeta.textContent = 'TunnelMux is not ready yet. Retry or open Settings.';
    elements.dashboardMessage.textContent = snapshot?.message ?? 'Unable to reach the local daemon.';
    renderStatus(`Daemon unavailable: ${snapshot?.message ?? 'check Settings'}`, true);
    return;
  }

  if (publicUrl) {
    elements.homePublicUrlMeta.textContent = enabledServices > 0
      ? 'Your tunnel is live and ready to share.'
      : 'Your tunnel is live. Visitors will see the default welcome page until you add a service.';
    elements.dashboardMessage.textContent = enabledServices > 0
      ? 'Live now.'
      : 'No services configured yet.';
    renderStatus('Dashboard refreshed.');
    return;
  }

  if (tunnelState === 'running' && namedCloudflared) {
    elements.homePublicUrlMeta.textContent = 'Your named Cloudflare tunnel is connected. Public hostname and Access are managed in Cloudflare.';
    elements.dashboardMessage.textContent = snapshot?.message ?? 'Named tunnel running.';
    renderStatus('Dashboard refreshed.');
    return;
  }

  if (tunnelState === 'stopped' || tunnelState === 'error') {
    elements.homePublicUrlMeta.textContent = 'The previous tunnel is no longer running. Start it again to restore a public URL.';
    elements.dashboardMessage.textContent = snapshot?.message ?? 'Tunnel not running.';
    renderStatus('Dashboard refreshed.', tunnelState === 'error');
    return;
  }

  elements.homePublicUrlMeta.textContent = 'TunnelMux is connected. Start the tunnel to get a public URL.';
  elements.dashboardMessage.textContent = snapshot?.message ?? 'Connected, but not live yet.';
  renderStatus('Dashboard refreshed.');
}

function renderProviderStatusSummary(summary) {
  if (!summary) {
    elements.providerStatusCard.hidden = true;
    state.providerStatusAction = null;
    return;
  }

  elements.providerStatusCard.hidden = false;
  elements.providerStatusTitle.textContent = summary.title ?? 'Provider Status';
  elements.providerStatusMessage.textContent = summary.message ?? '';
  elements.providerStatusBadge.textContent = titleCase(summary.level ?? 'info');
  elements.providerStatusBadge.className = `status-pill ${escapeClassName(summary.level ?? 'idle')}`;
  state.providerStatusAction = summary.action_kind ?? null;
  elements.providerStatusAction.textContent = summary.action_label ?? 'Review';
  elements.providerStatusAction.hidden = !summary.action_kind;
}

function renderRoutes(snapshot) {
  state.routeCache = snapshot?.routes ?? [];
  elements.routesMessage.textContent = state.routeCache.length
    ? 'Services exposed through your current tunnel.'
    : 'Add a local service to route traffic somewhere useful.';
  elements.routesList.innerHTML = '';

  const configured = state.routeCache.length;
  const enabled = state.routeCache.filter((route) => route.enabled).length;
  elements.servicesEnabledCount.textContent = `${enabled} enabled`;
  elements.newRoute.hidden = false;

  if (!state.routeCache.length) {
    const isLoadError = typeof snapshot?.message === 'string' && snapshot.message.startsWith('Failed to load services:');
    elements.routesEmptyTitle.textContent = isLoadError ? 'Could not load services.' : 'No services yet.';
    elements.routesEmptyCopy.textContent = isLoadError
      ? snapshot.message
      : (snapshot?.message ?? 'Add your first local service to replace the default welcome page.');
    elements.routesEmptyCopy.classList.toggle('error', isLoadError);
    elements.routesEmpty.hidden = false;
    elements.dashboardMessage.hidden = true;
    return;
  }

  elements.routesEmpty.hidden = true;
  elements.routesEmptyCopy.classList.remove('error');
  elements.dashboardMessage.hidden = false;

  for (const route of state.routeCache) {
    const item = document.createElement('article');
    item.className = 'service-card';
    item.innerHTML = `
      <div class="service-card-header">
        <div>
          <h3>${escapeHtml(route.id)}</h3>
          <p class="service-exposure">${escapeHtml(describeRouteExposure(route))}</p>
          <p class="service-local">${escapeHtml(route.upstream_url)}</p>
        </div>
        <span class="service-badge ${route.enabled ? 'enabled' : 'disabled'}">${route.enabled ? 'Live' : 'Off'}</span>
      </div>
      <div class="actions compact-actions">
        <button type="button" class="secondary action-chip" data-route-action="edit" data-route-id="${escapeAttribute(route.id)}">Edit</button>
        <button type="button" class="secondary action-chip" data-route-action="toggle" data-route-id="${escapeAttribute(route.id)}">${route.enabled ? 'Disable' : 'Enable'}</button>
        <button type="button" class="secondary action-chip danger-chip" data-route-action="delete" data-route-id="${escapeAttribute(route.id)}">Delete</button>
      </div>
    `;
    elements.routesList.appendChild(item);
  }

  bindRouteActionButtons();
}

async function refreshDiagnosticsWorkspace({ manual = false } = {}) {
  const results = await Promise.all([
    refreshDiagnosticsSummary(),
    refreshUpstreamsHealth(),
    refreshRecentLogs({ manual: false }),
  ]);

  if (manual) {
    const failed = results.some((result) => result === false);
    renderStatus(
      failed ? 'Troubleshooting refreshed with some panel errors.' : 'Troubleshooting refreshed.',
      failed,
    );
  }
}

async function refreshDiagnosticsSummary() {
  if (state.diagnostics.inFlight.summary) {
    return false;
  }
  state.diagnostics.inFlight.summary = true;
  renderDiagnosticsSummaryMeta('Loading runtime summary…');
  try {
    const summary = await invoke('load_diagnostics_summary');
    state.diagnostics.summary = summary;
    state.diagnostics.summaryUpdatedAt = new Date().toISOString();
    renderDiagnosticsSummary(summary);
    renderDiagnosticsOverview('Open these details only when something looks wrong.');
    return true;
  } catch (error) {
    renderDiagnosticsSummaryMeta(`Failed to load runtime summary: ${formatError(error)}`, true);
    renderDiagnosticsOverview('Troubleshooting is partially unavailable. Check the panel errors for details.', true);
    return false;
  } finally {
    state.diagnostics.inFlight.summary = false;
  }
}

async function refreshUpstreamsHealth() {
  if (state.diagnostics.inFlight.upstreams) {
    return false;
  }
  state.diagnostics.inFlight.upstreams = true;
  renderUpstreamsMeta('Loading upstream health…');
  try {
    const upstreams = await invoke('load_upstreams_health');
    state.diagnostics.upstreams = upstreams;
    state.diagnostics.upstreamsUpdatedAt = new Date().toISOString();
    renderUpstreamsHealth(upstreams);
    return true;
  } catch (error) {
    renderUpstreamsMeta(`Failed to load upstream health: ${formatError(error)}`, true);
    elements.upstreamsEmpty.hidden = !state.diagnostics.upstreams.length;
    return false;
  } finally {
    state.diagnostics.inFlight.upstreams = false;
  }
}

async function refreshRecentLogs({ manual = false } = {}) {
  if (state.diagnostics.inFlight.logs) {
    return false;
  }
  state.diagnostics.inFlight.logs = true;
  renderLogsMeta(`Loading last ${state.diagnostics.logLines} log lines…`);
  try {
    const logTail = await invoke('load_recent_logs', { lines: state.diagnostics.logLines });
    state.diagnostics.logTail = logTail;
    state.diagnostics.logsUpdatedAt = new Date().toISOString();
    renderRecentLogs(logTail);
    if (manual) {
      renderStatus('Recent logs refreshed.');
    }
    return true;
  } catch (error) {
    renderLogsMeta(`Failed to load recent logs: ${formatError(error)}`, true);
    if (manual) {
      renderStatus(`Failed to refresh logs: ${formatError(error)}`, true);
    }
    return false;
  } finally {
    state.diagnostics.inFlight.logs = false;
  }
}

function clearDisplayedLogs() {
  state.diagnostics.logTail = {
    requested_lines: state.diagnostics.logLines,
    lines: [],
  };
  renderRecentLogs(state.diagnostics.logTail);
  renderLogsMeta('Local log display cleared. Refresh again if you need a fresh snapshot.');
}

function renderDiagnosticsOverview(message, isError = false) {
  elements.diagnosticsOverview.textContent = message;
  elements.diagnosticsOverview.classList.toggle('error', isError);
}

function renderDiagnosticsSummary(summary) {
  if (!summary) {
    return;
  }

  elements.diagnosticsTunnelState.textContent = summary.tunnel_state ?? '—';
  elements.diagnosticsPendingRestart.textContent = formatYesNo(summary.pending_restart);
  elements.diagnosticsRouteCount.textContent = String(summary.route_count ?? '—');
  elements.diagnosticsEnabledRouteCount.textContent = String(summary.enabled_route_count ?? '—');
  elements.diagnosticsConfigReload.textContent = formatYesNo(summary.config_reload_enabled);
  elements.diagnosticsConfigReloadInterval.textContent = summary.config_reload_interval_ms
    ? `${summary.config_reload_interval_ms} ms`
    : '—';
  elements.diagnosticsLastReloadAt.textContent = formatTimestamp(summary.last_config_reload_at);
  elements.diagnosticsProviderLogFile.textContent = summary.provider_log_file ?? '—';
  elements.diagnosticsLastReloadError.textContent = summary.last_config_reload_error ?? '—';
  renderDiagnosticsSummaryMeta(`Updated ${formatRelativeNow(state.diagnostics.summaryUpdatedAt)}`);
}

function renderUpstreamsHealth(upstreams) {
  elements.upstreamsList.innerHTML = '';
  const items = Array.isArray(upstreams) ? upstreams : [];

  if (!items.length) {
    elements.upstreamsEmpty.hidden = false;
    renderUpstreamsMeta(`Updated ${formatRelativeNow(state.diagnostics.upstreamsUpdatedAt)} • no upstream data returned`);
    return;
  }

  elements.upstreamsEmpty.hidden = true;
  for (const upstream of items) {
    const item = document.createElement('article');
    item.className = 'diagnostics-card';
    item.innerHTML = `
      <div class="diagnostics-card-header">
        <div>
          <h3>${escapeHtml(upstream.upstream_url ?? 'unknown upstream')}</h3>
          <p class="service-exposure">${escapeHtml(upstream.health_check_path ?? '/')}</p>
        </div>
        <span class="health-badge ${escapeAttribute(upstream.health_label ?? 'unknown')}">${escapeHtml(upstream.health_label ?? 'unknown')}</span>
      </div>
      <dl class="route-meta">
        <div><dt>Last Checked</dt><dd>${escapeHtml(formatTimestamp(upstream.last_checked_at))}</dd></div>
        <div><dt>Last Error</dt><dd>${escapeHtml(upstream.last_error ?? '—')}</dd></div>
      </dl>
    `;
    elements.upstreamsList.appendChild(item);
  }

  renderUpstreamsMeta(`Updated ${formatRelativeNow(state.diagnostics.upstreamsUpdatedAt)} • ${items.length} upstream entries`);
}

function renderRecentLogs(logTail) {
  const lines = logTail?.lines ?? [];
  elements.recentLogs.textContent = lines.length ? lines.join('\n') : 'No logs loaded yet.';
  if (logTail?.requested_lines) {
    elements.logLinesSelect.value = String(logTail.requested_lines);
  }
  if (state.diagnostics.logsUpdatedAt) {
    renderLogsMeta(`Updated ${formatRelativeNow(state.diagnostics.logsUpdatedAt)} • showing last ${logTail?.requested_lines ?? state.diagnostics.logLines} lines`);
  }
}

function renderDiagnosticsSummaryMeta(message, isError = false) {
  setPanelMeta(elements.diagnosticsSummaryMeta, message, isError);
}

function renderUpstreamsMeta(message, isError = false) {
  setPanelMeta(elements.upstreamsMeta, message, isError);
}

function renderLogsMeta(message, isError = false) {
  setPanelMeta(elements.logsMeta, message, isError);
}

function setPanelMeta(element, message, isError = false) {
  if (!element) {
    return;
  }
  element.textContent = message;
  element.classList.toggle('error', isError);
}

function bindRouteActionButtons() {
  document.querySelectorAll('[data-route-action="edit"]').forEach((button) => {
    button.addEventListener('click', () => {
      const route = state.routeCache.find((item) => item.id === button.dataset.routeId);
      if (route) {
        populateRouteForm(route);
        openServiceDrawer();
      }
    });
  });

  document.querySelectorAll('[data-route-action="toggle"]').forEach((button) => {
    button.addEventListener('click', () => withBusy(() => toggleRouteEnabled(button.dataset.routeId)));
  });

  document.querySelectorAll('[data-route-action="delete"]').forEach((button) => {
    button.addEventListener('click', () => withBusy(() => deleteRoute(button.dataset.routeId)));
  });
}

function populateRouteForm(route) {
  state.editingOriginalId = route.id;
  elements.routeFormTitle.textContent = `Edit Service: ${route.id}`;
  elements.routeId.value = route.id;
  elements.routeId.disabled = true;
  elements.routeMatchPathPrefix.value = route.match_path_prefix ?? '/';
  elements.routeMatchHost.value = route.match_host ?? '';
  elements.routeUpstreamUrl.value = route.upstream_url ?? '';
  elements.routeFallbackUpstreamUrl.value = route.fallback_upstream_url ?? '';
  elements.routeHealthCheckPath.value = route.health_check_path ?? '';
  elements.routeEnabled.checked = Boolean(route.enabled);
  elements.serviceExposureMode.value = route.match_host ? 'subdomain' : 'path';
  elements.serviceAdvanced.open = Boolean(route.match_host || route.fallback_upstream_url || route.health_check_path);
  applyExposureMode();
  elements.saveRoute.textContent = 'Update Service';
}

function resetRouteForm() {
  state.editingOriginalId = null;
  elements.routeFormTitle.textContent = 'Add Service';
  elements.routeId.disabled = false;
  elements.routeId.value = '';
  elements.routeMatchPathPrefix.value = '/';
  elements.routeMatchHost.value = '';
  elements.routeUpstreamUrl.value = '';
  elements.routeFallbackUpstreamUrl.value = '';
  elements.routeHealthCheckPath.value = '';
  elements.routeEnabled.checked = true;
  elements.serviceExposureMode.value = 'path';
  elements.serviceAdvanced.open = false;
  elements.saveRoute.textContent = 'Save Service';
  applyExposureMode();
}

function applyExposureMode() {
  elements.serviceHostField.hidden = elements.serviceExposureMode.value !== 'subdomain';
}

async function copyPublicUrl() {
  const url = elements.publicUrl.textContent.trim();
  if (!url || url === 'Not running') {
    renderStatus('No public URL is available yet.', true);
    return;
  }

  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(url);
      renderStatus('Public URL copied.');
      return;
    }
    throw new Error('clipboard API unavailable');
  } catch (error) {
    renderStatus(`Failed to copy URL: ${formatError(error)}`, true);
  }
}

async function openPublicUrl() {
  const url = elements.publicUrl.textContent.trim();
  if (!url || url === 'Not running') {
    renderStatus('No public URL is available yet.', true);
    return;
  }
  window.open(url, '_blank', 'noopener,noreferrer');
}

function openCloudflareDashboard() {
  window.open(CLOUDFLARE_DASHBOARD_URL, '_blank', 'noopener,noreferrer');
}

function openCloudflareDocs() {
  window.open(CLOUDFLARE_TUNNEL_DOCS_URL, '_blank', 'noopener,noreferrer');
}

function handleProviderStatusAction() {
  switch (state.providerStatusAction) {
    case 'open_cloudflare':
      openCloudflareDashboard();
      break;
    case 'open_settings':
      openSettingsDrawer();
      break;
    case 'review_services':
      elements.servicesShell?.scrollIntoView({ behavior: 'smooth', block: 'start' });
      break;
    default:
      break;
  }
}

function describeRouteExposure(route) {
  const host = route.match_host?.trim();
  const path = ensurePath(route.match_path_prefix ?? '/');
  return host ? (path === '/' ? host : `${host}${path}`) : path;
}

function ensurePath(value) {
  const trimmed = (value ?? '').trim();
  if (!trimmed) {
    return '/';
  }
  return trimmed.startsWith('/') ? trimmed : `/${trimmed}`;
}

function renderStatus(message, isError = false) {
  elements.status.textContent = message;
  elements.status.classList.toggle('error', isError);
}

function formatYesNo(value) {
  return value ? 'Yes' : 'No';
}

function formatTimestamp(value) {
  if (!value) {
    return '—';
  }
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function formatRelativeNow(value) {
  if (!value) {
    return 'just now';
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleTimeString();
}

function formatError(error) {
  if (typeof error === 'string') {
    return error;
  }
  return error?.message ?? String(error);
}

function titleCase(value) {
  return String(value)
    .replaceAll('_', ' ')
    .replace(/\b\w/g, (match) => match.toUpperCase());
}

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function escapeAttribute(value) {
  return escapeHtml(value);
}

function escapeClassName(value) {
  return String(value).replace(/[^a-z0-9-_]/gi, '-').toLowerCase();
}
