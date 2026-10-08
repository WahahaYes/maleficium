//! The one-shot poster renderer: one hidden window per render, never shown,
//! never parented to the editor, destroyed (and on Linux its web process
//! killed) when the snapshot arrives or the job's hard time limit passes.
//!
//! The window loads a small host page from the `mfposter` scheme. The host
//! puts the runtime document (with its widget policy) into a sandboxed
//! `srcdoc` frame and speaks bridge protocol 1 to it unchanged: `ready`,
//! then `init` with the source bytes, then on `status loaded` a
//! `snapshot-request`; the `snapshot` reply goes back over the scheme. The
//! scheme answers only the window that owns a live job, so nothing else can
//! read a job's sources or post its result. The core decides everything
//! about the job (`maleficium_core::widgets::poster`); this module only runs
//! it. Speculative loading is off in the window before the host loads
//! (`crate::speculative`).

use maleficium_core::widgets::poster::PosterJob;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use tauri::http::{Request, Response};
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext, UriSchemeResponder};
use tauri::{WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// The scheme the host page, the runtime document and the sources come from.
pub const SCHEME: &str = "mfposter";
/// Window labels of renders start with this.
const LABEL_PREFIX: &str = "poster-";
/// The largest result body accepted (a data URL under 8 MiB, or a message).
const MAX_BODY: usize = 8 * 1024 * 1024;

/// The host page. `__W__` and `__H__` are the frame's CSS pixel size.
const HOST: &str = include_str!("poster/host.html");

struct Slot {
    job: Arc<PosterJob>,
    host: String,
    done: Sender<Result<String, String>>,
}

/// Live renders by window label.
#[derive(Default)]
pub struct Renders(Mutex<HashMap<String, Slot>>);

static NEXT: AtomicU64 = AtomicU64::new(1);

fn host_url() -> tauri::Url {
    // Windows and Android serve custom schemes as http://<scheme>.localhost.
    #[cfg(windows)]
    let s = format!("http://{SCHEME}.localhost/host");
    #[cfg(not(windows))]
    let s = format!("{SCHEME}://localhost/host");
    tauri::Url::parse(&s).expect("a static url parses")
}

fn reply(status: u16, mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Cache-Control", "no-store")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// The `mfposter` scheme: serves a live job to the window that owns it.
pub fn protocol<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    req: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let label = ctx.webview_label().to_string();
    let renders = ctx.app_handle().state::<Renders>();
    let slots = renders.0.lock().unwrap_or_else(|e| e.into_inner());
    let Some(slot) = slots.get(&label) else {
        return responder.respond(reply(404, "text/plain", b"no such render".to_vec()));
    };
    let path = req.uri().path();
    let post = req.method() == "POST";
    let res = match (post, path) {
        (false, "/host") => reply(200, "text/html", slot.host.clone().into_bytes()),
        (false, "/runtime") => reply(200, "text/plain", slot.job.document.clone().into_bytes()),
        (false, "/init") => reply(
            200,
            "application/json",
            serde_json::to_vec(&slot.job.init).unwrap_or_default(),
        ),
        (false, p) if p.starts_with("/source/") => {
            let key = &p["/source/".len()..];
            match slot.job.sources.iter().find(|s| s.key == key) {
                Some(s) => reply(200, "application/octet-stream", s.bytes.clone()),
                None => reply(404, "text/plain", Vec::new()),
            }
        }
        (true, "/done" | "/fail") if req.body().len() <= MAX_BODY => {
            let text = String::from_utf8_lossy(req.body()).into_owned();
            let _ = slot.done.send(if path == "/done" {
                Ok(text)
            } else {
                Err(format!("widget {}: {text}", slot.job.widget_id))
            });
            reply(204, "text/plain", Vec::new())
        }
        _ => reply(404, "text/plain", Vec::new()),
    };
    drop(slots);
    responder.respond(res);
}

/// Kills the window's web process where the platform lets us (a runtime
/// spinning on its main thread would otherwise outlive the window), then
/// destroys the window.
fn kill<R: Runtime>(win: &WebviewWindow<R>) {
    #[cfg(target_os = "linux")]
    let _ = win.with_webview(|w| {
        use webkit2gtk::WebViewExt;
        w.inner().terminate_web_process();
    });
    let _ = win.destroy();
}

/// Runs one job in a fresh hidden window and returns the runtime's PNG data
/// URL. Blocks the calling thread (never the main thread) until the
/// snapshot arrives, the runtime reports an error, or the time limit
/// passes; the window is gone in every case.
pub fn render<R: Runtime>(app: &AppHandle<R>, job: Arc<PosterJob>) -> Result<String, String> {
    let label = format!("{LABEL_PREFIX}{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let (tx, rx) = mpsc::channel();
    let (w, h) = job.frame;
    let host = HOST
        .replace("__W__", &w.to_string())
        .replace("__H__", &h.to_string());
    let timeout = job.timeout;
    let widget = job.widget_id.clone();
    let renders = app.state::<Renders>();
    renders.0.lock().unwrap_or_else(|e| e.into_inner()).insert(
        label.clone(),
        Slot {
            job,
            host,
            done: tx,
        },
    );
    // The window opens blank; speculative loading goes off before the host
    // page (and the widget in it) is navigated to, in the same queue.
    let blank = tauri::Url::parse("about:blank").expect("a static url parses");
    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(blank))
        .title("Maleficium poster")
        .visible(false)
        .focused(false)
        .skip_taskbar(true)
        .decorations(false)
        .resizable(false)
        .inner_size(f64::from(w), f64::from(h))
        .build();
    let out = match built {
        Err(e) => Err(format!(
            "widget {widget}: cannot open the poster renderer: {e}"
        )),
        Ok(win) => {
            crate::speculative::off(&win);
            let r = match win.navigate(host_url()) {
                Err(e) => Err(format!(
                    "widget {widget}: cannot load the poster host: {e}"
                )),
                Ok(()) => rx.recv_timeout(timeout).unwrap_or_else(|_| {
                    Err(format!(
                        "widget {widget}: the runtime gave no poster within {} ms; its renderer was killed",
                        timeout.as_millis()
                    ))
                }),
            };
            kill(&win);
            r
        }
    };
    renders
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&label);
    out
}
