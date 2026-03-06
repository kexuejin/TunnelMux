# tunnelmux-gui

`tunnelmux-gui` is the Tauri-based desktop GUI for TunnelMux.

## Scope

Current GUI supports:
- local daemon connection settings (`base_url` + optional token)
- tunnel defaults such as provider, gateway target URL, and restart behavior
- a single-page shell for public URL, tunnel state, start/stop, and the service list
- a side drawer for service-centric create/update/delete flows
- on-demand troubleshooting for runtime summary, upstream health, and recent provider logs

The GUI prefers to auto-start a local `tunnelmuxd` when no daemon is reachable. If it finds an existing daemon, it connects to that daemon instead of replacing it.

## Local Run

```bash
cargo run -p tunnelmux-gui
```

You can still start `tunnelmuxd` yourself for development or advanced workflows. If you do, the GUI will connect to it and will not stop it on exit.

Troubleshooting remains intentionally secondary. Most users should be able to start a tunnel, copy a URL, and manage services without leaving the main page.

## Native Bundles

GitHub Releases now publish native GUI installer assets in addition to raw archives:
- macOS: `.dmg`
- Windows: `.msi`
- Linux: `.deb`

These first-release installers are unsigned by default, so platform trust warnings may still appear. Maintainers can opt into the signed macOS/Windows release path described in `docs/RELEASING.md`, but public installers may remain unsigned until those CI toggles are enabled.
