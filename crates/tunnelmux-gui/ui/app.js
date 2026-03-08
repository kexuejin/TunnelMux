import {
  applyProviderAvailabilitySnapshot,
  classifyRoutesPanel,
  formatCurrentTunnelMeta,
  formatCurrentTunnelUrl,
  formatHomeProviderHint,
  formatTunnelOptionLabel,
  resolveCreateTunnelDefaults,
  resolveDashboardPublicUrlActions,
  resolveDashboardStatus,
  resolveRouteFormTitle,
  resolveServiceDrawerPrimaryField,
  shouldPassiveCurrentTunnelProviderRefresh,
  shouldPassiveEmptyStateProviderRefresh,
  shouldOpenTunnelAdvanced,
  shouldPassiveDrawerProviderRefresh,
  shouldShowErrorDetailsAction,
  summarizePassiveCurrentTunnelProviderRefresh,
  summarizePassiveEmptyStateProviderRefresh,
  summarizePassiveDrawerProviderRefresh,
  summarizeShareStatusAction,
  summarizeStartReadyStatusAction,
  summarizeStartSuccessAction,
  summarizeStartFailureRecovery,
  summarizeDaemonRecoveryAction,
  summarizeDaemonUnavailableMessage,
  summarizeDashboardGuidance,
  summarizeDrawerProviderReadiness,
  summarizeEmptyStateProviderGuidance,
  summarizeHomeTunnelActions,
  summarizeProviderAvailability,
  summarizeProviderRecheckFollowThrough,
  summarizeRouteSaveFailure,
  summarizeRouteSaveStatus,
  summarizeZeroServiceHeroAction,
  summarizeStatusMessage,
  titleCase,
  tunnelPickerRowClass,
} from './tunnel-picker-helpers.mjs';

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
  settings: null,
  tunnelWorkspace: {
    tunnels: [],
    current_tunnel_id: null,
  },
  routeCache: [],
  editingOriginalId: null,
  settingsDrawerOpen: false,
  serviceDrawerOpen: false,
  tunnelDrawerOpen: false,
  tunnelEditorMode: 'create',
  tunnelEditorRecoveryTarget: null,
  confirmResolver: null,
  tunnelPickerOpen: false,
  providerStatusAction: null,
  providerStatusActionPayload: null,
  providerStatusFollowUpAction: null,
  providerStatusFollowUpActionPayload: null,
  providerAvailabilitySnapshot: null,
  daemonBootstrapping: false,
  dashboardConnected: false,
  dashboardTunnelState: 'offline',
  statusActionKind: null,
  statusActionLabel: null,
  statusActionPayload: null,
  statusMessage: '',
  statusIsError: false,
  passiveProviderRefreshInFlight: false,
  servicesAttentionTimer: null,
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
    renderTunnelWorkspace({ tunnels: [], current_tunnel_id: null });
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
  await refreshTunnelWorkspace();
  if (state.tunnelWorkspace.tunnels.length) {
    const daemon = await invoke('daemon_connection_state');
    if (daemon?.bootstrapping) {
      renderDaemonStatus(daemon);
      await waitForManagedDaemonBootstrap();
    } else {
      await ensureLocalDaemonAndRefresh();
    }
  } else {
    renderStatus('No tunnel configured yet. Create a tunnel to get started.');
    renderDashboard({
      connected: false,
      tunnel: null,
      message: 'No tunnel configured yet.',
    });
    renderRoutes({ routes: [], message: 'Create a tunnel before adding services.' });
    renderProviderStatusSummary(null);
  }
});

