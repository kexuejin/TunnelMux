# GUI System Tray Design

**Date:** 2026-03-11  
**Status:** Approved for implementation

## Context

The initial GUI MVP intentionally excluded tray/background behavior (see
`docs/plans/2026-03-06-gui-mvp-design.md`). The requirement is now to keep
TunnelMux running when the main window is closed, using the system tray as the
primary re-entry point.

## Goals

- Keep the app resident when the main window close button is clicked (close -> hide).
- Always show a system tray icon while the app is running.
- Provide a minimal tray menu:
  - Show/Hide Window
  - Quit
- Left click on the tray icon toggles window visibility where supported.

## Non-Goals

- No minimize-to-tray behavior changes.
- No auto-start on login / background launch configuration.
- No tunnel lifecycle controls in the tray menu.
- No dock/taskbar visibility changes.

## UX Behavior

- Main window close button:
  - prevents the window from being destroyed
  - hides the window instead
- Tray icon:
  - always visible while the app is running
  - left click toggles main window visibility (macOS + Windows)
  - Linux may not emit tray click events; the tray menu remains the fallback
- Tray menu:
  - Show/Hide Window toggles visibility
  - Quit exits the application

## Implementation Approach (Tauri 2)

- Enable Tauri's `tray-icon` feature.
- Create the tray icon and menu in Rust during `tauri::Builder::setup`:
  - use `tauri::tray::TrayIconBuilder`
  - use `tauri::menu::MenuBuilder`
- Register a window event listener for the main window (`label = "main"`) and
  intercept `WindowEvent::CloseRequested`:
  - if the app is not exiting, call `api.prevent_close()` and `window.hide()`
- Maintain a shared `exit_requested` flag in managed GUI state:
  - tray Quit sets the flag, then calls `AppHandle::exit(0)`
  - `RunEvent::ExitRequested` sets the flag too (avoids close interception
    during a normal app exit such as Cmd+Q)

## Verification

- Manual:
  - close main window: the app keeps running and tray icon remains
  - tray menu "Show/Hide Window": toggles window visibility
  - tray menu "Quit": exits the app
  - left click toggles on macOS + Windows (Linux may not support click events)
- Automated:
  - `cargo test -p tunnelmux-gui`

