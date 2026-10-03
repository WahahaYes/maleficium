//! Speculative loading off in every app webview that can load widget or
//! author content: the editor window and each poster render window.
//!
//! A preconnect, prefetch or DNS prefetch reaches the network outside any
//! content security policy: WebKit opens a TCP connection for
//! `<link rel=preconnect>` from a widget whose policy is `default-src
//! 'none'`, and no setting in the page closes it. The engine's feature flags
//! do.
//!
//! - Linux (WebKitGTK): the five features in [`SPECULATIVE`] are turned off
//!   through the settings feature list (WebKitGTK 2.42 and later). The poster
//!   driver (`e2e/poster-run.sh`) pins it: a widget with preconnect links to
//!   a loopback listener renders and the listener sees no connection.
//! - macOS (WKWebView): the flags are private API (`_features`); there is no
//!   public switch, so nothing is set. UNVERIFIED whether WKWebView leaks.
//! - Windows (WebView2, Chromium): Chromium did not preconnect under the
//!   strict policy in the browser spike, and WebView2 has no setting for it,
//!   so nothing is set. Chromium's prerender and blocked-frame connections
//!   are not covered by any flag here. UNVERIFIED on WebView2.

use tauri::{Runtime, WebviewWindow};

/// Engine features that load speculatively and so reach the network outside
/// any CSP. Names are WebKitGTK feature identifiers.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub const SPECULATIVE: &[&str] = &[
    "LinkPreconnect",
    "LinkDNSPrefetch",
    "LinkPrefetch",
    "LinkPreconnectEarlyHints",
    "SpeculationRulesPrefetch",
];

/// Turns speculative loading off in the window's webview. Runs on the main
/// thread, in order with the window's other webview messages: a navigation
/// requested after this call loads with the features off. Logs the features
/// it found to stderr (`speculative loading off in <label>: ...`).
pub fn off<R: Runtime>(win: &WebviewWindow<R>) {
    #[cfg(target_os = "linux")]
    {
        let label = win.label().to_string();
        let _ = win.with_webview(move |w| {
            let found = linux::off(&w.inner());
            eprintln!(
                "speculative loading off in {label}: {}",
                if found.is_empty() {
                    "none found".to_string()
                } else {
                    found.join(",")
                }
            );
        });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = win;
}

#[cfg(target_os = "linux")]
mod linux {
    use super::SPECULATIVE;
    use std::ffi::{c_char, c_int, CStr};
    use webkit2gtk::WebViewExt;

    #[repr(C)]
    struct FeatureList {
        _p: [u8; 0],
    }
    #[repr(C)]
    struct Feature {
        _p: [u8; 0],
    }

    // The feature list API (WebKitGTK 2.42) is not in the webkit2gtk crate.
    #[link(name = "webkit2gtk-4.1")]
    extern "C" {
        fn webkit_settings_get_all_features() -> *mut FeatureList;
        fn webkit_feature_list_get_length(list: *mut FeatureList) -> usize;
        fn webkit_feature_list_get(list: *mut FeatureList, index: usize) -> *mut Feature;
        fn webkit_feature_list_unref(list: *mut FeatureList);
        fn webkit_feature_get_identifier(f: *mut Feature) -> *const c_char;
        fn webkit_settings_set_feature_enabled(
            settings: *mut webkit2gtk::ffi::WebKitSettings,
            f: *mut Feature,
            enabled: c_int,
        );
    }

    /// Turns [`SPECULATIVE`] off in the view's settings; returns the ones
    /// this WebKitGTK has.
    pub fn off(view: &webkit2gtk::WebView) -> Vec<String> {
        let Some(settings) = view.settings() else {
            return Vec::new();
        };
        let raw: *mut webkit2gtk::ffi::WebKitSettings =
            webkit2gtk::glib::translate::ToGlibPtr::to_glib_none(&settings).0;
        let mut found = Vec::new();
        // SAFETY: the list is owned here and released once; each feature
        // pointer is borrowed from it and used before the release; the
        // identifier is a NUL-terminated string the feature owns.
        unsafe {
            let list = webkit_settings_get_all_features();
            for i in 0..webkit_feature_list_get_length(list) {
                let f = webkit_feature_list_get(list, i);
                let id = CStr::from_ptr(webkit_feature_get_identifier(f)).to_string_lossy();
                if SPECULATIVE.contains(&id.as_ref()) {
                    webkit_settings_set_feature_enabled(raw, f, 0);
                    found.push(id.into_owned());
                }
            }
            webkit_feature_list_unref(list);
        }
        found
    }
}
