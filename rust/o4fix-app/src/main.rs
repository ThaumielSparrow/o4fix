#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod queue;
mod settings;

fn main() {
    // Portable zip = no installer bootstrapper; spec: runtime check + link.
    if tauri::webview_version().is_err() {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("o4fix — WebView2 required")
            .set_description(
                "Microsoft WebView2 Runtime was not found.\n\n\
                 Install it from:\n\
                 https://developer.microsoft.com/microsoft-edge/webview2/\n\
                 then run o4fix again. (Windows 11 includes it by default.)",
            )
            .show();
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(queue::AppState::default())
        .invoke_handler(tauri::generate_handler![
            queue::start_queue,
            queue::cancel_job,
            queue::pick_files,
            queue::load_settings,
            queue::save_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running o4fix");
}
