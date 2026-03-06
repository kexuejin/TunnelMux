const isTauri = typeof window.__TAURI__ !== 'undefined' && window.__TAURI__.core;
const invoke = (command, payload = {}) => {
  if (!isTauri) {
    return Promise.reject(new Error('Tauri bridge is unavailable in preview mode.'));
  }
  return window.__TAURI__.core.invoke(command, payload);
};

const elements = {};
const state = {
  busy: false,
  routeCache: [],
  editingOriginalId: null,
  activeWorkspace: 'operations',
  diagnostics: {
    logLines: 100,
    summary: null,
    upstreams: [],
    logTail: null,
    summaryUpdatedAt: null,
    upstreamsUpdatedAt: null,
    logsUpdatedAt: null,
    intervals: {
      summary: null,
      logs: null,
    },
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
  setActiveWorkspace('operations');

  if (!isTauri) {
    renderStatus('Preview shell loaded outside Tauri. Open the desktop app to enable commands.', true);
    renderRoutes({ routes: [], message: 'Routes preview unavailable outside Tauri.' });
    renderDiagnosticsOverview('Diagnostics preview unavailable outside Tauri.', true);
    renderDiagnosticsSummaryMeta('Preview mode only.', true);
    renderUpstreamsMeta('Preview mode only.', true);
    renderLogsMeta('Preview mode only.', true);
    renderRecentLogs({ requested_lines: state.diagnostics.logLines, lines: [] });
    return;
  }

  await loadSettings();
  await refreshAll();
});

function bindElements() {
  elements.status = document.getElementById('app-status');
  elements.workspaceTabs = [...document.querySelectorAll('[data-workspace-tab]')];
  elements.workspaces = [...document.querySelectorAll('[data-workspace]')];

  elements.message = document.getElementById('dashboard-message');
  elements.connected = document.getElementById('dashboard-connected');
  elements.state = document.getElementById('dashboard-state');
  elements.provider = document.getElementById('dashboard-provider');
  elements.publicUrl = document.getElementById('dashboard-public-url');
  elements.targetUrl = document.getElementById('dashboard-target-url');
  elements.baseUrl = document.getElementById('settings-base-url');
  elements.token = document.getElementById('settings-token');
  elements.saveSettings = document.getElementById('save-settings');
  elements.refreshDashboard = document.getElementById('refresh-dashboard');
  elements.startProvider = document.getElementById('start-provider');
  elements.startTargetUrl = document.getElementById('start-target-url');
  elements.startAutoRestart = document.getElementById('start-auto-restart');
  elements.startTunnel = document.getElementById('start-tunnel');
  elements.stopTunnel = document.getElementById('stop-tunnel');

  elements.routesMessage = document.getElementById('routes-message');
  elements.routesEmpty = document.getElementById('routes-empty');
  elements.routesList = document.getElementById('routes-list');
  elements.newRoute = document.getElementById('new-route');
  elements.cancelRouteEdit = document.getElementById('cancel-route-edit');
  elements.routeFormTitle = document.getElementById('route-form-title');
  elements.routeId = document.getElementById('route-id');
  elements.routeMatchHost = document.getElementById('route-match-host');
  elements.routeMatchPathPrefix = document.getElementById('route-match-path-prefix');
  elements.routeStripPathPrefix = document.getElementById('route-strip-path-prefix');
  elements.routeUpstreamUrl = document.getElementById('route-upstream-url');
  elements.routeFallbackUpstreamUrl = document.getElementById('route-fallback-upstream-url');
  elements.routeHealthCheckPath = document.getElementById('route-health-check-path');
  elements.routeEnabled = document.getElementById('route-enabled');
  elements.saveRoute = document.getElementById('save-route');

  elements.refreshDiagnostics = document.getElementById('refresh-diagnostics');
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
  elements.refreshLogs = document.getElementById('refresh-logs');
  elements.clearLogs = document.getElementById('clear-logs');
  elements.recentLogs = document.getElementById('recent-logs');
}