function bindElements() {
  elements.status = document.getElementById('app-status');
  elements.statusErrorDetails = document.getElementById('status-error-details');
  elements.statusAction = document.getElementById('status-action');
  elements.openSettings = document.getElementById('open-settings');
  elements.tunnelEmptyState = document.getElementById('tunnel-empty-state');
  elements.createTunnelEmpty = document.getElementById('create-tunnel-empty');
  elements.emptyProviderCopy = document.getElementById('empty-provider-copy');
  elements.emptyProviderAction = document.getElementById('empty-provider-action');
  elements.emptyProviderFollowUpAction = document.getElementById('empty-provider-follow-up-action');
  elements.tunnelContextBar = document.getElementById('tunnel-context-bar');
  elements.currentTunnelName = document.getElementById('current-tunnel-name');
  elements.currentTunnelBadge = document.getElementById('current-tunnel-badge');
  elements.currentTunnelMeta = document.getElementById('current-tunnel-meta');
  elements.currentTunnelUrl = document.getElementById('current-tunnel-url');
  elements.tunnelPickerShell = document.getElementById('tunnel-picker-shell');
  elements.tunnelPickerTrigger = document.getElementById('tunnel-picker-trigger');
  elements.tunnelPickerPopover = document.getElementById('tunnel-picker-popover');
  elements.tunnelPickerList = document.getElementById('tunnel-picker-list');
  elements.newTunnel = document.getElementById('new-tunnel');
  elements.editTunnel = document.getElementById('edit-tunnel');
  elements.homeGrid = document.getElementById('home-grid');

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
  elements.providerStatusFollowUpAction = document.getElementById('provider-status-follow-up-action');

  elements.startTunnel = document.getElementById('start-tunnel');
  elements.heroAddService = document.getElementById('hero-add-service');
  elements.homeProviderAction = document.getElementById('home-provider-action');
  elements.homeProviderFollowUpAction = document.getElementById('home-provider-follow-up-action');
  elements.stopTunnel = document.getElementById('stop-tunnel');
  elements.copyPublicUrl = document.getElementById('copy-public-url');
  elements.openPublicUrl = document.getElementById('open-public-url');
  elements.manageProvider = document.getElementById('manage-provider');

  elements.routesMessage = document.getElementById('routes-message');
  elements.servicesNotice = document.getElementById('services-notice');
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

  elements.tunnelBackdrop = document.getElementById('tunnel-backdrop');
  elements.tunnelDrawer = document.getElementById('tunnel-drawer');
  elements.closeTunnel = document.getElementById('close-tunnel');
  elements.tunnelFormTitle = document.getElementById('tunnel-form-title');
  elements.tunnelName = document.getElementById('tunnel-name');
  elements.tunnelProvider = document.getElementById('tunnel-provider');
  elements.tunnelProviderReadinessTitle = document.getElementById('tunnel-provider-readiness-title');
  elements.tunnelProviderReadinessMessage = document.getElementById('tunnel-provider-readiness-message');
  elements.tunnelProviderReadinessBadge = document.getElementById('tunnel-provider-readiness-badge');
  elements.tunnelProviderInstallAction = document.getElementById('tunnel-provider-install-action');
  elements.tunnelProviderRecheckAction = document.getElementById('tunnel-provider-recheck-action');
  elements.tunnelAdvanced = document.getElementById('tunnel-advanced');
  elements.tunnelGatewayTargetUrl = document.getElementById('tunnel-gateway-target-url');
  elements.tunnelAutoRestart = document.getElementById('tunnel-auto-restart');
  elements.tunnelCloudflaredFields = document.getElementById('tunnel-cloudflared-fields');
  elements.tunnelCloudflaredTunnelToken = document.getElementById('tunnel-cloudflared-tunnel-token');
  elements.tunnelNgrokFields = document.getElementById('tunnel-ngrok-fields');
  elements.tunnelNgrokAuthtoken = document.getElementById('tunnel-ngrok-authtoken');
  elements.tunnelNgrokDomain = document.getElementById('tunnel-ngrok-domain');
  elements.deleteTunnel = document.getElementById('delete-tunnel');
  elements.saveTunnel = document.getElementById('save-tunnel');
  elements.saveAndStartTunnel = document.getElementById('save-and-start-tunnel');
  elements.confirmBackdrop = document.getElementById('confirm-backdrop');
  elements.confirmDialog = document.getElementById('confirm-dialog');
  elements.confirmTitle = document.getElementById('confirm-title');
  elements.confirmMessage = document.getElementById('confirm-message');
  elements.confirmCancel = document.getElementById('confirm-cancel');
  elements.confirmConfirm = document.getElementById('confirm-confirm');

  elements.settingsBackdrop = document.getElementById('settings-backdrop');
  elements.settingsDrawer = document.getElementById('settings-drawer');
  elements.closeSettings = document.getElementById('close-settings');
  elements.baseUrl = document.getElementById('settings-base-url');
  elements.token = document.getElementById('settings-token');
  elements.saveSettings = document.getElementById('save-settings');

  elements.troubleshootingDetails = document.getElementById('troubleshooting-details');
  elements.errorDetailsBackdrop = document.getElementById('error-details-backdrop');
  elements.errorDetailsDialog = document.getElementById('error-details-dialog');
  elements.closeErrorDetails = document.getElementById('close-error-details');
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
  elements.openSettings?.addEventListener('click', () => openSettingsDrawer());
  elements.statusAction?.addEventListener('click', () => withBusy(handleStatusAction));
  elements.closeSettings?.addEventListener('click', closeSettingsDrawer);
  elements.settingsBackdrop?.addEventListener('click', closeSettingsDrawer);
  elements.createTunnelEmpty?.addEventListener('click', () => openTunnelDrawer({ mode: 'create' }));
  elements.emptyProviderAction?.addEventListener('click', () => withBusy(handleEmptyProviderAction));
  elements.emptyProviderFollowUpAction?.addEventListener('click', () => withBusy(handleEmptyProviderFollowUpAction));
  elements.newTunnel?.addEventListener('click', () => openTunnelDrawer({ mode: 'create' }));
  elements.editTunnel?.addEventListener('click', () => openTunnelDrawer({ mode: 'edit' }));
  elements.closeTunnel?.addEventListener('click', closeTunnelDrawer);
  elements.tunnelBackdrop?.addEventListener('click', closeTunnelDrawer);
  elements.tunnelProvider?.addEventListener('change', syncTunnelProviderFields);
  elements.tunnelProviderInstallAction?.addEventListener('click', () => void copyTunnelProviderInstallCommand());
  elements.tunnelProviderRecheckAction?.addEventListener('click', () => void withBusy(recheckTunnelDrawerProvider));
  elements.tunnelPickerTrigger?.addEventListener('click', toggleTunnelPicker);
  elements.deleteTunnel?.addEventListener('click', deleteTunnelProfile);
  elements.saveTunnel?.addEventListener('click', () => withBusy(() => saveTunnel({ startNow: false })));
  elements.saveAndStartTunnel?.addEventListener('click', () => withBusy(() => saveTunnel({ startNow: true })));
  elements.confirmBackdrop?.addEventListener('click', () => closeConfirmDialog(false));
  elements.confirmCancel?.addEventListener('click', () => closeConfirmDialog(false));
  elements.confirmConfirm?.addEventListener('click', () => closeConfirmDialog(true));
  elements.statusErrorDetails?.addEventListener('click', () => void openErrorDetailsDialog());
  elements.errorDetailsBackdrop?.addEventListener('click', closeErrorDetailsDialog);
  elements.closeErrorDetails?.addEventListener('click', closeErrorDetailsDialog);
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && state.confirmResolver) {
      closeConfirmDialog(false);
      return;
    }
    if (event.key === 'Escape' && state.tunnelPickerOpen) {
      closeTunnelPicker();
      return;
    }
    if (event.key === 'Escape' && !elements.errorDetailsDialog?.hidden) {
      closeErrorDetailsDialog();
    }
  });
  document.addEventListener('click', (event) => {
    if (!state.tunnelPickerOpen) {
      return;
    }
    const target = event.target;
    if (!(target instanceof Node)) {
      return;
    }
    if (elements.tunnelPickerShell?.contains(target)) {
      return;
    }
    closeTunnelPicker();
  });
  window.addEventListener('focus', () => void refreshProviderAvailabilityOnForeground());
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState !== 'visible') {
      return;
    }
    void refreshProviderAvailabilityOnForeground();
  });

  elements.startTunnel?.addEventListener('click', () => withBusy(startTunnel));
  elements.heroAddService?.addEventListener('click', () => {
    resetRouteForm();
    openServiceDrawer();
  });
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
  elements.homeProviderAction?.addEventListener('click', () => withBusy(handleHomeProviderAction));
  elements.homeProviderFollowUpAction?.addEventListener('click', () => withBusy(handleHomeProviderFollowUpAction));
  elements.providerStatusAction?.addEventListener('click', () => withBusy(handleProviderStatusAction));
  elements.providerStatusFollowUpAction?.addEventListener('click', () => withBusy(handleProviderStatusFollowUpAction));

  elements.refreshDiagnostics?.addEventListener('click', () => withBusy(() => refreshDiagnosticsWorkspace({ manual: true })));
  elements.refreshLogs?.addEventListener('click', () => withBusy(() => refreshRecentLogs({ manual: true })));
  elements.clearLogs?.addEventListener('click', clearDisplayedLogs);
  elements.logLinesSelect?.addEventListener('change', () => {
    state.diagnostics.logLines = Number(elements.logLinesSelect.value) || 100;
    if (!elements.errorDetailsDialog?.hidden) {
      void refreshRecentLogs({ manual: false });
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

function openTunnelDrawer({ mode, recoveryTarget = null }) {
  state.tunnelEditorMode = mode;
  state.tunnelEditorRecoveryTarget = recoveryTarget;
  state.tunnelDrawerOpen = true;
  elements.tunnelBackdrop.hidden = false;
  elements.tunnelDrawer.hidden = false;
  elements.tunnelFormTitle.textContent = mode === 'edit' ? 'Edit Tunnel' : 'Create Tunnel';
  populateTunnelFields(
    mode === 'edit'
      ? getCurrentTunnelSettings()
      : resolveCreateTunnelDefaults(state.providerAvailabilitySnapshot),
    recoveryTarget,
  );
  elements.deleteTunnel.hidden = mode !== 'edit' || (state.settings?.tunnels?.length ?? 0) <= 1;
  requestAnimationFrame(() => focusTunnelDrawerPrimaryField({ mode, recoveryTarget }));
}

function closeTunnelDrawer() {
  state.tunnelDrawerOpen = false;
  state.tunnelEditorRecoveryTarget = null;
  elements.tunnelBackdrop.hidden = true;
  elements.tunnelDrawer.hidden = true;
}

function openServiceDrawer() {
  state.serviceDrawerOpen = true;
  elements.serviceBackdrop.hidden = false;
  elements.serviceDrawer.hidden = false;
  requestAnimationFrame(() => focusServiceDrawerPrimaryField());
}

function openServiceEditorForRoute(routeId = null) {
  const route = routeId
    ? state.routeCache.find((item) => item.id === routeId)
    : (state.routeCache.length === 1 ? state.routeCache[0] : null);

  if (!route) {
    highlightServicesPanel();
    return;
  }

  populateRouteForm(route);
  openServiceDrawer();
}

function highlightServicesPanel() {
  const firstCard = elements.routesList?.querySelector('.service-card');
  elements.servicesShell?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  elements.servicesShell?.classList.add('needs-attention');
  firstCard?.classList.add('needs-attention');

  if (state.servicesAttentionTimer) {
    window.clearTimeout(state.servicesAttentionTimer);
  }

  state.servicesAttentionTimer = window.setTimeout(() => {
    elements.servicesShell?.classList.remove('needs-attention');
    firstCard?.classList.remove('needs-attention');
    state.servicesAttentionTimer = null;
  }, 1600);
}

function closeServiceDrawer() {
  state.serviceDrawerOpen = false;
  elements.serviceBackdrop.hidden = true;
  elements.serviceDrawer.hidden = true;
}

function focusServiceDrawerPrimaryField() {
  focusServiceDrawerField();
}

function focusServiceDrawerField(recoveryTarget = null) {
  const resolvedFieldId = resolveServiceRecoveryField(recoveryTarget) || resolveServiceDrawerPrimaryField({
    editing_route_id: state.editingOriginalId,
    route_count: currentRouteCount(),
  });
  const field = resolvedFieldId === 'route-upstream-url'
    ? elements.routeUpstreamUrl
    : resolvedFieldId === 'route-fallback-upstream-url'
      ? elements.routeFallbackUpstreamUrl
      : resolvedFieldId === 'route-health-check-path'
        ? elements.routeHealthCheckPath
      : elements.routeId;
  if (!field || field.disabled || typeof field.focus !== 'function') {
    return;
  }

  field.focus();
  if (typeof field.select === 'function') {
    field.select();
  }
}

function resolveServiceRecoveryField(recoveryTarget) {
  switch (recoveryTarget) {
    case 'route-id':
      return 'route-id';
    case 'route-upstream-url':
      return 'route-upstream-url';
    case 'route-fallback-upstream-url':
      return 'route-fallback-upstream-url';
    case 'route-health-check-path':
      return 'route-health-check-path';
    default:
      return null;
  }
}

function currentRouteCount() {
  return Number(getCurrentTunnelDetails()?.route_count ?? state.routeCache.length);
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
    elements.openSettings,
    elements.startTunnel,
    elements.heroAddService,
    elements.stopTunnel,
    elements.copyPublicUrl,
    elements.openPublicUrl,
    elements.emptyProviderAction,
    elements.emptyProviderFollowUpAction,
    elements.homeProviderAction,
    elements.homeProviderFollowUpAction,
    elements.providerStatusAction,
    elements.providerStatusFollowUpAction,
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
    elements.statusAction,
  ];

  controls.filter(Boolean).forEach((element) => {
    element.disabled = nextBusy;
  });

  document.querySelectorAll('[data-route-action]').forEach((button) => {
    button.disabled = nextBusy;
  });

  renderHomeProviderActions();
  renderEmptyStateProviderGuidance();
  renderTunnelDrawerProviderReadiness();
}

function collectSettingsForm() {
  return {
    base_url: elements.baseUrl.value,
    token: elements.token.value || null,
    current_tunnel_id: state.settings?.current_tunnel_id ?? null,
    tunnels: state.settings?.tunnels ?? [],
  };
}

function populateSettingsFields(settings) {
  state.settings = settings;
  elements.baseUrl.value = settings.base_url ?? '';
  elements.token.value = settings.token ?? '';
  syncProviderHints();
}

function syncProviderHints() {
  const tunnel = getCurrentTunnelDetails(getCurrentTunnelSettings());
  elements.homeProviderHint.textContent = tunnel ? formatHomeProviderHint(tunnel) : 'No tunnel data yet.';
}

async function refreshProviderAvailabilitySnapshot() {
  try {
    state.providerAvailabilitySnapshot = await invoke('load_provider_availability_snapshot');
  } catch {
    state.providerAvailabilitySnapshot = null;
  }
  syncProviderHints();
  renderHomeProviderActions();
  renderEmptyStateProviderGuidance();
  renderTunnelDrawerProviderReadiness();
  return state.providerAvailabilitySnapshot;
}

async function refreshTunnelWorkspace() {
  try {
    const workspace = await invoke('load_tunnel_workspace');
    state.tunnelWorkspace = workspace;
    renderTunnelWorkspace(workspace);
    await refreshProviderAvailabilitySnapshot();
  } catch (error) {
    if (state.daemonBootstrapping) {
      return;
    }

    renderStatus(`Failed to load tunnels: ${formatError(error)}`, true);
    renderTunnelWorkspace({ tunnels: [], current_tunnel_id: null });
    state.providerAvailabilitySnapshot = null;
    renderHomeProviderActions();
    renderEmptyStateProviderGuidance();
    renderTunnelDrawerProviderReadiness();
  }
}

function getCurrentWorkspaceTunnel() {
  const tunnels = state.tunnelWorkspace?.tunnels ?? [];
  return tunnels.find((tunnel) => tunnel.id === state.tunnelWorkspace?.current_tunnel_id) ?? tunnels[0] ?? null;
}

function getCurrentTunnelDetails(currentTunnel = getCurrentWorkspaceTunnel()) {
  const currentTunnelSettings = getCurrentTunnelSettings();
  if (!currentTunnel && !currentTunnelSettings) {
    return null;
  }

  return applyProviderAvailabilitySnapshot(
    {
      ...currentTunnelSettings,
      ...currentTunnel,
    },
    state.providerAvailabilitySnapshot,
  );
}

function currentProviderAvailabilitySummary() {
  return summarizeProviderAvailability(getCurrentTunnelDetails());
}

function renderHomeProviderActions(currentTunnel = getCurrentTunnelDetails()) {
  if (!elements.startTunnel || !elements.homeProviderAction || !elements.homeProviderFollowUpAction) {
    return;
  }

  if (!currentTunnel) {
    elements.homeProviderAction.hidden = true;
    elements.homeProviderFollowUpAction.hidden = true;
    return;
  }

  const actionState = summarizeHomeTunnelActions(currentTunnel, undefined, state.providerAvailabilitySnapshot);
  elements.startTunnel.disabled = state.busy || Boolean(actionState.start_disabled);
  elements.startTunnel.textContent = actionState.start_label ?? 'Start Tunnel';

  elements.homeProviderAction.textContent = actionState.action_label ?? 'Copy Install Command';
  elements.homeProviderAction.hidden = !actionState.action_kind;
  elements.homeProviderAction.disabled = state.busy;

  elements.homeProviderFollowUpAction.textContent = actionState.follow_up_action_label ?? 'Recheck Provider';
  elements.homeProviderFollowUpAction.hidden = !actionState.follow_up_action_kind;
  elements.homeProviderFollowUpAction.disabled = state.busy;
}

function renderEmptyStateProviderGuidance() {
  if (!elements.emptyProviderCopy || !elements.emptyProviderAction || !elements.emptyProviderFollowUpAction) {
    return;
  }

  const summary = summarizeEmptyStateProviderGuidance(state.providerAvailabilitySnapshot);
  elements.emptyProviderCopy.textContent = summary?.message ?? '';
  elements.emptyProviderCopy.hidden = !summary?.message;

  elements.emptyProviderAction.textContent = summary?.action_label ?? 'Copy Install Command';
  elements.emptyProviderAction.hidden = !summary?.action_kind;
  elements.emptyProviderAction.disabled = state.busy;

  elements.emptyProviderFollowUpAction.textContent = summary?.follow_up_action_label ?? 'Recheck Provider';
  elements.emptyProviderFollowUpAction.hidden = !summary?.follow_up_action_kind;
  elements.emptyProviderFollowUpAction.disabled = state.busy;
}

function renderTunnelWorkspace(workspace) {
  const tunnels = workspace?.tunnels ?? [];
  const currentTunnel = tunnels.find((tunnel) => tunnel.id === workspace?.current_tunnel_id) ?? tunnels[0] ?? null;
  const hasCurrentTunnel = Boolean(currentTunnel);

  elements.tunnelEmptyState.hidden = hasCurrentTunnel;
  elements.tunnelContextBar.hidden = !hasCurrentTunnel;
  elements.homeGrid.hidden = !hasCurrentTunnel;
  elements.servicesShell.hidden = !hasCurrentTunnel;

  if (!hasCurrentTunnel) {
    renderEmptyStateProviderGuidance();
    closeTunnelPicker();
    closeErrorDetailsDialog();
    return;
  }

  elements.currentTunnelName.textContent = currentTunnel.name;
  const tunnelState = currentTunnel.state ?? 'idle';
  elements.currentTunnelBadge.textContent = titleCase(tunnelState);
  elements.currentTunnelBadge.className = `status-pill ${escapeClassName(tunnelState)}`;
  elements.currentTunnelMeta.textContent = formatCurrentTunnelMeta(currentTunnel);
  const currentTunnelDetails = getCurrentTunnelDetails(currentTunnel);
  elements.homeProviderHint.textContent = formatHomeProviderHint(currentTunnelDetails);
  const currentTunnelUrl = formatCurrentTunnelUrl(currentTunnel);
  elements.currentTunnelUrl.hidden = !currentTunnelUrl;
  if (currentTunnelUrl) {
    elements.currentTunnelUrl.textContent = currentTunnelUrl;
  }
  const providerAvailabilitySummary = summarizeProviderAvailability(currentTunnelDetails);
  if (providerAvailabilitySummary) {
    renderProviderStatusSummary(providerAvailabilitySummary);
  } else {
    renderProviderStatusSummary(null);
  }
  renderHomeProviderActions(currentTunnelDetails);
  renderTunnelPicker(workspace, currentTunnel);
}

function toggleTunnelPicker() {
  if (state.tunnelPickerOpen) {
    closeTunnelPicker();
    return;
  }
  state.tunnelPickerOpen = true;
  elements.tunnelPickerPopover.hidden = false;
  elements.tunnelPickerTrigger.setAttribute('aria-expanded', 'true');
}

function closeTunnelPicker() {
  state.tunnelPickerOpen = false;
  if (elements.tunnelPickerPopover) {
    elements.tunnelPickerPopover.hidden = true;
  }
  if (elements.tunnelPickerTrigger) {
    elements.tunnelPickerTrigger.setAttribute('aria-expanded', 'false');
  }
}

function renderTunnelPicker(workspace, currentTunnel) {
  const tunnels = workspace?.tunnels ?? [];
  if (!elements.tunnelPickerShell || !elements.tunnelPickerList || !elements.tunnelPickerTrigger) {
    return;
  }

  elements.tunnelPickerShell.hidden = tunnels.length <= 1;
  elements.tunnelPickerTrigger.textContent = currentTunnel
    ? `Switch from ${currentTunnel.name}`
    : 'Switch Tunnel';
  elements.tunnelPickerList.innerHTML = '';

  tunnels.forEach((tunnel) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = tunnelPickerRowClass(tunnel, tunnel.id === workspace.current_tunnel_id);
    button.innerHTML = `
      <div class="picker-item-copy">
        <span class="picker-item-name">${escapeHtml(tunnel.name)}</span>
        <span class="picker-item-meta">${escapeHtml(formatTunnelOptionLabel(tunnel))}</span>
      </div>
      <span class="status-pill ${escapeClassName(tunnel.state ?? 'idle')}">${escapeHtml(titleCase(tunnel.state ?? 'idle'))}</span>
    `;
    button.addEventListener('click', () => {
      if (tunnel.id === workspace.current_tunnel_id) {
        closeTunnelPicker();
        return;
      }
      void withBusy(() => switchTunnel(tunnel.id));
    });
    elements.tunnelPickerList.appendChild(button);
  });
}

function populateTunnelFields(tunnel, recoveryTarget = null) {
  elements.tunnelName.value = tunnel?.name ?? '';
  elements.tunnelProvider.value = tunnel?.provider ?? 'cloudflared';
  elements.tunnelGatewayTargetUrl.value = tunnel?.gateway_target_url ?? 'http://127.0.0.1:48080';
  elements.tunnelAutoRestart.checked = Boolean(tunnel?.auto_restart ?? true);
  elements.tunnelCloudflaredTunnelToken.value = tunnel?.cloudflared_tunnel_token ?? '';
  elements.tunnelNgrokAuthtoken.value = tunnel?.ngrok_authtoken ?? '';
  elements.tunnelNgrokDomain.value = tunnel?.ngrok_domain ?? '';
  elements.tunnelAdvanced.open = shouldOpenTunnelAdvanced(tunnel, recoveryTarget);
  syncTunnelProviderFields();
}

function syncTunnelProviderFields() {
  const provider = elements.tunnelProvider.value || 'cloudflared';
  elements.tunnelCloudflaredFields.hidden = provider !== 'cloudflared';
  elements.tunnelNgrokFields.hidden = provider !== 'ngrok';
  if (provider === 'ngrok' && !String(elements.tunnelNgrokAuthtoken?.value ?? '').trim()) {
    elements.tunnelAdvanced.open = true;
  }
  renderTunnelDrawerProviderReadiness();
}

function renderTunnelDrawerProviderReadiness() {
  if (!elements.tunnelProviderReadinessTitle || !elements.tunnelProviderReadinessMessage) {
    return;
  }

  const provider = elements.tunnelProvider?.value || 'cloudflared';
  if (!state.providerAvailabilitySnapshot) {
    elements.tunnelProviderReadinessTitle.textContent = `${titleCase(provider)} Check Pending`;
    elements.tunnelProviderReadinessMessage.textContent = 'TunnelMux will verify whether this provider is installed before you start the tunnel.';
    elements.tunnelProviderReadinessBadge.textContent = 'Info';
    elements.tunnelProviderReadinessBadge.className = 'status-pill idle';
    if (elements.tunnelProviderInstallAction) {
      elements.tunnelProviderInstallAction.hidden = true;
      elements.tunnelProviderInstallAction.disabled = state.busy;
    }
    if (elements.tunnelProviderRecheckAction) {
      elements.tunnelProviderRecheckAction.hidden = true;
      elements.tunnelProviderRecheckAction.disabled = state.busy;
    }
    if (elements.saveAndStartTunnel) {
      elements.saveAndStartTunnel.disabled = state.busy;
      elements.saveAndStartTunnel.textContent = 'Save and Start';
    }
    return;
  }

  const readiness = summarizeDrawerProviderReadiness(
    collectTunnelProfile(),
    state.providerAvailabilitySnapshot,
  );
  elements.tunnelProviderReadinessTitle.textContent = readiness.title;
  elements.tunnelProviderReadinessMessage.textContent = readiness.message;
  elements.tunnelProviderReadinessBadge.textContent = titleCase(readiness.level ?? 'info');
  elements.tunnelProviderReadinessBadge.className = `status-pill ${escapeClassName(readiness.level ?? 'idle')}`;

  if (elements.tunnelProviderInstallAction) {
    elements.tunnelProviderInstallAction.hidden = !readiness.action_kind;
    elements.tunnelProviderInstallAction.textContent = readiness.action_label ?? 'Copy Install Command';
    elements.tunnelProviderInstallAction.disabled = state.busy;
  }

  if (elements.tunnelProviderRecheckAction) {
    elements.tunnelProviderRecheckAction.hidden = !readiness.follow_up_action_kind;
    elements.tunnelProviderRecheckAction.textContent = readiness.follow_up_action_label ?? 'Recheck Provider';
    elements.tunnelProviderRecheckAction.disabled = state.busy;
  }

  if (elements.saveAndStartTunnel) {
    elements.saveAndStartTunnel.disabled = state.busy || Boolean(readiness.start_disabled);
    elements.saveAndStartTunnel.textContent = readiness.start_label ?? 'Save and Start';
  }
}

async function copyTunnelProviderInstallCommand() {
  const readiness = summarizeDrawerProviderReadiness(
    elements.tunnelProvider?.value || 'cloudflared',
    state.providerAvailabilitySnapshot,
  );

  if (!readiness.action_payload) {
    renderStatus('No install command is available for this provider yet.', true);
    return;
  }

  await copyTextValue(
    readiness.action_payload,
    'Install command copied.',
    'Failed to copy install command',
  );
}

async function recheckTunnelDrawerProvider() {
  const readiness = summarizeDrawerProviderReadiness(
    elements.tunnelProvider?.value || 'cloudflared',
    state.providerAvailabilitySnapshot,
  );

  await runProviderUiAction(
    readiness.follow_up_action_kind,
    readiness.follow_up_action_payload,
    'drawer',
  );
}

async function refreshProviderAvailabilityOnForeground() {
  await refreshEmptyStateProviderAvailabilityOnForeground();
  await refreshCurrentTunnelProviderAvailabilityOnForeground();
  await refreshDrawerProviderAvailabilityOnForeground();
}

async function refreshEmptyStateProviderAvailabilityOnForeground() {
  const previousGuidance = summarizeEmptyStateProviderGuidance(state.providerAvailabilitySnapshot);
  if (!shouldPassiveEmptyStateProviderRefresh({
    hasCurrentTunnel: Boolean(getCurrentWorkspaceTunnel()),
    tunnelDrawerOpen: state.tunnelDrawerOpen,
    busy: state.busy || state.passiveProviderRefreshInFlight,
    visibilityState: document.visibilityState,
    providerAvailabilitySnapshot: state.providerAvailabilitySnapshot,
  })) {
    return;
  }

  state.passiveProviderRefreshInFlight = true;

  try {
    const snapshot = await refreshProviderAvailabilitySnapshot();
    if (!snapshot) {
      return;
    }

    await refreshProviderStatusSummary();
    const nextGuidance = summarizeEmptyStateProviderGuidance(snapshot);
    const statusUpdate = summarizePassiveEmptyStateProviderRefresh({
      previousGuidance,
      nextGuidance,
      nextProviderAvailabilitySnapshot: snapshot,
    });

    if (statusUpdate) {
      renderStatus(statusUpdate.message, statusUpdate.isError, statusUpdate.statusAction);
    }
  } finally {
    state.passiveProviderRefreshInFlight = false;
  }
}

async function refreshCurrentTunnelProviderAvailabilityOnForeground() {
  const previousTunnel = getCurrentTunnelDetails();
  if (!shouldPassiveCurrentTunnelProviderRefresh({
    currentTunnel: previousTunnel,
    tunnelDrawerOpen: state.tunnelDrawerOpen,
    busy: state.busy || state.passiveProviderRefreshInFlight,
    visibilityState: document.visibilityState,
  })) {
    return;
  }

  state.passiveProviderRefreshInFlight = true;

  try {
    const snapshot = await refreshProviderAvailabilitySnapshot();
    if (!snapshot) {
      return;
    }

    await refreshProviderStatusSummary();
    const nextTunnel = getCurrentTunnelDetails();
    const statusUpdate = summarizePassiveCurrentTunnelProviderRefresh({
      previousTunnel,
      nextTunnel,
    });

    if (statusUpdate) {
      renderStatus(statusUpdate.message, statusUpdate.isError, statusUpdate.statusAction);
    }
  } finally {
    state.passiveProviderRefreshInFlight = false;
  }
}

async function refreshDrawerProviderAvailabilityOnForeground() {
  const provider = elements.tunnelProvider?.value || 'cloudflared';
  if (!shouldPassiveDrawerProviderRefresh({
    tunnelDrawerOpen: state.tunnelDrawerOpen,
    busy: state.busy || state.passiveProviderRefreshInFlight,
    visibilityState: document.visibilityState,
    provider,
    providerAvailabilitySnapshot: state.providerAvailabilitySnapshot,
  })) {
    return;
  }

  const previousReadiness = summarizeDrawerProviderReadiness(provider, state.providerAvailabilitySnapshot);
  state.passiveProviderRefreshInFlight = true;

  try {
    const snapshot = await refreshProviderAvailabilitySnapshot();
    if (!snapshot) {
      return;
    }

    await refreshProviderStatusSummary();
    const nextReadiness = summarizeDrawerProviderReadiness(provider, snapshot);
    const statusUpdate = summarizePassiveDrawerProviderRefresh({
      provider,
      previousReadiness,
      nextReadiness,
    });

    if (statusUpdate) {
      renderStatus(statusUpdate.message, statusUpdate.isError);
    }
  } finally {
    state.passiveProviderRefreshInFlight = false;
  }
}

function getCurrentTunnelSettings() {
  const settings = state.settings;
  if (!settings) {
    return null;
  }
  return settings.tunnels.find((tunnel) => tunnel.id === settings.current_tunnel_id) ?? settings.tunnels[0] ?? null;
}

function nextTunnelId() {
  const existing = new Set((state.settings?.tunnels ?? []).map((tunnel) => tunnel.id));
  let index = 1;
  while (existing.has(`tunnel-${index}`)) {
    index += 1;
  }
  return `tunnel-${index}`;
}

function collectTunnelProfile() {
  const current = getCurrentTunnelSettings();
  const id = state.tunnelEditorMode === 'edit' && current ? current.id : nextTunnelId();
  const defaults = state.tunnelEditorMode === 'create'
    ? resolveCreateTunnelDefaults(state.providerAvailabilitySnapshot)
    : current ?? resolveCreateTunnelDefaults(state.providerAvailabilitySnapshot);
  return {
    id,
    name: elements.tunnelName.value.trim() || defaults.name,
    provider: elements.tunnelProvider.value || defaults.provider,
    gateway_target_url: elements.tunnelGatewayTargetUrl.value.trim() || defaults.gateway_target_url,
    auto_restart: elements.tunnelAutoRestart.checked ?? defaults.auto_restart,
    cloudflared_tunnel_token: elements.tunnelCloudflaredTunnelToken.value || null,
    ngrok_authtoken: elements.tunnelNgrokAuthtoken.value || null,
    ngrok_domain: elements.tunnelNgrokDomain.value || null,
  };
}

async function saveTunnel({ startNow }) {
  try {
    const profile = collectTunnelProfile();
    const workspace = await invoke('save_tunnel_profile', { profile });
    state.tunnelWorkspace = workspace;
    await loadSettings();
    renderTunnelWorkspace(workspace);
    if (startNow) {
      await ensureLocalDaemonAndRefresh();
      const startResult = await startTunnel();
      if (!startResult.ok) {
        if (startResult.recoveryTarget) {
          openTunnelDrawer({ mode: 'edit', recoveryTarget: startResult.recoveryTarget ?? null });
        }
        return;
      }
      closeTunnelDrawer();
      return;
    }
    closeTunnelDrawer();
    const statusAction = summarizeStartReadyStatusAction(getCurrentTunnelDetails());
    renderStatus(statusAction ? 'Tunnel saved. Start Tunnel is available.' : 'Tunnel saved.', false, statusAction);
  } catch (error) {
    const message = formatError(error);
    const failure = summarizeStartFailureRecovery({
      message,
      settings: state.settings,
      tunnel: collectTunnelProfile(),
    });
    if (failure.recoveryTarget) {
      renderStatus(message, true);
      requestAnimationFrame(() => focusTunnelDrawerPrimaryField({
        mode: state.tunnelEditorMode,
        recoveryTarget: failure.recoveryTarget ?? null,
      }));
      return;
    }
    renderStatus(`Failed to save tunnel: ${message}`, true);
  }
}

async function switchTunnel(nextTunnelId) {
  const workspace = await invoke('select_tunnel_profile', { id: nextTunnelId || '' });
  state.tunnelWorkspace = workspace;
  await loadSettings();
  renderTunnelWorkspace(workspace);
  closeTunnelPicker();
  await ensureLocalDaemonAndRefresh();
  const selectedTunnel = (workspace?.tunnels ?? []).find((tunnel) => tunnel.id === nextTunnelId)
    ?? (workspace?.tunnels ?? []).find((tunnel) => tunnel.id === workspace.current_tunnel_id);
  if (selectedTunnel) {
    renderStatus(`Switched to ${selectedTunnel.name}.`);
  }
}

async function deleteTunnelProfile() {
  const current = getCurrentTunnelSettings();
  if (!current) {
    return;
  }
  if ((state.settings?.tunnels?.length ?? 0) <= 1) {
    renderStatus('Keep at least one tunnel, or create another before deleting this one.', true);
    return;
  }
  const currentSummary = (state.tunnelWorkspace?.tunnels ?? []).find((tunnel) => tunnel.id === current.id);
  const serviceCount = Number(currentSummary?.route_count ?? 0);
  const stateLabel = titleCase(currentSummary?.state ?? 'idle');
  const confirmed = await requestConfirmation({
    title: `Delete ${current.name}?`,
    message: serviceCount > 0
      ? `State: ${stateLabel}\nServices removed from daemon: ${serviceCount}`
      : `State: ${stateLabel}`,
    confirmLabel: 'Delete Tunnel',
  });
  if (!confirmed) {
    return;
  }

  await withBusy(async () => {
    const workspace = await invoke('delete_tunnel_profile', { id: current.id });
    state.tunnelWorkspace = workspace;
    await loadSettings();
    renderTunnelWorkspace(workspace);
    closeTunnelDrawer();
    await ensureLocalDaemonAndRefresh();
    renderStatus(`Deleted tunnel ${current.name}.`);
  });
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
    const result = await invoke('save_settings', { settings: collectSettingsForm() });
    populateSettingsFields(result.settings);
    renderDaemonStatus(result.daemon_status);
    await refreshAll();
    closeSettingsDrawer();
  } catch (error) {
    renderStatus(`Failed to save settings: ${formatError(error)}`, true);
  }
}

