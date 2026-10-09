//! Preview in browser: export the single-file paper bundle to the core's
//! scratch folder and hand it to the OS default handler. The browser is the
//! only sandbox. The webview never names the file to open: it names a project
//! and a main file, and the path opened is the one the core just wrote.

use maleficium_core::bundle::{self, ArticleView, BundleExported};
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

/// The in-app article: the reader bytes with their heading anchors for the
/// tab view. The webview names a project and a main file only; the returned
/// view carries no path.
#[tauri::command]
pub fn article_bundle(
    cx: State<'_, Core>,
    root_id: String,
    main_rel: String,
) -> Result<ArticleView, String> {
    article_view(&cx, &root_id, &main_rel)
}

fn article_view(cx: &Core, root_id: &str, main_rel: &str) -> Result<ArticleView, String> {
    bundle::article_bundle(cx, root_id, main_rel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use maleficium_core::bundle::ArticleAnchor;

    #[test]
    fn an_unknown_root_is_refused_without_naming_a_path() {
        let cx = Core::default();
        let e = article_view(&cx, "no-such-root", "main.tex").unwrap_err();
        assert!(!e.is_empty() && !e.contains('/'), "{e}");
    }

    #[test]
    fn the_view_reaches_the_tab_as_html_and_anchors() {
        let view = ArticleView {
            html: "<article></article>".into(),
            anchors: vec![ArticleAnchor {
                id: "S1".into(),
                text: "1 One".into(),
            }],
        };
        assert_eq!(
            serde_json::to_value(&view).unwrap(),
            serde_json::json!({"html": "<article></article>", "anchors": [{"id": "S1", "text": "1 One"}]})
        );
    }
}
