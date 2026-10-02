// Release builds on Windows open no console window beside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    // `maleficium --mcp` is the stdio MCP server, so an AI client can point
    // at the app itself (an AppImage has no stable path to maleficium-mcp).
    if std::env::args_os().nth(1).is_some_and(|a| a == "--mcp") {
        return maleficium_mcp::serve_stdio();
    }
    // `maleficium --widget-helper --fd N` is one live widget in its own
    // process: the helper library, entered before anything of the app starts.
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == "--widget-helper")
    {
        maleficium_widget_helper::run();
        return Ok(());
    }
    // Live widgets are X11 child windows of the editor, so the app runs on X11
    // (XWayland under a Wayland session) wherever an X display exists.
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_some() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    maleficium_lib::run();
    Ok(())
}
