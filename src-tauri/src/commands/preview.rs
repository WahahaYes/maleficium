//! Preview in browser: export the single-file paper bundle to the core's
//! scratch folder and hand it to the OS default handler. The browser is the
//! only sandbox. The webview never names the file to open: it names a project
//! and a main file, and the path opened is the one the core just wrote.

use maleficium_core::bundle::{self, BundleExported};
use maleficium_core::Core;
use tauri::State;

#[tauri::command]
pub fn preview_in_browser(
    cx: State<'_, Core>,
    root_id: String,
    main_rel: String,
) -> Result<BundleExported, String> {
    let exported = bundle::preview_bundle(&cx, &root_id, &main_rel)?;
    open::that_detached(&exported.path)
        .map_err(|e| format!("cannot open {} in the browser: {e}", exported.path))?;
    Ok(exported)
}
