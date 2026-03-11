# GUI System Tray Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a system tray icon with a minimal menu (Show/Hide Window, Quit) and change the main window close button to hide the window instead of exiting.

**Architecture:** Implement tray behavior on the Rust side using Tauri 2's tray and menu APIs. Intercept `WindowEvent::CloseRequested` to hide the main window. Use a shared `exit_requested` flag so explicit app exit (tray Quit, Cmd+Q) is not blocked by the close interception.

**Tech Stack:** Rust, Tauri 2 (`tauri::tray`, `tauri::menu`)

---

### Task 1: Enable Tauri Tray Support

**Files:**
- Modify: `crates/tunnelmux-gui/Cargo.toml`

**Step 1: Enable tray feature flags**

Update the Tauri dependency to include tray and PNG decoding support:

```toml
tauri = { version = "2.10.3", features = ["tray-icon", "image-png"] }
```

**Step 2: Verify the crate builds**

Run: `cargo test -p tunnelmux-gui`  
Expected: PASS (existing 97 tests)

---

### Task 2: Add Exit Coordination State

**Files:**
- Modify: `crates/tunnelmux-gui/src/state.rs`

**Step 1: Add an exit flag to managed state**

Add a shared flag so the app can distinguish "close -> hide" from a real exit:

```rust
use std::sync::atomic::AtomicBool;

pub struct GuiAppState {
    // ...
    pub exit_requested: Arc<AtomicBool>,
}
```

Initialize it in `Default`:

```rust
exit_requested: Arc::new(AtomicBool::new(false)),
```

**Step 2: Verify tests still pass**

Run: `cargo test -p tunnelmux-gui`  
Expected: PASS

---

### Task 3: Add Tray Integration Module

**Files:**
- Create: `crates/tunnelmux-gui/src/tray.rs`

**Step 1: Implement tray menu + click behavior**

Implement a small module that:
- builds a tray menu with two items:
  - `tray.toggle_window` ("Show/Hide Window")
  - `tray.quit` ("Quit")
- creates a tray icon with:
  - tooltip "TunnelMux"
  - menu always attached (Linux visibility)
  - `show_menu_on_left_click(false)` so left click can toggle window (macOS + Windows)
- handles menu events:
  - toggle window visibility
  - quit: set `exit_requested = true`, then call `AppHandle::exit(0)`
- handles tray click events (where supported):
  - on left click + `MouseButtonState::Up`, toggle window visibility

Use `include_bytes!("../icons/32x32.png")` with `tauri::image::Image::from_bytes` as a fallback icon if `default_window_icon()` is missing.

Suggested shape:

```rust
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use tauri::{
    AppHandle, Manager,
    menu::{MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    image::Image,
};

const TRAY_ID: &str = "main";
const MENU_TOGGLE_ID: &str = "tray.toggle_window";
const MENU_QUIT_ID: &str = "tray.quit";

pub fn install_tray(app: &AppHandle, exit_requested: Arc<AtomicBool>) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, MENU_TOGGLE_ID, "Show/Hide Window", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT_ID, "Quit", true, None::<&str>)?;
    let menu = MenuBuilder::new(app).item(&toggle).item(&quit).build()?;

    let fallback_icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
    let tray_icon = app.default_window_icon().cloned().unwrap_or(fallback_icon);

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tray_icon)
        .menu(&menu)
        .tooltip("TunnelMux")
        .show_menu_on_left_click(false)
        .on_menu_event({
            let exit_requested = exit_requested.clone();
            move |app, event| {
                if event.id() == MENU_TOGGLE_ID {
                    toggle_main_window(app);
                } else if event.id() == MENU_QUIT_ID {
                    exit_requested.store(true, Ordering::SeqCst);
                    app.exit(0);
                }
            }
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

pub fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return };
    match window.is_visible() {
        Ok(true) => { let _ = window.hide(); }
        _ => {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }
}
```

**Step 2: Verify tests pass**

Run: `cargo test -p tunnelmux-gui`  
Expected: PASS

---

### Task 4: Wire Tray + Close-To-Tray Behavior

**Files:**
- Modify: `crates/tunnelmux-gui/src/lib.rs`

**Step 1: Export the new module**

Add:

```rust
pub mod tray;
```

**Step 2: Install tray and close interception in `setup`**

In `tauri::Builder::setup`:
- retrieve `exit_requested` from managed `GuiAppState`
- call `tray::install_tray(app.handle(), exit_requested.clone())?;`
- fetch the main window via `app.get_webview_window("main")`
- register `on_window_event` and intercept `WindowEvent::CloseRequested`:
  - if `exit_requested` is false, `api.prevent_close()` then `window.hide()`

**Step 3: Ensure normal exits are not blocked**

In `.run(...)`, set `exit_requested = true` on `RunEvent::ExitRequested`:

```rust
.run(|app, event| {
    if let tauri::RunEvent::ExitRequested { .. } = event {
        if let Some(state) = app.try_state::<crate::state::GuiAppState>() {
            state.exit_requested.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
});
```

**Step 4: Verify tests pass**

Run: `cargo test -p tunnelmux-gui`  
Expected: PASS

---

### Task 5: Manual Acceptance Checklist

**Step 1: Run the GUI**

Run: `cargo run -p tunnelmux-gui`

**Step 2: Verify close -> hide**

- click the window close button
- app remains running (tray icon still visible)

**Step 3: Verify tray behavior**

- tray menu "Show/Hide Window" toggles the main window
- tray left click toggles (macOS + Windows)
- tray menu "Quit" exits the app

