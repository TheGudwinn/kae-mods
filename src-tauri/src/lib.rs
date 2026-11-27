pub mod commands;
pub mod db;
pub mod error;
pub mod game;
pub mod history;
pub mod install;
pub mod legacy;
pub mod presence;
pub mod library;
pub mod minecraft;
pub mod net;
pub mod profiles;
pub mod queue;
pub mod settings;
pub mod sources;
pub mod steam;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // data of the earlier builds moves to the new folder before anything opens a file there
    #[cfg(windows)]
    legacy::migrate();
    let net = Arc::new(net::NetService::new(net::config::NetConfig::default()).expect("default net config is valid"));
    let gb = Arc::new(sources::gamebanana::GameBanana::new(net.clone()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_dir)?;
            let mut loaded = settings::load(&app_dir.join("settings.json"));
            if let Err(e) = net.update_config(loaded.net.clone()) {
                // A stored proxy that no longer builds must not stop the app from starting.
                eprintln!("saved network settings rejected ({e}); using defaults");
                loaded.net = net::config::NetConfig::default();
            }
            app.manage(commands::AppState::new(net.clone(), gb.clone(), app_dir, loaded));
            // Warm up the local database in the background so the first install is not delayed.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<commands::AppState>();
                match state.db().await {
                    Ok(db) => {
                        // Nothing is running after a restart: interrupted transfers wait, paused, for the user.
                        if let Err(e) = queue::store::recover(&db.pool).await {
                            eprintln!("queue recovery failed: {e}");
                        }
                        let host = Arc::new(commands::TauriHost::new(handle.clone()));
                        tauri::async_runtime::spawn(queue::run(db.pool.clone(), state.queue.clone(), host));
                    }
                    Err(e) => eprintln!("database failed to start: {e}"),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::detect_game,
            commands::check_game_path,
            commands::search_mods,
            commands::mod_details,
            commands::list_categories,
            commands::hero_portraits,
            commands::hero_model,
            commands::hero_animations,
            commands::hero_animation,
            commands::enqueue_install,
            commands::list_queue,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::remove_download,
            commands::clear_finished_downloads,
            commands::list_history,
            commands::clear_history,
            commands::pause_all_downloads,
            commands::resume_all_downloads,
            commands::list_installed,
            commands::list_foreign_addons,
            commands::describe_installed,
            commands::set_mod_enabled,
            commands::set_foreign_enabled,
            commands::uninstall_mod,
            commands::last_action,
            commands::undo_last,
            commands::reorder_mods,
            commands::library_stats,
            commands::check_mod_updates,
            commands::update_mod,
            commands::list_profiles,
            commands::create_profile,
            commands::update_profile_from_current,
            commands::rename_profile,
            commands::delete_profile,
            commands::apply_profile,
            commands::export_profile,
            commands::import_profile,
            commands::profile_code,
            commands::import_profile_code,
            commands::get_settings,
            commands::save_settings,
            commands::check_game_folder,
            commands::run_diagnostics,
            commands::app_version,
            commands::get_game_prefs,
            commands::save_game_prefs,
            commands::launch_game,
            commands::game_running,
            commands::run_checks,
            commands::update_foreign,
            commands::scan_compat,
            commands::peek_mod_file,
            commands::fetch_content,
            commands::save_capture,
            commands::hero_clips,
            commands::hero_abilities,
            commands::hero_posters,
            commands::hero_art,
            commands::repair_cameras,
            commands::apply_fix,
            commands::perf_state,
            commands::perf_details,
            commands::perf_apply,
            commands::perf_fix_foreign,
            commands::check_update,
            commands::is_installed,
            commands::mc_config,
            commands::mc_root,
            commands::mc_set_root,
            commands::mc_save_config,
            commands::mc_game_versions,
            commands::mc_loader_versions,
            commands::mc_instances,
            commands::mc_create_instance,
            commands::mc_update_instance,
            commands::mc_delete_instance,
            commands::mc_prepare,
            commands::mc_launch,
            commands::mc_running,
            commands::mc_stop,
            commands::mc_java,
            commands::mc_search,
            commands::mc_install_project,
            commands::mc_install_modpack,
            commands::mc_export_instance,
            commands::mc_import_pack,
            commands::mc_files,
            commands::mc_file_enabled,
            commands::mc_file_remove,
            commands::mc_open_folder,
            commands::mc_log,
            commands::mc_screenshots,
            commands::mc_check_updates,
            commands::mc_update_file,
            commands::mc_covers,
            commands::mc_set_cover,
            commands::mc_installed,
            commands::mc_project,
            commands::mc_file_meta,
            commands::mc_file_meta_cached,
            commands::mc_system_memory,
            commands::mc_cancel,
            commands::mc_ms_start,
            commands::mc_ms_poll,
            commands::mc_ms_logout,
            commands::mc_clone_instance,
            commands::mc_repair_instance,
            commands::mc_backup_saves,
            commands::mc_check_pack_update,
            commands::mc_update_pack,
            commands::mc_export_code,
            commands::mc_import_code,
            commands::mc_set_cover_url,
            commands::mc_screenshot,
            commands::mc_open_screenshot,
            commands::presence_set,
            commands::presence_available,
            commands::install_update
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(db) = app.state::<commands::AppState>().db.get() {
                    let _ = tauri::async_runtime::block_on(db.shutdown());
                }
            }
        });
}
