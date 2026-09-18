# tunnelmux-gui

`tunnelmux-gui` is the Tauri-based desktop GUI for TunnelMux.

## Scope

Current GUI supports:
- local daemon connection settings (`base_url` + optional token)
- tunnel defaults such as provider, gateway target URL, and restart behavior
- a single-page shell for public URL, tunnel state, start/stop, and the service list
- a side drawer for service-centric create/update/delete flows
- on-demand troubleshooting for runtime summary, upstream health, and recent provider logs

The GUI **hosts the daemon inside its own process**. It links `tunnelmuxd` as a
library and runs it on the Tauri async runtime, so nothing is spawned and there
is exactly one owner of the control port, the state files, the api token, and the
provider processes.

If a daemon already answers on the configured address, the GUI adopts it and
never stops it. That is the path for a headless `tunnelmuxd` you start yourself
for development or advanced workflows: the GUI connects to it, and leaves it
running on exit.

Quitting the app stops the embedded daemon and the tunnels it owns. Closing the
window only hides it to the tray.

## Local Run

```bash
cargo run -p tunnelmux-gui
```

To run a headless daemon alongside — for the CLI, or for scripted checks — give
it its own port and data files so it cannot collide with the embedded one:

```bash
tunnelmuxd --listen 127.0.0.1:4766 --gateway-listen 127.0.0.1:18081 \
  --data-file /tmp/tmuxd-dev/state.json \
  --config-file /tmp/tmuxd-dev/config.json \
  --provider-log-file /tmp/tmuxd-dev/provider.log
```

Then point the GUI's `base_url` at `http://127.0.0.1:4766` to drive it, or use
`tunnelmux-cli --server 127.0.0.1:4766`.

Troubleshooting remains intentionally secondary. Most users should be able to start a tunnel, copy a URL, and manage services without leaving the main page.

## Native Bundles

GitHub Releases now publish native GUI installer assets in addition to raw archives:
- macOS: `.dmg`
- Windows: `.msi`
- Linux: `.deb`

These first-release installers are unsigned by default, so platform trust warnings may still appear. Maintainers can opt into the signed macOS/Windows release path described in `docs/RELEASING.md`, but public installers may remain unsigned until those CI toggles are enabled.
