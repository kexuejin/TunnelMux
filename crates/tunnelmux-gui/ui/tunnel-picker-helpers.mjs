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
