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
};

window.addEventListener('DOMContentLoaded', async () => {
  bindElements();
  bindEvents();

  if (!isTauri) {
    renderStatus('Preview shell loaded outside Tauri. Open the desktop app to enable commands.', true);
    renderRoutes({ routes: [], message: 'Routes preview unavailable outside Tauri.' });
    return;
  }

  resetRouteForm();
  await loadSettings();
  await refreshAll();
});

function bindElements() {
  elements.status = document.getElementById('app-status');
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
}

function bindEvents() {
  elements.saveSettings?.addEventListener('click', () => withBusy(saveSettings));
  elements.refreshDashboard?.addEventListener('click', () => withBusy(refreshAll));
  elements.startTunnel?.addEventListener('click', () => withBusy(startTunnel));
  elements.stopTunnel?.addEventListener('click', () => withBusy(stopTunnel));
  elements.newRoute?.addEventListener('click', () => resetRouteForm());
  elements.cancelRouteEdit?.addEventListener('click', () => resetRouteForm());
  elements.saveRoute?.addEventListener('click', () => withBusy(saveRoute));
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
  const staticButtons = [
    elements.saveSettings,
    elements.refreshDashboard,
    elements.startTunnel,
    elements.stopTunnel,
    elements.newRoute,
    elements.cancelRouteEdit,
    elements.saveRoute,
  ];
  staticButtons.filter(Boolean).forEach((button) => {
    button.disabled = nextBusy;
  });

  document.querySelectorAll('[data-route-action]').forEach((button) => {
    button.disabled = nextBusy;
  });
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
    renderStatus('Tunnel start command completed.');
  } catch (error) {
    renderStatus(`Failed to start tunnel: ${formatError(error)}`, true);
  }
}

async function stopTunnel() {
  try {
    const snapshot = await invoke('stop_tunnel');
    renderDashboard(snapshot);
    renderStatus('Tunnel stop command completed.');
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
  renderStatus(snapshot.connected ? 'Dashboard refreshed.' : `Dashboard updated: ${snapshot.message ?? 'daemon unavailable'}`, !snapshot.connected);
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

function bindRouteActionButtons() {
  document.querySelectorAll('[data-route-action="edit"]').forEach((button) => {
    button.addEventListener('click', () => {
      const route = state.routeCache.find((item) => item.id === button.dataset.routeId);
      if (route) {
        populateRouteForm(route);
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
