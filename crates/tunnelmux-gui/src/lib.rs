pub mod commands;
pub mod daemon_manager;
pub mod provider_installer;
pub mod settings;
pub mod state;
pub mod tray;
pub mod view_models;

pub fn run() {
    tauri::Builder::default()
        .manage(state::GuiAppState::default())
        .setup(|app| {
            use std::sync::atomic::Ordering;

            use tauri::Manager;

            let app_handle = app.handle().clone();

            let exit_requested = app.state::<state::GuiAppState>().exit_requested.clone();
            let tray_ready = match tray::install_tray(&app_handle, exit_requested.clone()) {
                Ok(()) => true,
                Err(err) => {
                    eprintln!("failed to initialize system tray: {err}");
                    false
                }
            };

            if tray_ready {
                if let Some(main_window) = app.get_webview_window("main") {
                    main_window.on_window_event({
                        let main_window = main_window.clone();
                        let exit_requested = exit_requested.clone();
                        move |event| {
                            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                                if !exit_requested.load(Ordering::SeqCst) {
                                    api.prevent_close();
                                    let _ = main_window.hide();
                                }
                            }
                        }
                    });
                }
            }

            tauri::async_runtime::spawn(async move {
                let _ = commands::bootstrap_local_daemon(&app_handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_settings,
            commands::save_settings,
            commands::ensure_local_daemon,
            commands::daemon_connection_state,
            commands::probe_connection,
            commands::load_tunnel_workspace,
            commands::load_provider_availability_snapshot,
            commands::install_provider,
            commands::save_tunnel_profile,
            commands::select_tunnel_profile,
            commands::delete_tunnel_profile,
            commands::refresh_dashboard,
            commands::start_tunnel,
            commands::stop_tunnel,
            commands::list_routes,
            commands::save_route,
            commands::delete_route,
            commands::load_diagnostics_summary,
            commands::load_upstreams_health,
            commands::load_recent_logs,
            commands::load_provider_status_summary,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build TunnelMux GUI")
        .run(|app, event| {
            use std::sync::atomic::Ordering;

            use tauri::Manager;

            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app.try_state::<state::GuiAppState>() {
                    state.exit_requested.store(true, Ordering::SeqCst);
                }
            }
        });
}
