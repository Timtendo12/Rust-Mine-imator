//! Tauri application: owns the application state and exposes it to the
//! frontend through commands. The frontend only holds view state.

mod commands;
mod state;

/// Starts the application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::from_args(std::env::args().skip(1)))
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::startup_project,
            commands::open_project,
            commands::evaluate_frame
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the application");
}
