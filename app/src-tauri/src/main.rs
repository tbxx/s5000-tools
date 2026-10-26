//! s5000-tools: a desktop editor for the Akai S5000/S6000.

// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod params;
mod transfer;
mod worker;

#[cfg(test)]
mod tests;

use tauri::{Emitter, Manager};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(worker::Worker::spawn(move |e| {
                drop(handle.emit("disconnected", e))
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::connect,
            commands::disconnect,
            commands::info,
            commands::memory,
            commands::delete_items,
            commands::rename_item,
            commands::create_program,
            commands::create_multi,
            commands::audition,
            commands::get_item,
            commands::put_file,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running s5000-tools");
}
