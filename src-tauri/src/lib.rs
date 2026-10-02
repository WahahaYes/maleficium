use tauri::Manager;

mod commands;
mod widgets;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(maleficium_core::Core::default())
        .manage(widgets::Widgets::default())
        .invoke_handler(tauri::generate_handler![
            commands::api::core_request,
            commands::compile::compile_tex,
            commands::compile::cancel_compile,
            commands::history::history_get,
            commands::history::history_restore,
            widgets::widgets_open,
            widgets::widgets_approve,
            widgets::widgets_view,
            widgets::widgets_activate,
            widgets::widgets_deactivate_all,
            widgets::widgets_hidden,
            widgets::widgets_theme,
            widgets::widgets_reload,
            widgets::widgets_close
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(cx) = app.try_state::<maleficium_core::Core>() {
                    maleficium_core::compile::shutdown(&cx);
                }
            }
        });
}
