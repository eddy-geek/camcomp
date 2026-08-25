// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    camcomp_desktop::build()
        .run(tauri::generate_context!())
        .expect("failed to start the camcomp desktop shell");
}
