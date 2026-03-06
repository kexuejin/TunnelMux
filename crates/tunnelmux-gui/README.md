# tunnelmux-gui

`tunnelmux-gui` is the Tauri-based desktop control console for TunnelMux.

## Scope

Current GUI supports:
- local daemon connection settings (`base_url` + optional token)
- operations workspace for dashboard refresh and tunnel status display
- tunnel `start` / `stop`
- routes workspace for route list / create / update / delete
- diagnostics workspace for runtime summary, upstream health, and recent provider logs

The GUI connects to an already-running `tunnelmuxd` and does not launch the daemon itself.

## Local Run

```bash
cargo run -p tunnelmux-gui
```

Start `tunnelmuxd` first, then point the GUI at the daemon URL if you changed the default.

Diagnostics polling runs only while the diagnostics workspace is open. The first release uses polling rather than SSE streams.
