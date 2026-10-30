//! s5000-tools: a desktop editor for the Akai S5000/S6000.

// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod disk;
mod drag_out;
mod multi;
mod params;
mod samplers;
mod transfer;
mod worker;

#[cfg(test)]
mod hardware_tests;
#[cfg(test)]
mod tests;

use serde::Serialize;
use tauri::{Emitter, Manager};

#[derive(Serialize, Clone)]
struct Lost {
    id: u32,
    message: String,
}

/// Remembers the window's position and size between runs. With
/// S5000_TOOLS_CONFIG_DIR set (testing), the file goes there too.
fn window_state<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let mut builder = tauri_plugin_window_state::Builder::default();
    if let Some(dir) = std::env::var_os("S5000_TOOLS_CONFIG_DIR") {
        let path = std::path::PathBuf::from(dir).join("window-state.json");
        builder = builder.with_filename(path.to_string_lossy());
    }
    builder.build()
}

fn main() {
    // Files fetched for earlier drags to the Finder; the Finder has copied them.
    let _ = std::fs::remove_dir_all(commands::drag_root());
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(window_state())
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(samplers::Samplers::new(move |id, message| {
                drop(handle.emit("sampler-lost", Lost { id, message }))
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::samplers,
            drag_out::drag_out_start,
            drag_out::drag_out_finish,
            commands::usb_devices,
            commands::add_sampler,
            commands::remove_sampler,
            commands::reconnect_sampler,
            commands::set_active,
            commands::sampler_info,
            commands::memory,
            commands::delete_items,
            commands::rename_item,
            commands::create_program,
            commands::create_multi,
            commands::audition,
            commands::disks,
            commands::disk_list,
            commands::disk_create_folder,
            commands::disk_delete,
            commands::plan_transfer,
            commands::transfer,
            commands::drag_folder,
            commands::list_folder,
            commands::load_settings,
            commands::save_settings,
            commands::screen,
            commands::press,
            commands::release,
            commands::wheel,
            commands::meters,
            commands::param_catalog,
            commands::read_program,
            commands::read_program_file,
            commands::read_live,
            commands::write_param,
            commands::multi_catalog,
            commands::read_multi,
            commands::read_multi_file,
            commands::read_multi_live,
            commands::write_multi_param,
            commands::set_part_program,
        ])
        .run(tauri::generate_context!())
        .expect("error while running s5000-tools");
}
