//! The menu bar. Most items are carried out by the web side: the backend
//! sends `menu` events to the window they apply to. Views, Preferences,
//! opening windows and Help go to the main window, which owns the window
//! list; the rest (size, panes) to whichever window has focus.

use serde::Serialize;
#[cfg(not(target_os = "macos"))]
use tauri::menu::PredefinedMenuItem;
use tauri::menu::{Menu, MenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, EventTarget, Manager, Runtime};
use tauri_plugin_opener::OpenerExt;

const ISSUES: &str = "https://github.com/tbxx/s5000-tools/issues";
const PROJECT: &str = "https://github.com/tbxx/s5000-tools";

#[derive(Serialize, Clone)]
struct MenuEvent<'a> {
    id: &'a str,
    /// The window it is for; the others ignore it.
    window: String,
}

fn item<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    label: &str,
    key: Option<&str>,
) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(app, id, label, true, key)
}

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let name = app.package_info().name.clone();
    let app_menu = SubmenuBuilder::new(app, &name)
        .about(None)
        .separator()
        .item(&item(
            app,
            "app.preferences",
            "Preferences…",
            Some("CmdOrCtrl+,"),
        )?)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let file = SubmenuBuilder::new(app, "File").close_window().build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let view = SubmenuBuilder::new(app, "View")
        .item(&item(app, "view.transfers", "Transfers", Some("CmdOrCtrl+1"))?)
        .item(&item(app, "view.panel", "Front panel", Some("CmdOrCtrl+2"))?)
        .item(&item(app, "view.meters", "Meters", Some("CmdOrCtrl+3"))?)
        .item(&item(app, "view.program", "Program", Some("CmdOrCtrl+4"))?)
        .item(&item(app, "view.multi", "Multi", Some("CmdOrCtrl+5"))?)
        .separator()
        .item(&item(app, "size.bigger", "Bigger", Some("CmdOrCtrl+="))?)
        .item(&item(app, "size.smaller", "Smaller", Some("CmdOrCtrl+-"))?)
        .item(&item(app, "size.actual", "Actual size", Some("CmdOrCtrl+0"))?)
        .separator()
        .item(&item(app, "pane.next", "Next pane", Some("CmdOrCtrl+]"))?)
        .item(&item(app, "pane.previous", "Previous pane", Some("CmdOrCtrl+["))?)
        .separator()
        .fullscreen()
        .build()?;
    let window = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .item(&item(
            app,
            "window.panel",
            "Front panel in its own window",
            Some("CmdOrCtrl+Shift+2"),
        )?)
        .item(&item(
            app,
            "window.meters",
            "Meters in their own window",
            Some("CmdOrCtrl+Shift+3"),
        )?)
        .item(&item(
            app,
            "window.program",
            "New program window",
            Some("CmdOrCtrl+Shift+4"),
        )?)
        .item(&item(
            app,
            "window.multi",
            "New multi window",
            Some("CmdOrCtrl+Shift+5"),
        )?)
        .item(&item(app, "window.keyboard", "Keyboard", Some("CmdOrCtrl+K"))?)
        .separator()
        .item(&item(
            app,
            "window.main",
            "Main window",
            Some("CmdOrCtrl+Shift+1"),
        )?)
        .build()?;
    let help = SubmenuBuilder::new(app, "Help")
        .item(&item(
            app,
            "help.guide",
            &format!("{name} Help"),
            Some("CmdOrCtrl+Shift+/"),
        )?)
        .item(&item(app, "help.keys", "Keyboard shortcuts", None)?)
        .separator()
        .item(&item(app, "help.project", "Project page", None)?)
        .item(&item(app, "help.report", "Report a problem…", None)?)
        .build()?;
    #[cfg(target_os = "macos")]
    {
        window.set_as_windows_menu_for_nsapp()?;
        help.set_as_help_menu_for_nsapp()?;
    }
    let menu = Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window, &help])?;
    // Other systems have no app menu; Preferences moves to Edit, as is usual there.
    #[cfg(not(target_os = "macos"))]
    {
        menu.remove(&app_menu)?;
        edit.append(&PredefinedMenuItem::separator(app)?)?;
        edit.append(&item(
            app,
            "app.preferences",
            "Preferences…",
            Some("CmdOrCtrl+,"),
        )?)?;
        file.append(&PredefinedMenuItem::quit(app, None)?)?;
    }
    Ok(menu)
}

pub fn handle<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "help.project" => drop(app.opener().open_url(PROJECT, None::<&str>)),
        "help.report" => drop(app.opener().open_url(ISSUES, None::<&str>)),
        _ => {
            let focused = app
                .webview_windows()
                .into_iter()
                .find(|(_, w)| w.is_focused().unwrap_or(false))
                .map(|(label, _)| label);
            let to_main = !(id.starts_with("size.") || id.starts_with("pane."));
            let target = if to_main {
                "main".to_string()
            } else {
                focused.unwrap_or_else(|| "main".into())
            };
            // Switching views and Preferences happen in the main window, so bring it forward.
            if (id.starts_with("view.") || id == "app.preferences" || id == "window.main")
                && let Some(main) = app.get_webview_window("main")
            {
                let _ = main.unminimize();
                let _ = main.set_focus();
            }
            let _ = app.emit_to(
                EventTarget::webview_window(target.clone()),
                "menu",
                MenuEvent { id, window: target },
            );
        }
    }
}
