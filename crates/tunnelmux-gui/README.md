# tunnelmux-gui

`tunnelmux-gui` is the Tauri-based desktop control console for TunnelMux.

## Scope

Current GUI MVP supports:
- local daemon connection settings (`base_url` + optional token)
- dashboard refresh and tunnel status display
- tunnel `start` / `stop`
- route list / create / update / delete

The GUI connects to an already-running `tunnelmuxd` and does not launch the daemon itself.

## Local Run

```bash
cargo run -p tunnelmux-gui
```

Start `tunnelmuxd` first, then point the GUI at the daemon URL if you changed the default.
