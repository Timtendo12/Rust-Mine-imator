//! Tauri application: owns the application state and exposes it to the
//! frontend through commands. The frontend only holds view state.

mod audio;
mod commands;
mod editing;
mod frame_editor;
mod export;
mod gizmo;
mod recent;
#[cfg(test)]
mod render_check;
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
            // Interface texts ship with the program.
            let language = app
                .path()
                .resolve("assets/Data/Languages/english.milanguage", tauri::path::BaseDirectory::Resource)
                .map_err(|e| e.to_string())
                .and_then(|path| std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display())))
                .and_then(|bytes| mi_format::language::Language::load(&bytes).map_err(|e| e.to_string()));
            match language {
                Ok(language) => app.state::<state::AppState>().set_language(language),
                Err(error) => eprintln!("Could not load the language file: {error}"),
            }

            // The font of text objects.
            let font = app
                .path()
                .resolve("assets/Data/Fonts/minecraft.png", tauri::path::BaseDirectory::Resource)
                .ok()
                .and_then(|path| std::fs::read(path).ok())
                .and_then(|bytes| mi_assets::SpriteFont::minecraft(&bytes));
            match font {
                Some(font) => app.state::<state::AppState>().set_font(font),
                None => eprintln!("Could not load the Minecraft font; text will not be drawn"),
            }

            // The Minecraft assets ship with the program as well.
            let pack = app
                .path()
                .resolve("assets/Data/Minecraft", tauri::path::BaseDirectory::Resource)
                .map_err(|e| e.to_string())
                .and_then(|folder| {
                    mi_assets::AssetPack::open(&folder, mi_core::version::MINECRAFT_VERSION).map_err(|e| e.to_string())
                });
            match pack {
                Ok(pack) => {
                    // Numeric block ids of old schematics.
                    let legacy = app
                        .path()
                        .resolve("assets/Data/legacy.midata", tauri::path::BaseDirectory::Resource)
                        .ok()
                        .and_then(|path| std::fs::read(path).ok());
                    let state = app.state::<state::AppState>();
                    match legacy {
                        Some(bytes) => state.set_legacy(mi_assets::LegacyBlocks::load(&bytes, pack.blocks())),
                        None => eprintln!("Could not read legacy.midata; old schematics will be empty"),
                    }
                    state.set_pack(pack)
                }
                Err(error) => eprintln!("Could not load the Minecraft assets: {error}"),
            }

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
            commands::close_project,
            commands::recent_projects,
            commands::forget_recent_project,
            commands::evaluate_frame,
            commands::set_viewport_rect,
            commands::viewport_drag,
            commands::viewport_zoom,
            commands::viewport_reset_camera,
            commands::set_view_options,
            commands::viewport_pick,
            commands::set_selection,
            commands::export_image,
            commands::export_movie,
            commands::cancel_export,
            commands::audio_play,
            commands::audio_stop,
            commands::sound_peaks,
            commands::viewport_gizmo,
            editing::set_timeline_values,
            editing::finish_edit,
            editing::move_keyframes,
            editing::remove_keyframes,
            editing::create_keyframes,
            editing::copy_keyframes,
            editing::paste_keyframes,
            editing::rename_timeline,
            editing::set_timelines_hidden,
            editing::undo,
            editing::redo,
            editing::save_project,
            editing::set_region,
            editing::cycle_repeat,
            editing::add_marker,
            editing::edit_marker,
            editing::remove_marker,
            editing::backup_project,
            editing::last_backup,
            editing::create_timeline,
            editing::remove_timelines,
            editing::duplicate_timelines,
            editing::reparent_timelines,
            editing::workbench_items,
            editing::create_model,
            editing::create_block,
            editing::create_item,
            editing::create_scenery,
            editing::create_text,
            editing::create_audio,
            editing::set_model_skin,
            editing::project_settings,
            editing::set_setting,
            editing::set_project_info,
            editing::new_project,
            editing::timeline_values,
            editing::timeline_settings,
            editing::set_timeline_setting
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the application");
}
