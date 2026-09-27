// Release builds on Windows open no console window beside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    // `maleficium --mcp` is the stdio MCP server, so an AI client can point
    // at the app itself (an AppImage has no stable path to maleficium-mcp).
    if std::env::args_os().nth(1).is_some_and(|a| a == "--mcp") {
        return maleficium_lib::mcp::serve_stdio();
    }
    maleficium_lib::run();
    Ok(())
}
