//! `maleficium-widget-helper --fd N`: one widget in its own process. It reads
//! its config and content from the inherited socket, shows a sandboxed
//! WebKitGTK view as a child of the editor's X11 window, relays the widget's
//! bridge messages, and exits when the socket closes.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("maleficium-widget-helper: no widget helper on this platform");
    std::process::exit(2);
}

#[cfg(target_os = "linux")]
fn main() {
    linux::run();
}

#[cfg(target_os = "linux")]
mod linux {
    use gdk::prelude::*;
    use gtk::prelude::*;
    use javascriptcore::ValueExt;
    use maleficium_widget_helper::{
        host_page, host_policy, init_script, post_script, relay, theme_message, valid_id,
        world_script, Content, HANDLER, HOST_URI, SPECULATIVE, WORLD,
    };
    use maleficium_widget_host::proto::{
        encode, ActiveWhy, Decoder, FromHelper, Hardening, Placement, Rect, ToHelper, PROTOCOL,
    };
    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::os::fd::FromRawFd;
    use std::os::unix::net::UnixStream;
    use std::rc::Rc;
    use webkit2gtk::{
        NavigationPolicyDecision, NavigationPolicyDecisionExt, NetworkProxyMode,
        NetworkProxySettings, PolicyDecisionExt, PolicyDecisionType, SecurityManagerExt,
        SettingsExt, URIRequestExt, URISchemeRequestExt, URISchemeResponseExt,
        UserContentInjectedFrames, UserContentManager, UserContentManagerExt, UserScript,
        UserScriptInjectionTime, WebContext, WebContextExt, WebView, WebViewExt,
        WebsiteDataManager,
    };
    use x11::xlib;

    /// No proxy listens here: the engine's own connections go nowhere.
    const DEAD_PROXY: &str = "http://127.0.0.1:9";

    #[allow(non_camel_case_types)]
    type gboolean = libc::c_int;

    #[repr(C)]
    struct FeatureList {
        _p: [u8; 0],
    }
    #[repr(C)]
    struct Feature {
        _p: [u8; 0],
    }

    #[link(name = "webkit2gtk-4.1")]
    extern "C" {
        fn webkit_settings_get_all_features() -> *mut FeatureList;
        fn webkit_feature_list_get_length(list: *mut FeatureList) -> usize;
        fn webkit_feature_list_get(list: *mut FeatureList, index: usize) -> *mut Feature;
        fn webkit_feature_list_unref(list: *mut FeatureList);
        fn webkit_feature_get_identifier(f: *mut Feature) -> *const libc::c_char;
        fn webkit_settings_set_feature_enabled(
            settings: *mut webkit2gtk::ffi::WebKitSettings,
            f: *mut Feature,
            enabled: gboolean,
        );
    }

    const SHAPE_BOUNDING: libc::c_int = 0;
    const SHAPE_INPUT: libc::c_int = 2;
    const SHAPE_SET: libc::c_int = 0;
    const UNSORTED: libc::c_int = 0;

    #[link(name = "Xext")]
    extern "C" {
        fn XShapeCombineRectangles(
            dpy: *mut xlib::Display,
            dest: xlib::Window,
            kind: libc::c_int,
            x: libc::c_int,
            y: libc::c_int,
            rects: *mut xlib::XRectangle,
            n: libc::c_int,
            op: libc::c_int,
            ordering: libc::c_int,
        );
        fn XShapeCombineMask(
            dpy: *mut xlib::Display,
            dest: xlib::Window,
            kind: libc::c_int,
            x: libc::c_int,
            y: libc::c_int,
            src: xlib::Pixmap,
            op: libc::c_int,
        );
    }

    fn die(msg: &str) -> ! {
        eprintln!("maleficium-widget-helper: {msg}");
        std::process::exit(3);
    }

