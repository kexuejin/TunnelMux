use tauri::Manager;

pub mod commands;
pub mod daemon_manager;
pub mod settings;
pub mod state;
pub mod view_models;

pub fn run() {
    let app = tauri::Builder::default()
        .manage(state::GuiAppState::default())
        .setup(|app| {
            let app_handle = app.handle().clone();
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
            commands::refresh_dashboard,
            commands::start_tunnel,
            commands::stop_tunnel,
            commands::list_routes,
            commands::save_route,
            commands::delete_route,
            commands::load_diagnostics_summary,
            commands::load_upstreams_health,
            commands::load_recent_logs,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build TunnelMux GUI");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }) {
            let state = app_handle.state::<state::GuiAppState>();
            let _ = daemon_manager::stop_managed_daemon_in_state(&state.daemon_runtime);
        }
    });
}