async function waitForManagedDaemonBootstrap() {
  for (let attempt = 0; attempt < 48; attempt += 1) {
    const daemon = await invoke('daemon_connection_state');
    renderDaemonStatus(daemon);
    if (!daemon?.bootstrapping) {
      await refreshAll();
      return;
    }

    await new Promise((resolve) => setTimeout(resolve, 250));
  }

  renderDaemonStatus({
    connected: false,
    ownership: 'unavailable',
    message: 'Starting local TunnelMux is taking longer than expected. Retry the local daemon or check whether another app is already using this port.',
  });
}

async function ensureLocalDaemonAndRefresh() {
  try {
    const daemon = await invoke('ensure_local_daemon');
    renderDaemonStatus(daemon);
  } catch (error) {
    const formattedError = formatError(error);
    const daemonMessage = summarizeDaemonUnavailableMessage(formattedError);
    renderDaemonStatus({
      connected: false,
      ownership: 'unavailable',
      message: daemonMessage === formattedError
        ? `Could not start local TunnelMux: ${formattedError}`
        : daemonMessage,
    });
  }

  await refreshAll();
}

async function refreshAll() {
  await refreshTunnelWorkspace();
  await refreshDashboard();
  await refreshRoutes();
  await refreshProviderStatusSummary();
}