    /// Turns the speculative-loading features off; returns the ones found.
    fn speculative_off(view: &WebView) -> Vec<String> {
        let mut off = Vec::new();
        let settings = WebViewExt::settings(view).expect("a view has settings");
        unsafe {
            let list = webkit_settings_get_all_features();
            for i in 0..webkit_feature_list_get_length(list) {
                let f = webkit_feature_list_get(list, i);
                let id = std::ffi::CStr::from_ptr(webkit_feature_get_identifier(f))
                    .to_string_lossy()
                    .into_owned();
                if SPECULATIVE.contains(&id.as_str()) {
                    webkit_settings_set_feature_enabled(
                        glib::translate::ToGlibPtr::to_glib_none(&settings).0,
                        f,
                        0,
                    );
                    off.push(id);
                }
            }
            webkit_feature_list_unref(list);
        }
        off
    }

    fn respond(req: &webkit2gtk::URISchemeRequest, body: Vec<u8>, csp: Option<&str>) {
        let len = body.len() as i64;
        let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(body));
        let resp = webkit2gtk::URISchemeResponse::new(&stream, len);
        resp.set_content_type("text/html; charset=utf-8");
        let headers = soup::MessageHeaders::new(soup::MessageHeadersType::Response);
        if let Some(c) = csp {
            headers.append("Content-Security-Policy", c);
        }
        headers.append("Cache-Control", "no-store");
        resp.set_http_headers(headers);
        req.finish_with_response(&resp);
    }

    fn not_found(req: &webkit2gtk::URISchemeRequest) {
        let mut e = glib::Error::new(gio::IOErrorEnum::NotFound, "not served");
        req.finish_error(&mut e);
    }

    struct Helper {
        out: UnixStream,
        id: String,
        content: Content,
        view: WebView,
        dpy: *mut xlib::Display,
        xid: xlib::Window,
        gdk_win: gdk::Window,
        win: gtk::Window,
        size: (i32, i32),
        active: bool,
        loaded: bool,
        t0: std::time::Instant,
    }

    impl Helper {
        fn send(&mut self, m: &FromHelper) {
            if self.out.write_all(&encode(m)).is_err() {
                std::process::exit(0);
            }
        }

        fn js(&self, script: &str) {
            self.view.evaluate_javascript(
                script,
                Some(WORLD),
                None,
                None::<&gio::Cancellable>,
                |_| {},
            );
        }

        fn shape(&self, kind: libc::c_int, r: Option<Rect>) {
            unsafe {
                match r {
                    Some(r) => {
                        let mut x = xlib::XRectangle {
                            x: r.x as i16,
                            y: r.y as i16,
                            width: r.w.max(0) as u16,
                            height: r.h.max(0) as u16,
                        };
                        let n = i32::from(!r.is_empty());
                        XShapeCombineRectangles(
                            self.dpy, self.xid, kind, 0, 0, &mut x, n, SHAPE_SET, UNSORTED,
                        );
                    }
                    None => XShapeCombineMask(self.dpy, self.xid, kind, 0, 0, 0, SHAPE_SET),
                }
                xlib::XFlush(self.dpy);
            }
        }

        /// Hidden widgets stay mapped with nothing drawn, so the engine does
        /// not throttle the page and the watchdog sees a real beat.
        fn place(&mut self, p: Placement) {
            if !p.visible || p.clip.is_empty() {
                self.shape(SHAPE_BOUNDING, Some(Rect::default()));
                self.shape(SHAPE_INPUT, Some(Rect::default()));
                return;
            }
            let s = self.gdk_win.scale_factor().max(1);
            let (w, h) = (p.slot.w.max(1), p.slot.h.max(1));
            if (w, h) != self.size {
                self.size = (w, h);
                self.gdk_win
                    .move_resize(p.slot.x / s, p.slot.y / s, w / s, h / s);
                self.win.resize(w / s, h / s);
                self.win
                    .size_allocate(&gtk::Allocation::new(0, 0, w / s, h / s));
            } else {
                self.gdk_win.move_(p.slot.x / s, p.slot.y / s);
            }
            let clip = Rect {
                x: p.clip.x - p.slot.x,
                y: p.clip.y - p.slot.y,
                w: p.clip.w,
                h: p.clip.h,
            };
            self.shape(SHAPE_BOUNDING, Some(clip));
            if self.active {
                self.shape(SHAPE_INPUT, None);
            } else {
                self.shape(SHAPE_INPUT, Some(Rect::default()));
            }
        }

        fn set_active(&mut self, on: bool, why: ActiveWhy) {
            if on == self.active {
                return;
            }
            self.active = on;
            if on {
                self.shape(SHAPE_INPUT, None);
                unsafe {
                    xlib::XSetInputFocus(
                        self.dpy,
                        self.xid,
                        xlib::RevertToParent,
                        xlib::CurrentTime,
                    );
                    xlib::XFlush(self.dpy);
                }
                self.view.grab_focus();
                self.js("window.__mfwFocus()");
            } else {
                self.shape(SHAPE_INPUT, Some(Rect::default()));
            }
            self.send(&FromHelper::Active { on, why });
        }

        fn handle(&mut self, msg: ToHelper) {
            match msg {
                ToHelper::Place { seq, placement } => {
                    self.place(placement);
                    self.send(&FromHelper::Placed { seq });
                }
                ToHelper::Init {
                    runtime,
                    alt,
                    options,
                    theme,
                } => {
                    let s = init_script(
                        &self.id,
                        &runtime,
                        &alt,
                        &options,
                        &theme,
                        &self.content.sources,
                    );
                    self.js(&s);
                }
                ToHelper::Theme(t) => self.js(&post_script(&theme_message(&t))),
                ToHelper::Activate { on } => self.set_active(on, ActiveWhy::Host),
                ToHelper::SnapshotRequest { request_id } => self.js(&post_script(
                    &serde_json::json!({"mfw": 1, "type": "snapshot-request", "requestId": request_id}),
                )),
                ToHelper::Shutdown => gtk::main_quit(),
                other => die(&format!("unexpected {other:?} after load")),
            }
        }
    }

    fn read_frame(sock: &mut UnixStream, dec: &mut Decoder) -> ToHelper {
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            match dec.next_frame() {
                Ok(Some(body)) => {
                    return serde_json::from_slice(&body)
                        .unwrap_or_else(|e| die(&format!("bad frame: {e}")))
                }
                Ok(None) => {}
                Err(e) => die(&format!("{e:?}")),
            }
            match sock.read(&mut buf) {
                Ok(0) | Err(_) => std::process::exit(0),
                Ok(n) => dec.push(&buf[..n]),
            }
        }
    }

    pub fn run() {
        // Die with the host, whatever happens to it.
        unsafe {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            if libc::getppid() == 1 {
                std::process::exit(0);
            }
        }
        let args: Vec<String> = std::env::args().collect();
        let fd: i32 = match args.as_slice() {
            [_, flag, n] if flag == "--fd" => n.parse().unwrap_or_else(|_| die("bad --fd")),
            _ => die("usage: maleficium-widget-helper --fd N"),
        };
        let mut sock = unsafe { UnixStream::from_raw_fd(fd) };
        let mut dec = Decoder::default();

        let (id, parent, width, height, csp, hardening): (
            String,
            u64,
            i32,
            i32,
            String,
            Hardening,
        ) = match read_frame(&mut sock, &mut dec) {
            ToHelper::Config {
                proto,
                widget_id,
                parent,
                width,
                height,
                csp,
                hardening,
            } if proto == PROTOCOL => (widget_id, parent, width, height, csp, hardening),
            other => die(&format!("expected config, got {other:?}")),
        };
        if !valid_id(&id) {
            die("invalid widget id");
        }
        let mut content = Content::default();
        while !content.loaded {
            let f = read_frame(&mut sock, &mut dec);
            content.take(&f).unwrap_or_else(|e| die(&e));
        }

        gdk::set_allowed_backends("x11");
        gtk::init().unwrap_or_else(|e| die(&format!("no display: {e}")));
        let display = gdk::Display::default().unwrap_or_else(|| die("no display"));
        let xdisplay: gdkx11::X11Display = display
            .clone()
            .downcast()
            .unwrap_or_else(|_| die("the display is not X11"));

        // A context of its own: ephemeral, sandboxed, the dead proxy, and the
        // widget schemes registered here only.
        let dm = WebsiteDataManager::new_ephemeral();
        if hardening.dead_proxy {
            let mut s = NetworkProxySettings::new(Some(DEAD_PROXY), &[]);
            webkit2gtk::WebsiteDataManagerExt::set_network_proxy_settings(
                &dm,
                NetworkProxyMode::Custom,
                Some(&mut s),
            );
        }
        let ctx = WebContext::with_website_data_manager(&dm);
        ctx.set_sandbox_enabled(true);
        let doc = Rc::new(std::mem::take(&mut content.document).into_bytes());
        let widget_uri = format!("mfw://{id}/index.html");
        let widget_csp = (hardening.csp_header && !csp.is_empty()).then_some(csp);
        {
            let doc = doc.clone();
            let widget_uri = widget_uri.clone();
            ctx.register_uri_scheme("mfw", move |req| {
                if req.uri().as_deref() == Some(widget_uri.as_str()) {
                    respond(req, doc.as_ref().clone(), widget_csp.as_deref());
                } else {
                    not_found(req);
                }
            });
        }
        {
            let page = host_page(&id).into_bytes();
            let policy = host_policy(&id);
            ctx.register_uri_scheme("mfwhost", move |req| {
                if req.uri().as_deref() == Some(HOST_URI) {
                    respond(req, page.clone(), Some(&policy));
                } else {
                    not_found(req);
                }
            });
        }
        if let Some(sm) = ctx.security_manager() {
            sm.register_uri_scheme_as_secure("mfw");
            sm.register_uri_scheme_as_secure("mfwhost");
        }

        let ucm = UserContentManager::new();
        ucm.add_script(&UserScript::for_world(
            &world_script(),
            UserContentInjectedFrames::TopFrame,
            UserScriptInjectionTime::End,
            WORLD,
            &[],
            &[],
        ));
        let world_handler = ucm.register_script_message_handler_in_world(HANDLER, WORLD);
        if !world_handler {
            die("cannot register the bridge handler in its world");
        }
        let view = WebView::builder()
            .web_context(&ctx)
            .user_content_manager(&ucm)
            .build();
        if let Some(s) = WebViewExt::settings(&view) {
            s.set_enable_javascript(true);
            s.set_enable_developer_extras(false);
            s.set_javascript_can_open_windows_automatically(false);
            s.set_enable_page_cache(false);
            s.set_enable_write_console_messages_to_stdout(false);
        }
        let features_off = if hardening.speculative_off {
            speculative_off(&view)
        } else {
            Vec::new()
        };
        view.connect_context_menu(|_, _, _, _| true);
        view.connect_create(|_, _| None);
        {
            let widget_uri = widget_uri.clone();
            view.connect_decide_policy(move |_, decision, kind| match kind {
                PolicyDecisionType::NavigationAction => {
                    let uri = decision
                        .downcast_ref::<NavigationPolicyDecision>()
                        .and_then(|d| d.navigation_action())
                        .and_then(|a| a.request())
                        .and_then(|r| r.uri())
                        .map(|u| u.to_string())
                        .unwrap_or_default();
                    if uri == HOST_URI || uri == widget_uri || uri == "about:blank" {
                        false
                    } else {
                        decision.ignore();
                        true
                    }
                }
                PolicyDecisionType::NewWindowAction => {
                    decision.ignore();
                    true
                }
                _ => false,
            });
        }

        // The view lives in a child window of the editor's window.
        let foreign = gdkx11::X11Window::foreign_new_for_display(&xdisplay, parent as xlib::Window);
        let attrs = gdk::WindowAttr {
            x: Some(0),
            y: Some(0),
            width: width.max(1),
            height: height.max(1),
            window_type: gdk::WindowType::Child,
            wclass: gdk::WindowWindowClass::InputOutput,
            event_mask: gdk::EventMask::all(),
            visual: Some(
                display
                    .default_screen()
                    .system_visual()
                    .unwrap_or_else(|| die("no visual")),
            ),
            ..gdk::WindowAttr::default()
        };
        let gdk_win = gdk::Window::new(Some(foreign.upcast_ref()), &attrs);
        let xid = gdk_win
            .downcast_ref::<gdkx11::X11Window>()
            .unwrap_or_else(|| die("not an X11 window"))
            .xid();
        let dpy = unsafe {
            gdkx11::ffi::gdk_x11_display_get_xdisplay(
                glib::translate::ToGlibPtr::to_glib_none(&xdisplay).0,
            ) as *mut xlib::Display
        };
        let win = gtk::Window::new(gtk::WindowType::Toplevel);
        {
            let gw = gdk_win.clone();
            win.connect_realize(move |w| w.set_window(gw.clone()));
        }
        win.set_has_window(true);
        win.realize();
        win.register_window(&gdk_win);
        win.set_default_size(width.max(1), height.max(1));
        win.add(&view);
        win.show_all();

        let h = Rc::new(RefCell::new(Helper {
            out: sock
                .try_clone()
                .unwrap_or_else(|_| die("cannot clone the socket")),
            id: id.clone(),
            content,
            view: view.clone(),
            dpy,
            xid,
            gdk_win: gdk_win.clone(),
            win: win.clone(),
            size: (width.max(1), height.max(1)),
            active: false,
            loaded: false,
            t0: std::time::Instant::now(),
        }));
        {
            let mut hh = h.borrow_mut();
            hh.place(Placement::hidden());
            hh.shape(SHAPE_INPUT, Some(Rect::default()));
            gdk_win.show();
            let ready = FromHelper::Ready {
                pid: std::process::id(),
                window: xid,
                features_off,
                sandbox: true,
                dead_proxy: hardening.dead_proxy,
            };
            hh.send(&ready);
        }

        {
            let h = h.clone();
            ucm.connect_script_message_received(Some(HANDLER), move |_, res| {
                let raw = res
                    .js_value()
                    .map(|v| v.to_str().to_string())
                    .unwrap_or_default();
                if let Some(m) = relay(&raw) {
                    h.borrow_mut().send(&m);
                }
            });
        }
        {
            let h = h.clone();
            view.connect_load_changed(move |_, ev| {
                if ev == webkit2gtk::LoadEvent::Finished {
                    let mut hh = h.borrow_mut();
                    if !hh.loaded {
                        hh.loaded = true;
                        let took_ms = hh.t0.elapsed().as_millis() as u64;
                        hh.send(&FromHelper::Loaded { took_ms });
                    }
                }
            });
        }
        view.connect_web_process_terminated(|_, reason| {
            die(&format!("the web process ended: {reason:?}"));
        });
        {
            let h = h.clone();
            win.connect_key_press_event(move |_, ev| {
                let mut hh = h.borrow_mut();
                if hh.active && ev.keyval() == gdk::keys::constants::Escape {
                    hh.set_active(false, ActiveWhy::Escape);
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
        }
        {
            let h = h.clone();
            win.connect_focus_out_event(move |_, _| {
                if let Ok(mut hh) = h.try_borrow_mut() {
                    if hh.active {
                        hh.set_active(false, ActiveWhy::Blur);
                    }
                }
                glib::Propagation::Proceed
            });
        }

        // Host commands, read on the main loop; end of file ends the helper.
        {
            let h = h.clone();
            let mut reader = sock;
            let mut dec = dec;
            glib::source::unix_fd_add_local(
                fd,
                glib::IOCondition::IN | glib::IOCondition::HUP | glib::IOCondition::ERR,
                move |_, _| {
                    let mut buf = [0u8; 64 * 1024];
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => {
                            gtk::main_quit();
                            return glib::ControlFlow::Break;
                        }
                        Ok(n) => dec.push(&buf[..n]),
                    }
                    loop {
                        match dec.next_frame() {
                            Ok(Some(body)) => match serde_json::from_slice::<ToHelper>(&body) {
                                Ok(m) => h.borrow_mut().handle(m),
                                Err(e) => die(&format!("bad frame: {e}")),
                            },
                            Ok(None) => break,
                            Err(e) => die(&format!("{e:?}")),
                        }
                    }
                    glib::ControlFlow::Continue
                },
            );
        }

        view.load_uri(HOST_URI);
        gtk::main();
    }
}
