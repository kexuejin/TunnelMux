pub mod commands;
pub mod settings;
pub mod state;

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
        ])
        .run(tauri::generate_context!())
        .expect("failed to run TunnelMux GUI");
}