function bindEvents() {
  elements.workspaceTabs.forEach((button) => {
    button.addEventListener('click', () => setActiveWorkspace(button.dataset.workspaceTab));
  });

  elements.saveSettings?.addEventListener('click', () => withBusy(saveSettings));
  elements.refreshDashboard?.addEventListener('click', () => withBusy(refreshAll));
  elements.startTunnel?.addEventListener('click', () => withBusy(startTunnel));
  elements.stopTunnel?.addEventListener('click', () => withBusy(stopTunnel));
  elements.newRoute?.addEventListener('click', () => resetRouteForm());
  elements.cancelRouteEdit?.addEventListener('click', () => resetRouteForm());
  elements.saveRoute?.addEventListener('click', () => withBusy(saveRoute));

  elements.refreshDiagnostics?.addEventListener('click', () => withBusy(() => refreshDiagnosticsWorkspace({ manual: true })));
  elements.refreshLogs?.addEventListener('click', () => withBusy(() => refreshRecentLogs({ manual: true })));
  elements.clearLogs?.addEventListener('click', clearDisplayedLogs);
  elements.logLinesSelect?.addEventListener('change', () => {
    state.diagnostics.logLines = Number(elements.logLinesSelect.value) || 100;
    void refreshRecentLogs({ manual: false });
  });
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
  const staticControls = [
    elements.saveSettings,
    elements.refreshDashboard,
    elements.startTunnel,
    elements.stopTunnel,
    elements.newRoute,
    elements.cancelRouteEdit,
    elements.saveRoute,
    elements.refreshDiagnostics,
    elements.refreshLogs,
    elements.clearLogs,
    elements.logLinesSelect,
  ];

  staticControls.filter(Boolean).forEach((element) => {
    element.disabled = nextBusy;
  });

  document.querySelectorAll('[data-route-action]').forEach((button) => {
    button.disabled = nextBusy;
  });
}

function setActiveWorkspace(name) {
  state.activeWorkspace = name;
  elements.workspaceTabs.forEach((button) => {
    const isActive = button.dataset.workspaceTab === name;
    button.classList.toggle('is-active', isActive);
    button.setAttribute('aria-pressed', String(isActive));
  });

  elements.workspaces.forEach((section) => {
    const isActive = section.dataset.workspace === name;
    section.hidden = !isActive;
    section.classList.toggle('is-active', isActive);
  });

  if (name === 'diagnostics' && isTauri) {
    renderDiagnosticsOverview('Diagnostics polling is active while this workspace is open.');
    startDiagnosticsPolling();
  } else {
    stopDiagnosticsPolling();
  }
}

async function loadSettings() {
  try {
    const settings = await invoke('load_settings');
    elements.baseUrl.value = settings.base_url ?? '';
    elements.token.value = settings.token ?? '';
  } catch (error) {
    renderStatus(`Failed to load settings: ${formatError(error)}`, true);
  }
}

async function saveSettings() {
  try {
    const settings = await invoke('save_settings', {
      settings: {
        base_url: elements.baseUrl.value,
        token: elements.token.value || null,
      },
    });
    elements.baseUrl.value = settings.base_url ?? '';
    elements.token.value = settings.token ?? '';
    renderStatus('Settings saved. Refreshing dashboard and routes…');
    await refreshAll();
    if (state.activeWorkspace === 'diagnostics') {
      await refreshDiagnosticsWorkspace({ manual: false });
    }
  } catch (error) {
    renderStatus(`Failed to save settings: ${formatError(error)}`, true);
  }
}

async function refreshAll() {
  await refreshDashboard();
  await refreshRoutes();
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
    renderRoutes({ routes: [], message: `Failed to load routes: ${formatError(error)}` });
  }
}

async function startTunnel() {
  try {
    const snapshot = await invoke('start_tunnel', {
      input: {
        provider: elements.startProvider.value,
        target_url: elements.startTargetUrl.value,
        auto_restart: elements.startAutoRestart.checked,
      },
    });
    renderDashboard(snapshot);
    renderStatus('Tunnel started.');
    if (state.activeWorkspace === 'diagnostics') {
      void refreshDiagnosticsWorkspace({ manual: false });
    }
  } catch (error) {
    renderStatus(`Failed to start tunnel: ${formatError(error)}`, true);
  }
}

