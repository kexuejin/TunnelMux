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

export function tunnelPickerRowClass(tunnel, selected) {
  const state = tunnel?.state ?? 'idle';
  return `tunnel-picker-item${selected ? ' selected' : ''} ${state}`.trim();
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
