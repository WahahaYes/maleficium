//! The in-app article, served by URL rather than handed over as bytes.
//!
//! A frame's `srcdoc` (like `about:blank` and `blob:`) inherits its
//! parent's policy container, so the app window's CSP (`script-src 'self'`)
//! would govern the article and every widget frame nested in it, and the
//! reader's inline script would never run. A document loaded from its own
//! URL takes its policy from its own bytes instead: the reader's CSP meta,
//! the same as the exported bundle in a browser.
//!
//! [`serve`] exports the article and shelves the page under a fresh token,
//! one page per project (the next load replaces it); [`page`] reads it back
//! for whichever transport serves the frame: the desktop's custom scheme, or
//! an HTTP route in a web build (which should also scope tokens to the
//! session). No path ever reaches the webview.

use crate::bundle::ArticleAnchor;
use crate::Core;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use ts_rs::TS;

/// A shelved article: the token its page is served under, and the heading
/// anchors the editor-to-article sync scrolls to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ArticleServed {
    pub token: String,
    pub anchors: Vec<ArticleAnchor>,
}

/// The latest page per project root id: `(token, bytes)`.
#[derive(Default)]
pub struct Shelf(Mutex<HashMap<String, (String, Arc<String>)>>);

static NEXT: AtomicU64 = AtomicU64::new(1);

/// 32 hex characters, unique per process and not derivable from the root.
fn fresh_token(root_id: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let seed = format!(
        "{}:{}:{nanos}:{root_id}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    crate::bundle::sha_of(seed.as_bytes())[..32].to_string()
}

impl Shelf {
    fn put(&self, root_id: &str, html: String) -> String {
        let token = fresh_token(root_id);
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(root_id.to_string(), (token.clone(), Arc::new(html)));
        token
    }

    fn get(&self, token: &str) -> Option<Arc<String>> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .find(|(t, _)| t == token)
            .map(|(_, html)| html.clone())
    }
}

/// Exports `main_rel`'s article ([`crate::bundle::article_bundle`]) and
/// shelves it, replacing the project's previous page.
pub fn serve(cx: &Core, root_id: &str, main_rel: &str) -> Result<ArticleServed, String> {
    let view = crate::bundle::article_bundle(cx, root_id, main_rel)?;
    Ok(ArticleServed {
        token: cx.articles().put(root_id, view.html),
        anchors: view.anchors,
    })
}

/// The page shelved under `token`, if it is still the latest of its project.
/// The bytes carry their own CSP meta; serve them as `text/html`.
pub fn page(cx: &Core, token: &str) -> Option<Arc<String>> {
    cx.articles().get(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_page_per_project_is_served_and_replaces_the_last() {
        let cx = Core::default();
        let a1 = cx.articles().put("a", "<p>a1</p>".into());
        let b = cx.articles().put("b", "<p>b</p>".into());
        assert_eq!(
            page(&cx, &a1).as_deref().map(String::as_str),
            Some("<p>a1</p>")
        );
        let a2 = cx.articles().put("a", "<p>a2</p>".into());
        assert_ne!(a1, a2);
        assert!(page(&cx, &a1).is_none(), "a replaced page is gone");
        assert_eq!(
            page(&cx, &a2).as_deref().map(String::as_str),
            Some("<p>a2</p>")
        );
        assert_eq!(
            page(&cx, &b).as_deref().map(String::as_str),
            Some("<p>b</p>")
        );
        assert!(page(&cx, "").is_none() && page(&cx, "nope").is_none());
    }

    #[test]
    fn tokens_are_hex_and_never_name_the_root() {
        let t = fresh_token("/home/me/paper");
        assert_eq!(t.len(), 32);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(t, fresh_token("/home/me/paper"));
    }

    #[test]
    fn an_unknown_root_is_refused_without_naming_a_path() {
        let e = serve(&Core::default(), "no-such-root", "main.tex").unwrap_err();
        assert!(!e.is_empty() && !e.contains('/'), "{e}");
    }
}
