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
