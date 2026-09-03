//! Tauri commands over `camcomp-core` for the shared browser and desktop UI.
//!
//! Analysis stays in-process and off the UI thread, while native acquisition,
//! progress, cancellation, cache persistence, and scoped photo display remain
//! behind typed commands.
//!
//! The logic lives in a library rather than in `main.rs` so it can be tested
//! without launching a window.

#![forbid(unsafe_code)]

use tauri::{
    menu::{Menu, MenuItemBuilder, MenuItemKind, PredefinedMenuItem, SubmenuBuilder},
    Emitter, Runtime,
};

pub mod commands;

pub use commands::{
    AnalyzeRequest, AnalyzeResponse, Capabilities, CommandError, PhotoSourceKind,
    PreflightResponse, ResolvePhotoSourceResponse, RunState, SourceScope, PROGRESS_EVENT,
};

const OPEN_FOLDER_MENU_ID: &str = "open-folder";
const REFRESH_MENU_ID: &str = "refresh";
const APPLICATION_MENU_EVENT: &str = "camcomp://menu-action";

/// Build the Tauri application.
///
/// Separated from `main` so the command surface can be exercised without opening
/// a window (`PLAN-DROP1.md` §16.2).
pub fn build() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.set_menu(application_menu(app.handle())?)?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            if let Some(action) = menu_action(event.id().as_ref()) {
                let _ = app.emit(APPLICATION_MENU_EVENT, action);
            }
        })
        .manage(RunState::default())
        .manage(SourceScope::default())
        .invoke_handler(tauri::generate_handler![
            commands::capabilities,
            commands::preflight_inputs,
            commands::inspect_inputs,
            commands::analyze,
            commands::cancel_analysis,
            commands::load_analysis,
            commands::apply_overrides,
            commands::resolve_photo_source
        ])
}

fn application_menu<R: Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;
    let open = MenuItemBuilder::with_id(OPEN_FOLDER_MENU_ID, "Open…")
        .accelerator("CmdOrCtrl+O")
        .build(app)?;
    let refresh = MenuItemBuilder::with_id(REFRESH_MENU_ID, "Refresh")
        .accelerator("F5")
        .build(app)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let file_menu = menu.items()?.into_iter().find_map(|item| match item {
        MenuItemKind::Submenu(submenu) => match submenu.text() {
            Ok(text) if text == "File" => Some(submenu),
            _ => None,
        },
        _ => None,
    });
    if let Some(file_menu) = file_menu {
        file_menu.prepend_items(&[&open, &refresh, &separator])?;
    } else {
        let file_menu = SubmenuBuilder::new(app, "File")
            .items(&[&open, &refresh])
            .build()?;
        menu.prepend(&file_menu)?;
    }
    Ok(menu)
}

fn menu_action(id: &str) -> Option<&'static str> {
    match id {
        OPEN_FOLDER_MENU_ID => Some(OPEN_FOLDER_MENU_ID),
        REFRESH_MENU_ID => Some(REFRESH_MENU_ID),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_application_menu_items_to_frontend_actions() {
        assert_eq!(menu_action(OPEN_FOLDER_MENU_ID), Some("open-folder"));
        assert_eq!(menu_action(REFRESH_MENU_ID), Some("refresh"));
        assert_eq!(menu_action("close"), None);
    }
}
