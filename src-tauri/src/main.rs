// Release builds on Windows open no console window beside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    // `maleficium --mcp` is the stdio MCP server, so an AI client can point
    // at the app itself (an AppImage has no stable path to maleficium-mcp).
    if std::env::args_os().nth(1).is_some_and(|a| a == "--mcp") {
        return maleficium_mcp::serve_stdio();
    }
    // `maleficium --render-posters <root>`: headless poster renders over
    // stdin/stdout, no editor window (see `render_posters`).
    let mut args = std::env::args_os().skip(1);
    if args.next().is_some_and(|a| a == "--render-posters") {
        let root = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("usage: maleficium --render-posters <project root>"))?;
        std::process::exit(maleficium_lib::render_posters(&root.to_string_lossy())?);
    }
    maleficium_lib::run();
    Ok(())
}
