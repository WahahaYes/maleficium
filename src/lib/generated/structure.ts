// Generated from src-tauri/structure (maleficium-structure). Do not edit:
// change the Rust types, then run
//   MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace

export type OutlineKind = "section" | "label" | "figure" | "table" | "input";

export type OutlineEntry = { level: number, title: string, line: number, kind: OutlineKind, 
/**
 * Marker detail: label key, float graphics file, or input path.
 */
detail?: string, };

export type Outline = { entries: Array<OutlineEntry>, 
/**
 * Rows dropped by the cap.
 */
truncated: number, };

export type Severity = "error" | "warning";

export type Diagnostic = { 
/**
 * Root-relative source path; `None` when `external`.
 */
path?: string, line: number, message: string, severity: Severity, 
/**
 * The source resolves outside the project root.
 */
external: boolean, };

export type FetchOutcome = "fetched" | "failed";

export type MissingReason = "not-cached" | "fetch-failed" | "not-in-bundle" | "bundle-unreachable" | "bundle-invalid" | "cache-empty" | "bundle-changed" | "system-font" | "external-tool" | "shell-escape-required";

export type MissingDependency = { file?: string, reason: MissingReason, };

export type CompilePhase = "first-compile" | "format" | "tex" | "bibliography" | "xdvipdfmx" | "writing";

export type LineSignal = { "kind": "fetch", file: string, outcome: FetchOutcome, } | { "kind": "phase", phase: CompilePhase, detail?: string, };
