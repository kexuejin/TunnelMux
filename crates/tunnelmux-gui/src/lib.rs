pub mod settings;
pub mod state;

pub fn run() {
    tauri::Builder::default()
        .manage(state::GuiAppState::default())
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("failed to run TunnelMux GUI");
}