async function refreshProviderStatusSummary() {
  const currentTunnel = getCurrentTunnelDetails();
  const providerAvailabilitySummary = summarizeProviderAvailability(currentTunnel, undefined, state.providerAvailabilitySnapshot);
  if (providerAvailabilitySummary) {
    renderProviderStatusSummary(providerAvailabilitySummary);
    return;
  }

  try {
    const summary = await invoke('load_provider_status_summary');
    renderProviderStatusSummary(summary);
  } catch {
    renderProviderStatusSummary(null);
  }
}

function renderDaemonStatus(snapshot) {
  state.daemonBootstrapping = Boolean(snapshot?.bootstrapping);
  const ownership = snapshot?.ownership ?? 'unavailable';
  const connected = Boolean(snapshot?.connected);
  const message = summarizeDaemonUnavailableMessage(snapshot?.message ?? '');
  const statusAction = summarizeDaemonRecoveryAction(snapshot, state.settings);

  if (state.daemonBootstrapping) {
    renderStatus(message || 'Starting local TunnelMux…', false, null);
    return;
  }

  if (connected && ownership === 'managed') {
    renderStatus(message || 'Connected to a GUI-managed local TunnelMux daemon.', false, null);
    return;
  }

  if (connected && ownership === 'external') {
    renderStatus(message || 'Using an existing local TunnelMux daemon.', false, null);
    return;
  }

  renderStatus(message || 'Local TunnelMux is unavailable.', true, statusAction);
}

