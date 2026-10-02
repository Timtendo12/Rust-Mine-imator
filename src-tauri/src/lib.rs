//! Tauri application: owns the application state and exposes it to the
//! frontend through commands. The frontend only holds view state.

mod commands;

/// Starts the application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![commands::app_info, commands::inspect_project])
        .run(tauri::generate_context!())
        .expect("failed to start the application");
}
