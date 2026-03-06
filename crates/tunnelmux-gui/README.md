# tunnelmux-gui

`tunnelmux-gui` is the Tauri-based desktop GUI for TunnelMux.

## Scope

Current GUI supports:
- local daemon connection settings (`base_url` + optional token)
- tunnel defaults such as provider, gateway target URL, and restart behavior
- a `Home` surface for public URL, tunnel state, and start/stop
- a `Services` surface for service-centric create/update/delete flows
- on-demand troubleshooting for runtime summary, upstream health, and recent provider logs

The GUI connects to an already-running `tunnelmuxd` and does not launch the daemon itself.

## Local Run

```bash
cargo run -p tunnelmux-gui
```

Start `tunnelmuxd` first, then point the GUI at the daemon URL if you changed the default.

Troubleshooting remains intentionally secondary. Most users should be able to start a tunnel, copy a URL, and manage services without opening the diagnostics surface.

## Native Bundles

GitHub Releases now publish native GUI installer assets in addition to raw archives:
- macOS: `.dmg`
- Windows: `.msi`
- Linux: `.deb`

These first-release installers are unsigned by default, so platform trust warnings may still appear. Maintainers can opt into the signed macOS/Windows release path described in `docs/RELEASING.md`, but public installers may remain unsigned until those CI toggles are enabled.