async function refreshDashboard() {
  try {
    const snapshot = await invoke('refresh_dashboard');
    renderDashboard(snapshot);
  } catch (error) {
    if (state.daemonBootstrapping) {
      return;
    }

    renderStatus(`Failed to refresh dashboard: ${formatError(error)}`, true);
  }
}

async function refreshRoutes() {
  try {
    const snapshot = await invoke('list_routes');
    renderRoutes(snapshot);
  } catch (error) {
    if (state.daemonBootstrapping) {
      return;
    }

    renderRoutes({ routes: [], message: `Failed to load services: ${formatError(error)}` });
  }
}

async function startTunnel() {
  try {
    const tunnel = getCurrentTunnelSettings();
    if (!tunnel) {
      renderStatus('Create a tunnel before starting it.', true);
      return {
        ok: false,
        recoveryTarget: null,
      };
    }

    const providerAvailabilitySummary = currentProviderAvailabilitySummary();
    if (providerAvailabilitySummary) {
      renderProviderStatusSummary(providerAvailabilitySummary);
      renderHomeProviderActions();
      renderStatus(providerAvailabilitySummary.message, true);
      return {
        ok: false,
        recoveryTarget: null,
      };
    }

    const snapshot = await invoke('start_tunnel', {
      input: {
        provider: tunnel.provider,
        target_url: tunnel.gateway_target_url,
        auto_restart: tunnel.auto_restart,
      },
    });
    renderDashboard(snapshot);
    await refreshTunnelWorkspace();
    const statusAction = summarizeStartSuccessAction({
      public_url: getCurrentTunnelDetails()?.public_base_url ?? snapshot?.tunnel?.public_base_url ?? '',
      enabled_services: state.routeCache.filter((route) => route.enabled).length,
      named_cloudflared: Boolean(getCurrentTunnelDetails()?.cloudflared_tunnel_token),
      tunnel_state: getCurrentTunnelDetails()?.state ?? snapshot?.tunnel?.state ?? 'running',
    });
    renderStatus(statusAction?.kind === 'add_service' ? 'Tunnel started. Add Service to keep going.' : statusAction?.kind === 'copy_public_url' ? 'Tunnel started. Copy URL to share.' : 'Tunnel started.', false, statusAction);
    return {
      ok: true,
      recoveryTarget: null,
    };
  } catch (error) {
    const message = formatError(error);
    await refreshProviderAvailabilitySnapshot();
    await refreshProviderStatusSummary();
    const failure = summarizeStartFailureRecovery({
      message,
      settings: state.settings,
      tunnel: getCurrentTunnelDetails() ?? getCurrentTunnelSettings(),
    });

    if (failure.preservesProviderRecovery) {
      const providerAvailabilitySummary = currentProviderAvailabilitySummary();
      if (providerAvailabilitySummary) {
        renderProviderStatusSummary(providerAvailabilitySummary);
        renderHomeProviderActions();
        renderStatus(providerAvailabilitySummary.message, true);
      } else {
        renderStatus(`Failed to start tunnel: ${message}`, true);
      }
      return {
        ok: false,
        recoveryTarget: null,
      };
    }

    renderStatus(`Failed to start tunnel: ${message}`, true, failure.statusAction);
    return {
      ok: false,
      recoveryTarget: failure.recoveryTarget,
    };
  }
}

