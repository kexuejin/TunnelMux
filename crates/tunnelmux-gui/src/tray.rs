use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

const TRAY_ID: &str = "main";
const MENU_TOGGLE_ID: &str = "tray.toggle_window";
const MENU_QUIT_ID: &str = "tray.quit";

pub fn install_tray<R: Runtime>(
    app: &AppHandle<R>,
    exit_requested: Arc<AtomicBool>,
) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(
        app,
        MENU_TOGGLE_ID,
        "Show/Hide Window",
        true,
        None::<&str>,
    )?;
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
            if !cfg!(any(target_os = "macos", target_os = "windows")) {
                return;
            }

            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Down,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

pub fn toggle_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let is_visible = window.is_visible().unwrap_or(false);
    let is_minimized = window.is_minimized().unwrap_or(false);

    if is_visible && !is_minimized {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