async function stopTunnel() {
  try {
    const snapshot = await invoke('stop_tunnel');
    renderDashboard(snapshot);
    renderStatus('Tunnel stopped.');
    if (state.activeWorkspace === 'diagnostics') {
      void refreshDiagnosticsWorkspace({ manual: false });
    }
  } catch (error) {
    renderStatus(`Failed to stop tunnel: ${formatError(error)}`, true);
  }
}

async function saveRoute() {
  try {
    const snapshot = await invoke('save_route', {
      form: {
        original_id: state.editingOriginalId,
        id: elements.routeId.value.trim(),
        match_host: elements.routeMatchHost.value,
        match_path_prefix: elements.routeMatchPathPrefix.value,
        strip_path_prefix: elements.routeStripPathPrefix.value,
        upstream_url: elements.routeUpstreamUrl.value,
        fallback_upstream_url: elements.routeFallbackUpstreamUrl.value,
        health_check_path: elements.routeHealthCheckPath.value,
        enabled: elements.routeEnabled.checked,
      },
    });
    renderRoutes(snapshot);
    renderStatus(snapshot.message ?? 'Route saved.');
    resetRouteForm();
    if (state.activeWorkspace === 'diagnostics') {
      void refreshDiagnosticsWorkspace({ manual: false });
    }
  } catch (error) {
    renderStatus(`Failed to save route: ${formatError(error)}`, true);
  }
}

async function deleteRoute(id) {
  if (!window.confirm(`Delete route '${id}'?`)) {
    return;
  }

  try {
    const snapshot = await invoke('delete_route', { id });
    renderRoutes(snapshot);
    renderStatus(snapshot.message ?? 'Route deleted.');
    if (state.editingOriginalId === id) {
      resetRouteForm();
    }
    if (state.activeWorkspace === 'diagnostics') {
      void refreshDiagnosticsWorkspace({ manual: false });
    }
  } catch (error) {
    renderStatus(`Failed to delete route: ${formatError(error)}`, true);
  }
}

function renderDashboard(snapshot) {
  if (!snapshot) {
    return;
  }

  const tunnel = snapshot.tunnel ?? {};
  elements.connected.textContent = snapshot.connected ? 'Yes' : 'No';
  elements.state.textContent = tunnel.state ?? 'unknown';
  elements.provider.textContent = tunnel.provider ?? '—';
  elements.publicUrl.textContent = tunnel.public_base_url ?? '—';
  elements.targetUrl.textContent = tunnel.target_url ?? '—';
  elements.message.textContent = snapshot.message ?? (snapshot.connected ? 'Daemon reachable.' : 'Daemon unavailable.');
  elements.message.classList.toggle('muted', !snapshot.message);
  renderStatus(
    snapshot.connected ? 'Dashboard refreshed.' : `Dashboard updated: ${snapshot.message ?? 'daemon unavailable'}`,
    !snapshot.connected,
  );
}

