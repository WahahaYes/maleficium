use tauri::Manager;

mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(maleficium_core::Core::default())
        .manage(commands::compile::CompileState::default())
        .invoke_handler(tauri::generate_handler![
            commands::api::core_request,
            commands::compile::compile_tex,
            commands::compile::cancel_compile,
            commands::history::history_get,
            commands::history::history_restore
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(s) = app.try_state::<commands::compile::CompileState>() {
                    if let Some(mut c) = s.0.lock().unwrap().take() {
                        let _ = c.kill();
                        let _ = c.wait();
                    }
                }
            }
        });
}
