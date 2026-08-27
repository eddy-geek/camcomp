//! Tauri commands over `camcomp-core` for the shared browser and desktop UI.
//!
//! Analysis stays in-process and off the UI thread, while native acquisition,
//! progress, cancellation, cache persistence, and scoped photo display remain
//! behind typed commands.
//!
//! The logic lives in a library rather than in `main.rs` so it can be tested
//! without launching a window.

#![forbid(unsafe_code)]

pub mod commands;

pub use commands::{
    AnalyzeRequest, AnalyzeResponse, Capabilities, CommandError, PhotoSourceKind,
    PreflightResponse, ResolvePhotoSourceResponse, RunState, SourceScope, PROGRESS_EVENT,
};

/// Build the Tauri application.
///
/// Separated from `main` so the command surface can be exercised without opening
/// a window (`PLAN-DROP1.md` §16.2).
pub fn build() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