async function stopTunnel() {
  try {
    const snapshot = await invoke('stop_tunnel');
    renderDashboard(snapshot);
    await refreshTunnelWorkspace();
    renderStatus('Tunnel stopped.');
  } catch (error) {
    renderStatus(`Failed to stop tunnel: ${formatError(error)}`, true);
  }
}

async function saveRoute() {
  try {
    const exposureMode = elements.serviceExposureMode.value;
    const previousEnabledServices = state.routeCache.filter((route) => route.enabled).length;
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
    const nextEnabledServices = Array.isArray(snapshot?.routes)
      ? snapshot.routes.filter((route) => route.enabled).length
      : previousEnabledServices;
    renderRoutes(snapshot);
    await refreshTunnelWorkspace();
    const startAction = previousEnabledServices === 0 && nextEnabledServices > 0
      ? summarizeStartReadyStatusAction(getCurrentTunnelDetails())
      : null;
    if (startAction) {
      renderStatus('Service saved. Start Tunnel to keep going.', false, startAction);
      resetRouteForm();
      closeServiceDrawer();
      return;
    }
    const shareAction = summarizeShareStatusAction({
      public_url: getCurrentTunnelDetails()?.public_base_url ?? '',
      enabled_services: nextEnabledServices,
    });
    renderStatus(summarizeRouteSaveStatus({
      tunnel_state: state.dashboardTunnelState,
      public_url: getCurrentTunnelDetails()?.public_base_url ?? '',
      previous_enabled_services: previousEnabledServices,
      next_enabled_services: nextEnabledServices,
      message: snapshot?.message,
    }), false, shareAction);
    resetRouteForm();
    closeServiceDrawer();
  } catch (error) {
    const failure = summarizeRouteSaveFailure({ message: formatError(error) });
    if (failure) {
      if (failure.openAdvanced) {
        elements.serviceAdvanced.open = true;
      }
      renderStatus(failure.message, true);
      requestAnimationFrame(() => focusServiceDrawerField(failure.recoveryTarget));
      return;
    }

    renderStatus(`Failed to save service: ${formatError(error)}`, true);
  }
}

