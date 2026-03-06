window.addEventListener('DOMContentLoaded', () => {
  const status = document.getElementById('app-status');
  if (!status) {
    return;
  }

  const tauriAvailable = typeof window.__TAURI__ !== 'undefined';
  status.textContent = tauriAvailable
    ? 'Tauri shell loaded. Settings and control commands are the next milestone.'
    : 'Preview shell loaded outside Tauri.';
});
