//! `maleficium-engine <subcommand>`: the native engines the app and the MCP
//! server run as a killable child. `compile` is Tectonic's `-X compile` with
//! the arguments the app uses and the same plain status lines on stderr;
//! `convert` is latexml; `synctex` answers forward and inverse SyncTeX queries.

mod compile;
mod convert;
mod synctex;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let code = match args.next().and_then(|a| a.into_string().ok()).as_deref() {
        Some("compile") => compile::run(args.collect()),
        Some("convert") => convert::run(args.collect()),
        Some("synctex") => synctex::run(args.collect()),
        _ => {
            eprintln!("usage: maleficium-engine compile|convert|synctex ...");
            2
        }
    };
    std::process::exit(code);
}
