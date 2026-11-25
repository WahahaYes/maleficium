// Release builds on Windows open no console window beside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut args = args.iter().peekable();

    // `maleficium --mcp` is the stdio MCP server, so an AI client can point
    // at the app itself (an AppImage has no stable path to maleficium-mcp).
    if args.peek().is_some_and(|a| *a == "--mcp") {
        args.next();
        return maleficium_mcp::serve_stdio();
    }

    // `maleficium --render-posters <root>`: headless poster renders over
    // stdin/stdout, no editor window (see `render_posters`).
    if args.peek().is_some_and(|a| *a == "--render-posters") {
        args.next();
        let root = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("usage: maleficium --render-posters <project root>"))?;
        std::process::exit(maleficium_lib::render_posters(&root.to_string_lossy())?);
    }

    // `maleficium export <project-dir> --main <rel-tex> --out <dir> [--single-file] [--deny-warnings]`
    // Headless one-shot export: compiles, then writes a paper bundle. Never opens a window.
    if args.peek().is_some_and(|a| *a == "export") {
        args.next();
        return export_cli(args);
    }

    maleficium_lib::run();
    Ok(())
}

fn export_cli(
    mut args: std::iter::Peekable<std::slice::Iter<'_, std::ffi::OsString>>,
) -> anyhow::Result<()> {
    use maleficium_core::{Core, bundle::BundleProfile, compile::COMPILE_TIMEOUT_SECS};
    use std::path::PathBuf;

    let project_dir = args
        .next()
        .ok_or_else(|| usage_error("missing <project-dir>"))?;
    let project_dir = PathBuf::from(project_dir);

    let mut main_rel = None;
    let mut out_dir = None;
    let mut single_file = false;
    let mut deny_warnings = false;

    while let Some(arg) = args.next() {
        let s = arg.to_string_lossy();
        match s.as_ref() {
            "--main" => {
                main_rel = Some(
                    args.next()
                        .ok_or_else(|| usage_error("--main requires a value"))?,
                );
            }
            "--out" => {
                out_dir = Some(
                    args.next()
                        .ok_or_else(|| usage_error("--out requires a value"))?,
                );
            }
            "--single-file" => {
                single_file = true;
            }
            "--deny-warnings" => {
                deny_warnings = true;
            }
            other => {
                return Err(usage_error(&format!("unknown argument: {}", other)));
            }
        }
    }

    let main_rel = main_rel.ok_or_else(|| usage_error("--main is required"))?;
    let out_dir = out_dir.ok_or_else(|| usage_error("--out is required"))?;

    // Validate paths
    if !project_dir.is_dir() {
        return Err(anyhow::anyhow!(
            "project directory does not exist: {}",
            project_dir.display()
        ));
    }
    let project_dir = std::path::absolute(&project_dir)?;
    let out_dir = std::path::absolute(out_dir)?.into_os_string();
    let out_dir = &out_dir;

    let cx = Core::default();
    let root_id = "cli-export";

    // Grant the project root
    maleficium_core::fs::grant_root(&cx, root_id, &project_dir.to_string_lossy())
        .map_err(|e| anyhow::anyhow!("failed to grant project root: {}", e))?;

    // Resolve main file if not provided (but we require --main per design)
    let main_rel = main_rel.to_string_lossy().to_string();

    // Compile the document
    eprintln!("compiling {}", main_rel);
    let job_id =
        maleficium_core::compile::run(&cx, root_id, &main_rel, false, COMPILE_TIMEOUT_SECS)
            .map_err(|e| anyhow::anyhow!("compile start failed: {}", e))?;

    loop {
        let record = maleficium_core::compile::poll(&cx, &job_id, 50)
            .map_err(|e| anyhow::anyhow!("compile poll failed: {}", e))?;
        if record.status == maleficium_core::compile::JobStatus::Running {
            std::thread::sleep(std::time::Duration::from_millis(500));
            continue;
        }
        match record.status {
            maleficium_core::compile::JobStatus::Success => {
                eprintln!("compile succeeded");
                break;
            }
            maleficium_core::compile::JobStatus::Failed
            | maleficium_core::compile::JobStatus::TimedOut => {
                let tail: Vec<&String> = record
                    .lines
                    .iter()
                    .filter(|l| !l.starts_with("note:"))
                    .rev()
                    .take(15)
                    .collect();
                for line in tail.into_iter().rev() {
                    eprintln!("{line}");
                }
                std::process::exit(1);
            }
            maleficium_core::compile::JobStatus::Cancelled => {
                std::process::exit(1);
            }
            maleficium_core::compile::JobStatus::Running => {
                // Should not happen here since we check for Running above
                continue;
            }
        }
    }

    // Export the bundle
    let profile = if single_file {
        BundleProfile::SingleFile
    } else {
        BundleProfile::Folder
    };

    eprintln!("exporting bundle to {}", out_dir.to_string_lossy());
    let result = maleficium_core::bundle::export_bundle(
        &cx,
        root_id,
        &main_rel,
        &out_dir.to_string_lossy(),
        profile,
        None, // size_cap_bytes: use default
    );

    match result {
        Ok(bundle) => {
            // Print warnings to stderr
            for warning in &bundle.warnings {
                let kind = serde_json::to_value(warning.kind)?;
                eprintln!(
                    "{}: {}",
                    kind.as_str().unwrap_or("warning"),
                    warning.message
                );
            }
            if deny_warnings && !bundle.warnings.is_empty() {
                std::process::exit(3);
            }
            eprintln!(
                "export succeeded: {} ({} bytes, {} widgets, {} assets)",
                bundle.path, bundle.bytes, bundle.widgets, bundle.assets
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("export failed: {}", e);
            Err(anyhow::anyhow!("export failed: {}", e))
        }
    }
}

fn usage_error(msg: &str) -> anyhow::Error {
    eprintln!(
        "usage: maleficium export <project-dir> --main <rel-tex> --out <dest> [--single-file] [--deny-warnings]"
    );
    eprintln!("error: {}", msg);
    std::process::exit(2);
}
