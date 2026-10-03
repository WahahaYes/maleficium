//! `maleficium-engine <subcommand>`: the native engines the app and the MCP
//! server run as a killable child. `compile` is Tectonic's `-X compile` with
//! the arguments the app uses and the same plain status lines on stderr;
//! `convert` is latexml; `dump` generates latexml's format dumps;
//! `synctex` answers forward and inverse SyncTeX queries; `kpsewhich` is the
//! file finder latexml runs (the engine also answers as it when started under
//! that name, see `session.rs`).

mod compile;
mod convert;
mod kpsewhich;
mod session;
mod synctex;

fn main() {
    let mut args = std::env::args_os();
    // Checked first and kept trivial: latexml spawns this once per file lookup.
    let argv0 = args.next();
    if kpsewhich::invoked_as_kpsewhich(argv0.as_ref()) {
        std::process::exit(kpsewhich::run(args.collect()));
    }
    let code = match args.next().and_then(|a| a.into_string().ok()).as_deref() {
        Some("compile") => compile::run(args.collect()),
        Some("convert") => convert::run(args.collect()),
        Some("synctex") => synctex::run(args.collect()),
        Some("kpsewhich") => kpsewhich::run(args.collect()),
        _ => {
            eprintln!("usage: maleficium-engine compile|convert|dump|synctex|kpsewhich ...");
            2
        }
    };
    std::process::exit(code);
}
