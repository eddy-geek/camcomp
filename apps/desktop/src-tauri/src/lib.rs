//! Tauri commands over `camcomp-core`.
//!
//! This is the ADR acceptance spike of `PLAN-DROP1.md` §16, not the Drop 2 UI.
//! What it has to demonstrate is narrow and specific: the core is called
//! *directly* with no sidecar, analysis runs off the UI thread, progress and
//! cancellation work, errors arrive structured, and `camcomp.json` is written
//! atomically.
//!
//! The logic lives in a library rather than in `main.rs` so it can be tested
//! without launching a window.

#![forbid(unsafe_code)]

pub mod commands;

pub use commands::{
    AnalyzeRequest, AnalyzeResponse, Capabilities, CommandError, RunState, PROGRESS_EVENT,
};

/// Build the Tauri application.
///
/// Separated from `main` so the command surface can be exercised without opening
/// a window (`PLAN-DROP1.md` §16.2).
pub fn build() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(RunState::default())
        .invoke_handler(tauri::generate_handler![
            commands::capabilities,
            commands::inspect_inputs,
            commands::analyze,
            commands::cancel_analysis,
            commands::load_analysis,
            commands::apply_overrides
        ])
}
