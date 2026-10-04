use tauri::Manager;

mod commands;
mod poster;
mod speculative;

/// The app's one context: assets and config are embedded once.
fn context() -> tauri::Context {
    tauri::generate_context!()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(maleficium_core::Core::default())
        .manage(poster::Renders::default())
        .register_asynchronous_uri_scheme_protocol(poster::SCHEME, poster::protocol)
        .setup(|app| {
            // The editor window from the config: it loads no widget or
            // author content before setup, so the flags are set first.
            for win in app.webview_windows().values() {
                speculative::off(win);
            }
            // Compiles render missing auto-posters in this process.
            let renderer = commands::poster::AppRenderer(app.handle().clone());
            app.state::<maleficium_core::Core>()
                .set_poster_renderer(std::sync::Arc::new(renderer));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::api::core_request,
            commands::compile::compile_tex,
            commands::compile::cancel_compile,
            commands::history::history_get,
            commands::history::history_restore,
            commands::preview::preview_in_browser,
            commands::poster::render_poster,
            commands::widget_approval::widget_approve,
            commands::widget_approval::widget_revoke,
            commands::widget_approval::widget_auto_approve
        ])
        .build(context())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(cx) = app.try_state::<maleficium_core::Core>() {
                    maleficium_core::compile::shutdown(&cx);
                    maleficium_core::reflow::convert::shutdown(&cx);
                }
            }
        });
}

/// `maleficium --render-posters <project root>`: renders posters with no
/// editor window. Reads one JSON request per stdin line (`mainRel`,
/// `widgetId`, `outPath`, optional `timeoutMs` and `digest`, against the
/// given root), writes one JSON result per stdout line (`{"ok":true,
/// "result":...}` with the outcome: `status` `rendered` and the written
/// poster, or `approval_required` for an html widget the user has not
/// approved, which ran nothing; or `{"ok":false,"error":...}`), and exits
/// at end of input: 0 when every request rendered, 1 otherwise.
pub fn render_posters(root: &str) -> anyhow::Result<i32> {
    use maleficium_core::widgets::poster::PosterRequest;
    use std::io::{BufRead, Write};
    const ROOT_ID: &str = "project";
    let cx = maleficium_core::Core::default();
    maleficium_core::fs::grant_root(&cx, ROOT_ID, root).map_err(anyhow::Error::msg)?;
    let mut ctx = context();
    // No editor window: every window here is a poster render.
    ctx.config_mut().app.windows.clear();
    let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = failed.clone();
    let app = tauri::Builder::default()
        .manage(cx.clone())
        .manage(poster::Renders::default())
        .register_asynchronous_uri_scheme_protocol(poster::SCHEME, poster::protocol)
        .setup(move |app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let stdin = std::io::stdin();
                for line in stdin.lock().lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    let res = serde_json::from_str::<serde_json::Value>(&line)
                        .map_err(|e| format!("bad request line: {e}"))
                        .and_then(|mut v| {
                            v["rootId"] = ROOT_ID.into();
                            serde_json::from_value::<PosterRequest>(v)
                                .map_err(|e| format!("bad request: {e}"))
                        })
                        .and_then(|req| commands::poster::run(&handle, &cx, &req));
                    if !matches!(
                        res,
                        Ok(maleficium_core::widgets::poster::PosterOutcome::Rendered(_))
                    ) {
                        failed.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    let out = match res {
                        Ok(r) => serde_json::json!({ "ok": true, "result": r }),
                        Err(e) => serde_json::json!({ "ok": false, "error": e }),
                    };
                    let mut o = std::io::stdout().lock();
                    let _ = writeln!(o, "{out}");
                    let _ = o.flush();
                }
                handle.exit(0);
            });
            Ok(())
        })
        .build(ctx)?;
    // run_return, not run: run never returns (it exits the process itself).
    app.run_return(|_, event| {
        // A destroyed render window is often the last one; only the end of
        // input (an explicit exit) ends the run.
        if let tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            api.prevent_exit();
        }
    });
    Ok(i32::from(seen.load(std::sync::atomic::Ordering::Relaxed)))
}
