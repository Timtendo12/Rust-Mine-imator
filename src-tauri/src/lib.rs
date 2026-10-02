//! Tauri application: owns the application state and exposes it to the
//! frontend through commands. The frontend only holds view state.

mod commands;
mod scene_builder;
mod state;
mod viewport;

use tauri::Manager;

/// Starts the application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::from_args(std::env::args().skip(1)))
        .setup(|app| {
            let window = app.get_webview_window("main").ok_or("the main window is missing")?;
            match viewport::start(window.clone()) {
                Ok(handle) => {
                    let resize = handle.clone();
                    window.on_window_event(move |event| {
                        if let tauri::WindowEvent::Resized(size) = event {
                            resize.resize(size.width, size.height);
                        }
                    });
                    app.state::<state::AppState>().set_viewport(handle);
                }
                // The rest of the application works without a viewport, so
                // a machine without a usable graphics device still starts.
                Err(error) => eprintln!("The 3D viewport is not available: {error}"),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::startup_project,
            commands::open_project,
            commands::evaluate_frame,
            commands::set_viewport_rect,
            commands::viewport_drag,
            commands::viewport_zoom,
            commands::viewport_reset_camera,
            commands::set_view_options
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the application");
}
