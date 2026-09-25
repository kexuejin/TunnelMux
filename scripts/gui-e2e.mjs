#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import process from 'node:process';

const root = new URL('../', import.meta.url);
const html = readFileSync(new URL('crates/tunnelmux-gui/ui/index.html', root), 'utf8');
const app = readFileSync(new URL('crates/tunnelmux-gui/ui/app.js', root), 'utf8');
const tauri = JSON.parse(readFileSync(new URL('crates/tunnelmux-gui/tauri.conf.json', root), 'utf8'));

const requiredMarkup = [
  'id="app-status"',
  'role="status"',
  'aria-live="polite"',
  'role="dialog"',
  'aria-modal="true"',
  'data-view="overview"',
  'data-view="services"',
];
for (const marker of requiredMarkup) {
  if (!html.includes(marker)) throw new Error(`GUI contract missing ${marker}`);
}
if (tauri.app?.security?.csp == null) throw new Error('Tauri CSP is disabled');
if (!app.includes('startLiveRefresh')) throw new Error('GUI live refresh wiring is missing');
if (app.includes('eval(') || app.includes('new Function(')) {
  throw new Error('GUI contains a dynamic-code sink');
}

const driverUrl = process.env.TUNNELMUX_WEBDRIVER_URL;
if (driverUrl) {
  const response = await fetch(`${driverUrl.replace(/\/$/, '')}/status`);
  if (!response.ok) throw new Error(`WebDriver status failed: ${response.status}`);
  console.log('WebDriver endpoint is reachable; run the platform-specific Tauri scenario next.');
} else {
  console.log('GUI contract E2E passed (set TUNNELMUX_WEBDRIVER_URL for a WebDriver smoke run).');
}
