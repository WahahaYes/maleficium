use tauri::Manager;

mod commands;
pub mod core;
pub mod mcp;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::compile::CompileState::default())
        .invoke_handler(tauri::generate_handler![
            commands::guard::grant_project_access,
            commands::guard::grant_untitled_access,
            commands::compile::compile_tex,
            commands::compile::cancel_compile,
            commands::compile::engine_log,
            commands::compile::outputs_fresh,
            commands::compile::clean_outputs,
            commands::synctex::forward_sync,
            commands::synctex::inverse_sync
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
