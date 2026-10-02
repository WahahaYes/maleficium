//! `maleficium-widget-helper --fd N`: one widget in its own process. It reads
//! its config and content from the inherited socket, shows a sandboxed
//! WebKitGTK view as a child of the editor's X11 window, relays the widget's
//! bridge messages, and exits when the socket closes.

fn main() {
    maleficium_widget_helper::run();
}
