//! The `maleficium-article` scheme: serves the in-app article page the core
//! shelved (`maleficium_core::article_shelf`) at `/<token>`, so the Article
//! tab's frame loads it from its own URL and runs under the reader's CSP
//! meta instead of inheriting the window's policy, as a `srcdoc` would.
//! The frame stays `sandbox="allow-scripts"` (an opaque origin). Only
//! editor windows are answered: a poster render window gets nothing.

use maleficium_core::Core;
use tauri::http::{Request, Response};
use tauri::{Manager, Runtime, UriSchemeContext};

/// The scheme the Article tab's frame loads from. The window CSP's
/// `frame-src` names it (`maleficium-article:`, and on Windows
/// `http://maleficium-article.localhost`).
pub const SCHEME: &str = "maleficium-article";

fn reply(status: u16, mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// `GET /<token>`: the shelved page, else 404.
pub fn protocol<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    req: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    if ctx.webview_label().starts_with(crate::poster::LABEL_PREFIX) || req.method() != "GET" {
        return reply(404, "text/plain", Vec::new());
    }
    let token = req.uri().path().trim_start_matches('/');
    let cx = ctx.app_handle().state::<Core>();
    match maleficium_core::article_shelf::page(&cx, token) {
        Some(html) => reply(200, "text/html; charset=utf-8", html.as_bytes().to_vec()),
        None => reply(404, "text/plain", b"no such article".to_vec()),
    }
}