async function deleteRoute(id) {
  const route = state.routeCache.find((item) => item.id === id);
  const confirmed = await requestConfirmation({
    title: `Delete ${id}?`,
    message: [
      route ? `Exposure: ${describeRouteExposure(route)}` : null,
      route?.upstream_url ? `Local URL: ${route.upstream_url}` : null,
    ].filter(Boolean).join('\n'),
    confirmLabel: 'Delete Service',
  });
  if (!confirmed) {
    return;
  }

  await withBusy(async () => {
    try {
      const snapshot = await invoke('delete_route', { id });
      renderRoutes(snapshot);
      await refreshTunnelWorkspace();
      renderStatus(snapshot.message ?? 'Service deleted.');
      if (state.editingOriginalId === id) {
        resetRouteForm();
        closeServiceDrawer();
      }
    } catch (error) {
      renderStatus(`Failed to delete service: ${formatError(error)}`, true);
    }
  });
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
    await refreshTunnelWorkspace();
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
  state.dashboardConnected = connected;
  state.dashboardTunnelState = tunnelState;
  const namedCloudflared =
    tunnel?.provider === 'cloudflared' &&
    Boolean(snapshot?.settings?.cloudflared_tunnel_token);
  const guidance = summarizeDashboardGuidance({
    connected,
    public_url: publicUrl,
    tunnel_state: tunnelState,
    enabled_services: enabledServices,
    named_cloudflared: namedCloudflared,
    message: snapshot?.message ?? null,
  });

  elements.publicUrl.textContent = publicUrl || (
    tunnelState === 'running' && namedCloudflared
      ? 'Managed in Cloudflare'
      : tunnelState === 'running'
        ? 'Waiting for public URL…'
        : 'Not running'
  );
  elements.dashboardConnected.textContent = connected ? 'Yes' : 'No';
  elements.dashboardProvider.textContent = tunnel?.provider ?? collectSettingsForm().default_provider ?? '—';
  elements.servicesEnabledCount.textContent = `${enabledServices} enabled`;
  const publicUrlActions = resolveDashboardPublicUrlActions({
    public_url: publicUrl,
    enabled_services: enabledServices,
    tunnel_state: tunnelState,
    named_cloudflared: namedCloudflared,
  });
  elements.copyPublicUrl.disabled = !publicUrlActions.show_copy_public_url;
  elements.openPublicUrl.disabled = !publicUrlActions.show_open_public_url;
  elements.manageProvider.disabled = !publicUrlActions.show_manage_provider;
  elements.stopTunnel.disabled = tunnelState !== 'running';
  elements.startTunnel.textContent = tunnelState === 'stopped' || tunnelState === 'error'
    ? 'Restart Tunnel'
    : 'Start Tunnel';
  elements.startTunnel.hidden = tunnelState === 'running';
  elements.copyPublicUrl.hidden = !publicUrlActions.show_copy_public_url;
  elements.openPublicUrl.hidden = !publicUrlActions.show_open_public_url;
  elements.manageProvider.hidden = !publicUrlActions.show_manage_provider;
  elements.stopTunnel.hidden = tunnelState !== 'running';
  renderHomeProviderActions();
  renderHeroAddServiceAction(summarizeZeroServiceHeroAction({
    connected,
    tunnel_state: tunnelState,
    enabled_services: enabledServices,
  }));

  elements.stateBadge.textContent = titleCase(tunnelState);
  elements.stateBadge.className = `status-pill ${escapeClassName(tunnelState)}`;

  const dashboardStatus = resolveDashboardStatus(snapshot);
  if (!connected) {
    elements.homePublicUrlMeta.textContent = guidance.home_public_url_meta;
    elements.dashboardMessage.textContent = guidance.dashboard_message;
    if (dashboardStatus) {
      renderStatus(
        dashboardStatus.message,
        dashboardStatus.isError,
        summarizeDaemonRecoveryAction(
          {
            connected: false,
            ownership: 'unavailable',
            message: snapshot?.message ?? null,
          },
          state.settings,
        ),
      );
    }
    return;
  }

  elements.homePublicUrlMeta.textContent = guidance.home_public_url_meta;
  elements.dashboardMessage.textContent = guidance.dashboard_message;
}

function renderHeroAddServiceAction(action) {
  if (!elements.heroAddService) {
    return;
  }

  elements.heroAddService.hidden = !action;
  elements.heroAddService.textContent = action?.label ?? 'Add Service';
  elements.heroAddService.disabled = state.busy;
}

function renderProviderStatusSummary(summary) {
  if (!summary) {
    elements.providerStatusCard.hidden = true;
    state.providerStatusAction = null;
    state.providerStatusActionPayload = null;
    state.providerStatusFollowUpAction = null;
    state.providerStatusFollowUpActionPayload = null;
    if (elements.providerStatusFollowUpAction) {
      elements.providerStatusFollowUpAction.hidden = true;
    }
    return;
  }

  elements.providerStatusCard.hidden = false;
  elements.providerStatusTitle.textContent = summary.title ?? 'Provider Status';
  elements.providerStatusMessage.textContent = summary.message ?? '';
  elements.providerStatusBadge.textContent = titleCase(summary.level ?? 'info');
  elements.providerStatusBadge.className = `status-pill ${escapeClassName(summary.level ?? 'idle')}`;
  state.providerStatusAction = summary.action_kind ?? null;
  state.providerStatusActionPayload = summary.action_payload ?? null;
  state.providerStatusFollowUpAction = summary.follow_up_action_kind ?? null;
  state.providerStatusFollowUpActionPayload = summary.follow_up_action_payload ?? null;
  elements.providerStatusAction.textContent = summary.action_label ?? 'Review';
  elements.providerStatusAction.hidden = !summary.action_kind;
  elements.providerStatusAction.disabled = state.busy;
  if (elements.providerStatusFollowUpAction) {
    elements.providerStatusFollowUpAction.textContent = summary.follow_up_action_label ?? 'Recheck Provider';
    elements.providerStatusFollowUpAction.hidden = !summary.follow_up_action_kind;
    elements.providerStatusFollowUpAction.disabled = state.busy;
  }
}

function renderRoutes(snapshot) {
  const nextRoutes = Array.isArray(snapshot?.routes) ? snapshot.routes : [];
  const viewState = classifyRoutesPanel(snapshot, state.routeCache.length);
  if (viewState.mode !== 'stale') {
    state.routeCache = nextRoutes;
  }

  elements.routesMessage.textContent = state.routeCache.length
    ? 'Services exposed through your current tunnel.'
    : 'Add a local service to route traffic somewhere useful.';
  elements.routesList.innerHTML = '';

  const enabled = state.routeCache.filter((route) => route.enabled).length;
  elements.servicesEnabledCount.textContent = `${enabled} enabled`;
  renderHeroAddServiceAction(summarizeZeroServiceHeroAction({
    connected: state.dashboardConnected,
    tunnel_state: state.dashboardTunnelState,
    enabled_services: enabled,
  }));
  elements.newRoute.hidden = false;
  elements.servicesNotice.hidden = !viewState.notice;
  if (viewState.notice) {
    elements.servicesNotice.textContent = viewState.notice;
  }

  if (viewState.mode === 'error') {
    elements.routesEmpty.hidden = true;
    elements.dashboardMessage.hidden = true;
    return;
  }

  if (!state.routeCache.length) {
    const currentTunnel = getCurrentTunnelDetails();
    const shouldPromptBeforeSharing = currentTunnel?.state === 'running';

    elements.routesEmptyTitle.textContent = 'No services yet.';
    elements.routesEmptyCopy.textContent = shouldPromptBeforeSharing
      ? 'Add a service before sharing this URL. It replaces the default welcome page.'
      : (snapshot?.message ?? 'Add a local service to route traffic somewhere useful.');
    elements.routesEmptyCopy.classList.remove('error');
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
    button.addEventListener('click', () => deleteRoute(button.dataset.routeId));
  });
}

function populateRouteForm(route) {
  state.editingOriginalId = route.id;
  elements.routeFormTitle.textContent = resolveRouteFormTitle({
    editing_route_id: route.id,
    route_count: currentRouteCount(),
  });
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
  elements.routeFormTitle.textContent = resolveRouteFormTitle({
    editing_route_id: null,
    route_count: currentRouteCount(),
  });
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

  await copyTextValue(url, 'Public URL copied.', 'Failed to copy URL');
}

