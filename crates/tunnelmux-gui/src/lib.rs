pub mod commands;
pub mod daemon_manager;
pub mod settings;
pub mod state;
pub mod view_models;

pub fn run() {
    tauri::Builder::default()
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
            commands::load_tunnel_workspace,
            commands::load_provider_availability_snapshot,
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
        .run(|_, _| {});
}
