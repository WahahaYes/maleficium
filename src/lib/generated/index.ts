// Generated from src-tauri/index (maleficium-index). Do not edit:
// change the Rust types, then run
//   MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace

export type Source = "disk" | "buffer";

export type Unindexed = "not-text" | "too-large" | "not-utf8" | "over-budget";

export type Loc = { rel: string, line: number, };

export type MacroDef = { rel: string, line: number, command: string, params: number | null, body: string, };

export type Query = { pattern: string, 
/**
 * Treat `pattern` as a regular expression (else literal text).
 */
regex: boolean, caseSensitive: boolean, 
/**
 * Only matches with no word character on either side.
 */
wholeWord: boolean, };

export type Hit = { 
/**
 * 1-based line the match starts on.
 */
line: number, 
/**
 * UTF-16 column of the match start within `preview`'s line.
 */
col: number, 
/**
 * UTF-16 length of the match on that line (clipped at the line end).
 */
len: number, 
/**
 * The line the match starts on, without its newline, capped at
 * `PREVIEW_MAX` bytes around the match.
 */
preview: string, 
/**
 * UTF-16 offset of `preview` within its line (non-zero when clipped).
 */
previewCol: number, };

export type FileHits = { rel: string, source: Source, revision: string, hits: Array<Hit>, };

export type SearchResult = { files: Array<FileHits>, 
/**
 * Hits returned across `files`.
 */
hits: number, 
/**
 * Hits past the cap, not returned.
 */
truncated: number, 
/**
 * Files whose text was searched.
 */
searched: number, 
/**
 * Listed files with no text to search (binary, too large, …).
 */
unsearched: number, };

export type FileMatch = { rel: string, score: number, 
/**
 * UTF-16 positions in `rel` of the query's characters, for highlighting.
 */
positions: Array<number>, };

export type Ranked = { 
/**
 * Position in the list the caller passed.
 */
index: number, score: number, 
/**
 * UTF-16 positions of the query's characters, for highlighting.
 */
positions: Array<number>, };

export type ReplaceHunk = { line: number, 
/**
 * UTF-16 column and length of the match on the line before.
 */
col: number, len: number, before: string, after: string, };

export type ReplaceFile = { rel: string, source: Source, 
/**
 * Revision of the text the plan was made from.
 */
revision: string, replacements: number, 
/**
 * The first hunks of this file (see `ReplacePreview.hunksTruncated`).
 */
hunks: Array<ReplaceHunk>, };

export type ReplacePreview = { 
/**
 * Pass back to apply; refused once any listed file has changed.
 */
token: string, files: Array<ReplaceFile>, replacements: number, 
/**
 * Hunks not carried in `files` (the replacements still happen).
 */
hunksTruncated: number, };

export type BufferEdit = { rel: string, text: string, };

export type ReplaceApplied = { 
/**
 * History batch holding every file's prior content: undo restores it.
 */
batch: string, 
/**
 * Files written on disk.
 */
written: Array<string>, 
/**
 * New text for files the caller keeps open, not written here.
 */
edits: Array<BufferEdit>, replacements: number, };
