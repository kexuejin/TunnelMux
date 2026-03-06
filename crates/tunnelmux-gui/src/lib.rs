pub mod commands;
pub mod settings;
pub mod state;
pub mod view_models;

pub fn run() {
    tauri::Builder::default()
        .manage(state::GuiAppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::load_settings,
            commands::save_settings,
            commands::probe_connection,
            commands::refresh_dashboard,
            commands::start_tunnel,
            commands::stop_tunnel,
            commands::list_routes,
            commands::save_route,
            commands::delete_route,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run TunnelMux GUI");
}