async function copyTextValue(value, successMessage, failurePrefix) {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
      renderStatus(successMessage);
      return;
    }
    throw new Error('clipboard API unavailable');
  } catch (error) {
    renderStatus(`${failurePrefix}: ${formatError(error)}`, true);
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

async function handleEmptyProviderAction() {
  const summary = summarizeEmptyStateProviderGuidance(state.providerAvailabilitySnapshot);
  if (!summary) {
    return;
  }

  await runProviderUiAction(summary.action_kind, summary.action_payload, 'empty');
}

async function handleEmptyProviderFollowUpAction() {
  const summary = summarizeEmptyStateProviderGuidance(state.providerAvailabilitySnapshot);
  if (!summary) {
    return;
  }

  await runProviderUiAction(summary.follow_up_action_kind, summary.follow_up_action_payload, 'empty');
}

async function handleHomeProviderAction() {
  const actionState = summarizeHomeTunnelActions(getCurrentTunnelDetails(), undefined, state.providerAvailabilitySnapshot);
  await runProviderUiAction(actionState.action_kind, actionState.action_payload);
}

async function handleHomeProviderFollowUpAction() {
  const actionState = summarizeHomeTunnelActions(getCurrentTunnelDetails(), undefined, state.providerAvailabilitySnapshot);
  await runProviderUiAction(
    actionState.follow_up_action_kind,
    actionState.follow_up_action_payload,
    'home',
  );
}

async function handleProviderStatusAction() {
  await runProviderUiAction(state.providerStatusAction, state.providerStatusActionPayload);
}

async function handleProviderStatusFollowUpAction() {
  await runProviderUiAction(
    state.providerStatusFollowUpAction,
    state.providerStatusFollowUpActionPayload,
    'provider_status',
  );
}

async function runProviderUiAction(actionKind, actionPayload, actionSource = 'home') {
  switch (actionKind) {
    case 'open_cloudflare':
      openCloudflareDashboard();
      break;
    case 'open_cloudflare_docs':
      openCloudflareDocs();
      break;
    case 'edit_tunnel':
      openTunnelDrawer({ mode: 'edit', recoveryTarget: actionPayload ?? null });
      syncTunnelProviderFields();
      break;
    case 'use_installed_provider':
      if (!actionPayload) {
        renderStatus('No installed provider is available yet.', true);
        break;
      }
      openTunnelDrawer({ mode: 'edit', recoveryTarget: actionPayload === 'ngrok' ? 'ngrok_authtoken' : null });
      elements.tunnelProvider.value = actionPayload;
      syncTunnelProviderFields();
      break;
    case 'copy_install_command':
      if (!actionPayload) {
        renderStatus('No install command is available yet.', true);
        break;
      }
      await copyTextValue(
        actionPayload,
        'Install command copied.',
        'Failed to copy install command',
      );
      break;
    case 'edit_service':
      openServiceEditorForRoute(actionPayload);
      break;
    case 'review_services':
      highlightServicesPanel();
      break;
    case 'recheck_provider':
      await refreshCurrentProviderAvailability(actionPayload, actionSource);
      break;
    default:
      break;
  }
}

async function refreshCurrentProviderAvailability(providerName, source = 'home') {
  const previousEmptyStateGuidance = source === 'empty'
    ? summarizeEmptyStateProviderGuidance(state.providerAvailabilitySnapshot)
    : null;
  const snapshot = await refreshProviderAvailabilitySnapshot();
  await refreshProviderStatusSummary();

  if (!snapshot) {
    renderStatus('Failed to refresh provider availability.', true);
    return;
  }

  if (source === 'empty') {
    const guidance = summarizeEmptyStateProviderGuidance(snapshot);
    const statusUpdate = summarizePassiveEmptyStateProviderRefresh({
      previousGuidance: previousEmptyStateGuidance,
      nextGuidance: guidance,
      nextProviderAvailabilitySnapshot: snapshot,
    });
    if (statusUpdate) {
      renderStatus(statusUpdate.message, statusUpdate.isError, statusUpdate.statusAction);
      return;
    }

    if (guidance?.follow_up_action_kind) {
      renderStatus(`${titleCase(guidance.follow_up_action_payload ?? 'provider')} is still missing. Install it, then recheck again.`, true);
      return;
    }

    const statusAction = summarizeProviderRecheckFollowThrough({
      source,
      tunnel_state: 'offline',
    });
    renderStatus('Create Tunnel is available.', false, statusAction);
    return;
  }

  const resolvedProvider = providerName
    ?? elements.tunnelProvider?.value
    ?? getCurrentTunnelDetails()?.provider
    ?? 'provider';
  const readiness = summarizeDrawerProviderReadiness(resolvedProvider, snapshot);
  if (readiness.start_disabled) {
    renderStatus(`${titleCase(resolvedProvider)} is still missing. Install it, then recheck again.`, true);
    return;
  }

  const statusAction = summarizeProviderRecheckFollowThrough({
    source,
    tunnel_state: getCurrentTunnelDetails()?.state ?? state.dashboardTunnelState,
  });
  const statusMessage = statusAction?.kind === 'save_and_start_tunnel'
    ? `${titleCase(resolvedProvider)} is ready. Save and Start is available again.`
    : statusAction?.kind === 'start_tunnel'
      ? `${titleCase(resolvedProvider)} is ready. Start Tunnel is available again.`
      : `${titleCase(resolvedProvider)} is ready.`;

  renderStatus(statusMessage, false, statusAction);
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

function focusTunnelDrawerPrimaryField({ mode, recoveryTarget }) {
  const field = resolveTunnelRecoveryField(recoveryTarget) || (mode === 'create' ? elements.tunnelName : null);
  if (!field || field.disabled || typeof field.focus !== 'function') {
    return;
  }

  field.focus();
  if (typeof field.select === 'function') {
    field.select();
  }
}

function resolveTunnelRecoveryField(recoveryTarget) {
  switch (recoveryTarget) {
    case 'gateway_target_url':
      return elements.tunnelGatewayTargetUrl;
    case 'cloudflared_tunnel_token':
      return elements.tunnelCloudflaredTunnelToken;
    case 'ngrok_authtoken':
      return elements.tunnelNgrokAuthtoken;
    case 'ngrok_domain':
      return elements.tunnelNgrokDomain;
    default:
      return null;
  }
}

function renderStatus(message, isError = false, statusAction = null) {
  state.statusMessage = message ?? '';
  state.statusIsError = Boolean(isError);
  state.statusActionKind = statusAction?.kind ?? null;
  state.statusActionLabel = statusAction?.label ?? null;
  state.statusActionPayload = statusAction?.payload ?? null;
  elements.status.textContent = summarizeStatusMessage(message, isError);
  elements.status.classList.toggle('error', isError);
  if (elements.statusErrorDetails) {
    elements.statusErrorDetails.hidden = !shouldShowErrorDetailsAction({ isError });
  }
  if (elements.statusAction) {
    elements.statusAction.textContent = state.statusActionLabel ?? 'Retry';
    elements.statusAction.hidden = !state.statusActionKind;
    elements.statusAction.disabled = state.busy;
  }
}

async function handleStatusAction() {
  switch (state.statusActionKind) {
    case 'copy_public_url':
      await copyPublicUrl();
      break;
    case 'retry_local_daemon':
      await ensureLocalDaemonAndRefresh();
      break;
    case 'open_settings':
      openSettingsDrawer();
      break;
    case 'edit_tunnel':
      openTunnelDrawer({ mode: 'edit', recoveryTarget: state.statusActionPayload ?? null });
      syncTunnelProviderFields();
      break;
    case 'add_service':
      resetRouteForm();
      openServiceDrawer();
      break;
    case 'create_tunnel':
      openTunnelDrawer({ mode: 'create' });
      break;
    case 'start_tunnel':
      await startTunnel();
      break;
    case 'save_and_start_tunnel':
      await saveTunnel({ startNow: true });
      break;
    default:
      break;
  }
}

async function openErrorDetailsDialog() {
  if (elements.errorDetailsBackdrop) {
    elements.errorDetailsBackdrop.hidden = false;
  }
  if (elements.errorDetailsDialog) {
    elements.errorDetailsDialog.hidden = false;
  }
  if (elements.diagnosticsOverview) {
    elements.diagnosticsOverview.textContent = state.statusMessage || 'Error details';
    elements.diagnosticsOverview.classList.add('error');
  }
  await refreshDiagnosticsWorkspace({ manual: false });
}

function closeErrorDetailsDialog() {
  if (elements.errorDetailsBackdrop) {
    elements.errorDetailsBackdrop.hidden = true;
  }
  if (elements.errorDetailsDialog) {
    elements.errorDetailsDialog.hidden = true;
  }
}

async function requestConfirmation({ title, message, confirmLabel }) {
  elements.confirmTitle.textContent = title;
  elements.confirmMessage.textContent = message || 'Please confirm this action.';
  elements.confirmConfirm.textContent = confirmLabel || 'Confirm';
  elements.confirmBackdrop.hidden = false;
  elements.confirmDialog.hidden = false;

  return new Promise((resolve) => {
    state.confirmResolver = resolve;
  });
}

function closeConfirmDialog(confirmed) {
  if (!state.confirmResolver) {
    return;
  }

  const resolve = state.confirmResolver;
  state.confirmResolver = null;
  elements.confirmBackdrop.hidden = true;
  elements.confirmDialog.hidden = true;
  resolve(Boolean(confirmed));
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