function renderRoutes(snapshot) {
  state.routeCache = snapshot?.routes ?? [];
  elements.routesMessage.textContent = snapshot?.message ?? 'Routes loaded from daemon.';
  elements.routesList.innerHTML = '';

  if (!state.routeCache.length) {
    elements.routesEmpty.hidden = false;
    return;
  }

  elements.routesEmpty.hidden = true;

  for (const route of state.routeCache) {
    const item = document.createElement('article');
    item.className = 'route-card';
    item.innerHTML = `
      <div class="route-card-header">
        <div>
          <h3>${escapeHtml(route.id)}</h3>
          <p class="route-match">${escapeHtml(route.display_match ?? '*/')}</p>
        </div>
        <span class="route-badge ${route.enabled ? 'enabled' : 'disabled'}">${route.enabled ? 'enabled' : 'disabled'}</span>
      </div>
      <dl class="route-meta">
        <div><dt>Upstream</dt><dd>${escapeHtml(route.upstream_url)}</dd></div>
        <div><dt>Fallback</dt><dd>${escapeHtml(route.fallback_upstream_url ?? '—')}</dd></div>
        <div><dt>Health</dt><dd>${escapeHtml(route.health_check_path ?? '—')}</dd></div>
      </dl>
      <div class="actions compact-actions">
        <button type="button" class="secondary" data-route-action="edit" data-route-id="${escapeAttribute(route.id)}">Edit</button>
        <button type="button" data-route-action="delete" data-route-id="${escapeAttribute(route.id)}">Delete</button>
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
      failed ? 'Diagnostics refreshed with some panel errors.' : 'Diagnostics refreshed.',
      failed,
    );
  }
}

function startDiagnosticsPolling() {
  stopDiagnosticsPolling();
  void refreshDiagnosticsWorkspace({ manual: false });
  state.diagnostics.intervals.summary = window.setInterval(() => {
    void refreshDiagnosticsSummary();
    void refreshUpstreamsHealth();
  }, 5000);
  state.diagnostics.intervals.logs = window.setInterval(() => {
    void refreshRecentLogs({ manual: false });
  }, 3000);
}

function stopDiagnosticsPolling() {
  if (state.diagnostics.intervals.summary) {
    window.clearInterval(state.diagnostics.intervals.summary);
    state.diagnostics.intervals.summary = null;
  }
  if (state.diagnostics.intervals.logs) {
    window.clearInterval(state.diagnostics.intervals.logs);
    state.diagnostics.intervals.logs = null;
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
    renderDiagnosticsOverview('Diagnostics polling is active while this workspace is open.');
    return true;
  } catch (error) {
    renderDiagnosticsSummaryMeta(`Failed to load runtime summary: ${formatError(error)}`, true);
    renderDiagnosticsOverview('Diagnostics is partially unavailable. Check panel errors for details.', true);
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
  renderLogsMeta('Local log display cleared. Auto-refresh will repopulate on the next poll.');
}

function renderDiagnosticsOverview(message, isError = false) {
  if (!elements.diagnosticsOverview) {
    return;
  }
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
          <p class="route-match">${escapeHtml(upstream.health_check_path ?? '/')}</p>
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
        setActiveWorkspace('routes');
      }
    });
  });

  document.querySelectorAll('[data-route-action="delete"]').forEach((button) => {
    button.addEventListener('click', () => withBusy(() => deleteRoute(button.dataset.routeId)));
  });
}

function populateRouteForm(route) {
  state.editingOriginalId = route.id;
  elements.routeFormTitle.textContent = `Edit Route: ${route.id}`;
  elements.routeId.value = route.id;
  elements.routeId.disabled = true;
  elements.routeMatchHost.value = route.match_host ?? '';
  elements.routeMatchPathPrefix.value = route.match_path_prefix ?? '';
  elements.routeStripPathPrefix.value = route.strip_path_prefix ?? '';
  elements.routeUpstreamUrl.value = route.upstream_url ?? '';
  elements.routeFallbackUpstreamUrl.value = route.fallback_upstream_url ?? '';
  elements.routeHealthCheckPath.value = route.health_check_path ?? '';
  elements.routeEnabled.checked = Boolean(route.enabled);
  elements.saveRoute.textContent = 'Update Route';
}

function resetRouteForm() {
  state.editingOriginalId = null;
  elements.routeFormTitle.textContent = 'Create Route';
  elements.routeId.disabled = false;
  elements.routeId.value = '';
  elements.routeMatchHost.value = '';
  elements.routeMatchPathPrefix.value = '/';
  elements.routeStripPathPrefix.value = '';
  elements.routeUpstreamUrl.value = '';
  elements.routeFallbackUpstreamUrl.value = '';
  elements.routeHealthCheckPath.value = '';
  elements.routeEnabled.checked = true;
  elements.saveRoute.textContent = 'Save Route';
}

function renderStatus(message, isError = false) {
  if (!elements.status) {
    return;
  }
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
