const isTauri = typeof window.__TAURI__ !== 'undefined' && window.__TAURI__.core;
const invoke = (command, payload = {}) => {
  if (!isTauri) {
    return Promise.reject(new Error('Tauri bridge is unavailable in preview mode.'));
  }
  return window.__TAURI__.core.invoke(command, payload);
};

const elements = {};
let busy = false;

window.addEventListener('DOMContentLoaded', async () => {
  bindElements();
  bindEvents();

  if (!isTauri) {
    renderStatus('Preview shell loaded outside Tauri. Open the desktop app to enable commands.', true);
    return;
  }

  await loadSettings();
  await refreshDashboard();
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
}

function bindEvents() {
  elements.saveSettings?.addEventListener('click', () => withBusy(saveSettings));
  elements.refreshDashboard?.addEventListener('click', () => withBusy(refreshDashboard));
  elements.startTunnel?.addEventListener('click', () => withBusy(startTunnel));
  elements.stopTunnel?.addEventListener('click', () => withBusy(stopTunnel));
}

async function withBusy(fn) {
  if (busy) {
    return;
  }
  busy = true;
  setBusyState(true);
  try {
    await fn();
  } finally {
    busy = false;
    setBusyState(false);
  }
}

function setBusyState(nextBusy) {
  for (const id of ['saveSettings', 'refreshDashboard', 'startTunnel', 'stopTunnel']) {
    if (elements[id]) {
      elements[id].disabled = nextBusy;
    }
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
    renderStatus('Settings saved. Refreshing dashboard…');
    await refreshDashboard();
  } catch (error) {
    renderStatus(`Failed to save settings: ${formatError(error)}`, true);
  }
}

async function refreshDashboard() {
  try {
    const snapshot = await invoke('refresh_dashboard');
    renderDashboard(snapshot);
  } catch (error) {
    renderStatus(`Failed to refresh dashboard: ${formatError(error)}`, true);
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
