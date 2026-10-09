//! Custom widget runtimes: the package an author installs under
//! `<project>/runtimes/<name>@<major>/`, its `runtime.json`, the checks a
//! package passes before it can be approved, and the binding of one
//! widget's sidecar record to the runtime it names.
//!
//! Nothing here reads the disk: the approval snapshot
//! ([`crate::widget_approval::snapshot_runtime`]) reads a package folder
//! once and hands its files to [`check_package`], and the exporter binds
//! each widget with [`bind`] against the manifest of that same snapshot.

pub mod scan;

use crate::widgets::{Widget, WidgetSource};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// The project folder that holds installed runtimes.
pub const RUNTIMES_DIR: &str = "runtimes";
/// A package's manifest, at the top of its folder.
pub const MANIFEST: &str = "runtime.json";
/// The sidecar role the macro's mandatory argument is recorded under; bind
/// maps it to the runtime's primary role.
pub const PRIMARY: &str = "primary";
/// The only `runtime.json` contract this build reads.
pub const CONTRACT: u64 = 1;

/// Runtime names a package may not take: the built-in kinds and `custom`.
pub const RESERVED_NAMES: [&str; 6] = ["model", "video", "table", "chart", "html", "custom"];
/// Name prefixes the package keeps for itself.
pub const RESERVED_PREFIXES: [&str; 2] = ["m-", "maleficium"];
/// Option keys a runtime may not declare: every key the macros take, the
/// sidecar's own `sources` and `primary`, and the poster parameters.
pub const RESERVED_OPTIONS: [&str; 19] = [
    "runtime",
    "poster",
    "height",
    "width",
    "id",
    "alt",
    "sources",
    "primary",
    "inline",
    "remote",
    "sha256",
    "pdfrows",
    "data",
    "camera",
    "size",
    "background",
    "scale",
    "framedomains",
    "resourcedomains",
];
/// Licences a runtime and every library it vendors may carry.
pub const LICENSES: [&str; 10] = [
    "MIT",
    "MIT-0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "Apache-2.0",
    "ISC",
    "Zlib",
    "0BSD",
    "CC0-1.0",
    "Unlicense",
];
/// A source role's size cap when its manifest gives none, and the largest
/// it may give.
pub const DEFAULT_MAX_BYTES: u64 = 104_857_600;
pub const MAX_MAX_BYTES: u64 = 536_870_912;

const MAX_ROLES: usize = 8;
const MAX_EXTENSIONS: usize = 16;
const MAX_OPTIONS: usize = 32;
const MAX_ENUM: usize = 32;
const MAX_STRING: usize = 200;
const MAX_AUTHORS: usize = 16;
const MAX_VENDORED: usize = 64;
const MAX_VENDORED_FILES: usize = 256;
const VENDOR_DIR: &str = "vendor/";
const SAMPLES_DIR: &str = "samples/";

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("a static pattern compiles"))
}

macro_rules! pattern {
    ($name:ident, $p:expr) => {
        fn $name() -> &'static Regex {
            static CELL: OnceLock<Regex> = OnceLock::new();
            re(&CELL, $p)
        }
    };
}

pattern!(name_re, r"^[a-z][a-z0-9-]{1,39}$");
pattern!(ref_re, r"^([a-z][a-z0-9-]{1,39})@([1-9][0-9]{0,3})$");
pattern!(
    version_re,
    r"^([1-9][0-9]{0,3})\.(0|[1-9][0-9]{0,3})\.(0|[1-9][0-9]{0,3})$"
);
pattern!(role_re, r"^[a-z][a-z0-9]{0,15}$");
pattern!(key_re, r"^[a-z][a-z0-9]{0,23}$");
pattern!(ext_re, r"^[a-z0-9]{1,10}$");
pattern!(path_re, r"^[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)*$");
pattern!(vendored_name_re, r"^[a-z0-9@][a-z0-9._/@-]{0,213}$");
pattern!(vendored_version_re, r"^[0-9A-Za-z.+-]{1,64}$");
pattern!(string_value_re, r"^[^|,{}#\\%]{0,200}$");
pattern!(integer_value_re, r"^-?(0|[1-9][0-9]{0,14})$");
pattern!(
    number_value_re,
    r"^-?(0|[1-9][0-9]{0,14})(\.[0-9]{1,15})?([eE][+-]?[0-9]{1,3})?$"
);

/// A runtime name: the name pattern, not reserved.
pub fn valid_name(n: &str) -> bool {
    name_re().is_match(n)
        && !RESERVED_NAMES.contains(&n)
        && !RESERVED_PREFIXES.iter().any(|p| n.starts_with(p))
}

/// `<name>@<major>` with a valid name and a major of 1 to 9999.
pub fn valid_ref(r: &str) -> bool {
    split_ref(r).is_some()
}

/// The name and major of a valid ref.
pub fn split_ref(r: &str) -> Option<(&str, u32)> {
    let c = ref_re().captures(r)?;
    let name = c.get(1)?.as_str();
    let major = c.get(2)?.as_str().parse().ok()?;
    valid_name(name).then_some((name, major))
}

/// A source role name: plain, and not the sidecar's reserved `primary`.
pub fn valid_role(r: &str) -> bool {
    role_re().is_match(r) && r != PRIMARY
}

/// An option key's shape (the reserved keys are refused by the manifest).
pub fn valid_option_key(k: &str) -> bool {
    key_re().is_match(k)
}

/// A relative path inside a package: plain segments, never `.` or `..`.
pub fn valid_path(p: &str) -> bool {
    path_re().is_match(p) && p.split('/').all(|s| s != "." && s != "..")
}

fn allowed_license(l: &str) -> bool {
    LICENSES.contains(&l)
}

fn license_list() -> String {
    LICENSES.join(", ")
}

fn plain_text(s: &str, min: usize, max: usize) -> bool {
    let n = s.chars().count();
    n >= min && n <= max && !s.chars().any(char::is_control)
}

// ---- runtime.json -----------------------------------------------------------

/// One source role a runtime accepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRole {
    #[serde(default)]
    pub primary: bool,
    pub required: bool,
    pub extensions: Vec<String>,
    #[serde(default = "default_max_bytes")]
    pub max_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_max_bytes() -> u64 {
    DEFAULT_MAX_BYTES
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionType {
    String,
    Number,
    Integer,
    Boolean,
}

impl OptionType {
    fn name(self) -> &'static str {
        match self {
            Self::String => "a string",
            Self::Number => "a number",
            Self::Integer => "an integer",
            Self::Boolean => "a boolean",
        }
    }
}

/// One option a runtime declares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptionSpec {
    #[serde(rename = "type")]
    pub kind: OptionType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, rename = "enum", skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
}

/// The `options` object: a closed JSON-Schema-shaped object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptionsSchema {
    #[serde(rename = "type")]
    pub kind: String,
    pub additional_properties: bool,
    pub properties: BTreeMap<String, OptionSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub webgl: bool,
    /// Whether the runtime may compile WebAssembly: contract 1 packages
    /// omit it (they never use WASM), so it defaults to false.
    #[serde(default)]
    pub wasm: bool,
}

/// A library a package carries a copy of under `vendor/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Vendored {
    pub name: String,
    pub version: String,
    pub license: String,
    /// Where the copy came from: a claim shown to the user, never fetched.
    pub source: String,
    pub files: Vec<String>,
    pub license_file: String,
}

/// A checked `runtime.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifest {
    pub name: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub authors: Vec<String>,
    pub license: String,
    pub sources: BTreeMap<String, SourceRole>,
    pub options: Option<OptionsSchema>,
    pub capabilities: Capabilities,
    pub vendored: Vec<Vendored>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestFile {
    #[allow(dead_code)]
    contract: u64,
    name: String,
    version: String,
    title: String,
    description: String,
    authors: Vec<String>,
    license: String,
    sources: BTreeMap<String, SourceRole>,
    #[serde(default)]
    options: Option<OptionsSchema>,
    capabilities: Capabilities,
    vendored: Vec<Vendored>,
}

impl RuntimeManifest {
    /// The role the macro's mandatory argument fills.
    pub fn primary_role(&self) -> &str {
        self.sources
            .iter()
            .find(|(_, r)| r.primary)
            .map_or("", |(k, _)| k.as_str())
    }

    /// Every file a vendored entry names, its licence file included.
    pub fn vendored_files(&self) -> BTreeSet<String> {
        self.vendored
            .iter()
            .flat_map(|v| v.files.iter().chain(std::iter::once(&v.license_file)))
            .cloned()
            .collect()
    }

    /// Package files that are metadata, never folded: the manifest, the
    /// readme and licence, the samples and every vendored licence file.
    pub fn is_metadata(&self, path: &str) -> bool {
        matches!(path, MANIFEST | "README.md" | "LICENSE")
            || path.starts_with(SAMPLES_DIR)
            || self.vendored.iter().any(|v| v.license_file == path)
    }
}

/// `options.<k>`'s rules, the default included.
fn check_option(k: &str, o: &OptionSpec) -> Result<(), String> {
    let bad = |rule: &str| Err(format!("options.{k}: {rule}"));
    if let Some(d) = &o.description {
        if !plain_text(d, 0, MAX_STRING) {
            return bad("description must be at most 200 characters, no control characters");
        }
    }
    let numeric = matches!(o.kind, OptionType::Number | OptionType::Integer);
    if o.choices.is_some() && o.kind != OptionType::String {
        return bad("enum applies to string options only");
    }
    if o.max_length.is_some() && o.kind != OptionType::String {
        return bad("maxLength applies to string options only");
    }
    if (o.minimum.is_some() || o.maximum.is_some()) && !numeric {
        return bad("minimum and maximum apply to number and integer options only");
    }
    if let Some(n) = o.max_length {
        if !(1..=MAX_STRING).contains(&n) {
            return bad("maxLength must be 1 to 200");
        }
    }
    for v in [o.minimum, o.maximum].into_iter().flatten() {
        if !v.is_finite() || (o.kind == OptionType::Integer && v.fract() != 0.0) {
            return bad("minimum and maximum must be finite, and whole for an integer");
        }
    }
    if let (Some(lo), Some(hi)) = (o.minimum, o.maximum) {
        if lo > hi {
            return bad("minimum is above maximum");
        }
    }
    if let Some(c) = &o.choices {
        if c.is_empty() || c.len() > MAX_ENUM {
            return bad("enum must list 1 to 32 values");
        }
        let distinct: BTreeSet<&String> = c.iter().collect();
        if distinct.len() != c.len() {
            return bad("enum lists a value twice");
        }
        if let Some(v) = c.iter().find(|v| !string_ok(v)) {
            return Err(format!(
                "options.{k}: enum value `{v}` is not a value a document can write"
            ));
        }
    }
    if let Some(d) = &o.default {
        let raw = match d {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        check_value(k, o, d, &raw)
            .map_err(|e| format!("options.{k}: default does not validate: {e}"))?;
    }
    Ok(())
}

fn string_ok(s: &str) -> bool {
    string_value_re().is_match(s) && !s.chars().any(char::is_control)
}

fn shown(v: f64) -> String {
    format!("{v}")
}

/// The type, range, enum and length rules on a typed value (the default,
/// or a converted sidecar value). Messages name the key and the value as
/// written (`raw`).
fn check_value(k: &str, o: &OptionSpec, v: &Value, raw: &str) -> Result<(), String> {
    let text = raw;
    match (o.kind, v) {
        (OptionType::String, Value::String(s)) => {
            if !string_ok(s) {
                return Err(format!("option {k}=`{s}` is not a string"));
            }
            let max = o.max_length.unwrap_or(MAX_STRING);
            if s.chars().count() > max {
                return Err(format!("option {k} is longer than {max} characters"));
            }
            if let Some(c) = &o.choices {
                if !c.iter().any(|x| x == s) {
                    return Err(format!("option {k}={s} is not one of {}", c.join(", ")));
                }
            }
        }
        (OptionType::Boolean, Value::Bool(_)) => {}
        (OptionType::Integer, Value::Number(n)) if n.is_i64() => {}
        (OptionType::Number, Value::Number(n)) if n.as_f64().is_some_and(f64::is_finite) => {}
        _ => return Err(format!("option {k}=`{text}` is not {}", o.kind.name())),
    }
    if let Some(x) = v.as_f64() {
        if let Some(lo) = o.minimum.filter(|lo| x < *lo) {
            return Err(format!("option {k}={text} is below {}", shown(lo)));
        }
        if let Some(hi) = o.maximum.filter(|hi| x > *hi) {
            return Err(format!("option {k}={text} is above {}", shown(hi)));
        }
    }
    Ok(())
}

/// A sidecar option value (a string the document wrote) as the typed JSON
/// value its declaration asks for, every rule checked.
fn convert(k: &str, o: &OptionSpec, raw: &str) -> Result<Value, String> {
    let not = || format!("option {k}=`{raw}` is not {}", o.kind.name());
    let v = match o.kind {
        OptionType::String => {
            if !string_ok(raw) {
                return Err(not());
            }
            Value::String(raw.to_string())
        }
        OptionType::Boolean => match raw {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => return Err(not()),
        },
        OptionType::Integer => {
            if !integer_value_re().is_match(raw) {
                return Err(not());
            }
            Value::from(raw.parse::<i64>().map_err(|_| not())?)
        }
        OptionType::Number => {
            if !number_value_re().is_match(raw) {
                return Err(not());
            }
            let f: f64 = raw.parse().map_err(|_| not())?;
            Value::Number(serde_json::Number::from_f64(f).ok_or_else(not)?)
        }
    };
    check_value(k, o, &v, raw)?;
    Ok(v)
}

/// Parse and check one `runtime.json` of the package `reference`
/// (`<name>@<major>`, its folder name). Every message is prefixed
/// `runtime <ref>: `.
pub fn parse_manifest(reference: &str, text: &str) -> Result<RuntimeManifest, String> {
    parse_inner(reference, text).map_err(|e| format!("runtime {reference}: {e}"))
}

fn parse_inner(reference: &str, text: &str) -> Result<RuntimeManifest, String> {
    if text.len() > crate::widgets::MAX_MANIFEST_BYTES {
        return Err(format!("{MANIFEST} is larger than 64 KiB"));
    }
    let (folder_name, major) =
        split_ref(reference).ok_or_else(|| format!("`{reference}` is not <name>@<major>"))?;
    let raw: Value =
        serde_json::from_str(text).map_err(|e| format!("{MANIFEST} is not valid JSON: {e}"))?;
    if let Some(c) = raw.get("contract") {
        if c.as_u64() != Some(CONTRACT) {
            return Err(format!("contract must be 1, found {c}"));
        }
    }
    let f: ManifestFile = serde_json::from_value(raw).map_err(|e| format!("{MANIFEST}: {e}"))?;
    if f.name != folder_name {
        return Err(format!("name `{}` does not match its folder", f.name));
    }
    let version_major = version_re()
        .captures(&f.version)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u32>().ok());
    if version_major != Some(major) {
        return Err(format!(
            "version `{}` is not MAJOR.MINOR.PATCH with major {major}",
            f.version
        ));
    }
    if !plain_text(&f.title, 1, 80) {
        return Err("title must be 1 to 80 characters, no control characters".to_string());
    }
    if !plain_text(&f.description, 0, 500) {
        return Err(
            "description must be at most 500 characters, no control characters".to_string(),
        );
    }
    if f.authors.is_empty() || f.authors.len() > MAX_AUTHORS {
        return Err("authors must list 1 to 16 names".to_string());
    }
    if f.authors
        .iter()
        .any(|a| !(1..=100).contains(&a.chars().count()))
    {
        return Err("authors: each name must be 1 to 100 characters".to_string());
    }
    if !allowed_license(&f.license) {
        return Err(format!(
            "license `{}` is not on the allowlist ({})",
            f.license,
            license_list()
        ));
    }
    if f.sources.is_empty() || f.sources.len() > MAX_ROLES {
        return Err("sources: a runtime takes 1 to 8 roles".to_string());
    }
    for (r, s) in &f.sources {
        if !valid_role(r) {
            return Err(format!("sources: `{r}` is not a role name"));
        }
        if s.extensions.is_empty() || s.extensions.len() > MAX_EXTENSIONS {
            return Err(format!("sources.{r}: extensions must list 1 to 16 values"));
        }
        if let Some(e) = s.extensions.iter().find(|e| !ext_re().is_match(e)) {
            return Err(format!("sources.{r}: `{e}` is not a plain extension"));
        }
        if !(1..=MAX_MAX_BYTES).contains(&s.max_bytes) {
            return Err(format!(
                "sources.{r}: maxBytes must be 1 to {MAX_MAX_BYTES}"
            ));
        }
        if let Some(d) = &s.description {
            if !plain_text(d, 0, MAX_STRING) {
                return Err(format!(
                    "sources.{r}: description must be at most 200 characters, no control characters"
                ));
            }
        }
    }
    let primaries: Vec<&String> = f
        .sources
        .iter()
        .filter(|(_, s)| s.primary)
        .map(|(k, _)| k)
        .collect();
    if primaries.len() != 1 {
        return Err(format!(
            "sources: exactly one role must be primary, found {}",
            primaries.len()
        ));
    }
    if !f.sources[primaries[0]].required {
        return Err(format!(
            "sources: the primary role {} must be required",
            primaries[0]
        ));
    }
    if let Some(o) = &f.options {
        if o.kind != "object" {
            return Err("options: type must be \"object\"".to_string());
        }
        if o.additional_properties {
            return Err("options: additionalProperties must be false".to_string());
        }
        if o.properties.len() > MAX_OPTIONS {
            return Err("options: at most 32 properties".to_string());
        }
        for (k, spec) in &o.properties {
            if !valid_option_key(k) || RESERVED_OPTIONS.contains(&k.as_str()) {
                return Err(format!("options: `{k}` is not an allowed option name"));
            }
            check_option(k, spec)?;
        }
    }
    if f.vendored.len() > MAX_VENDORED {
        return Err("vendored: at most 64 entries".to_string());
    }
    for v in &f.vendored {
        let n = &v.name;
        if !vendored_name_re().is_match(n) {
            return Err(format!("vendored `{n}` is not a plain library name"));
        }
        if !vendored_version_re().is_match(&v.version) {
            return Err(format!(
                "vendored {n}: version `{}` is not plain",
                v.version
            ));
        }
        if !allowed_license(&v.license) {
            return Err(format!(
                "vendored {n}: license `{}` is not on the allowlist",
                v.license
            ));
        }
        if v.source.chars().count() > 2048 {
            return Err(format!(
                "vendored {n}: source is longer than 2048 characters"
            ));
        }
        if v.files.is_empty() || v.files.len() > MAX_VENDORED_FILES {
            return Err(format!("vendored {n}: files must list 1 to 256 paths"));
        }
        for p in v.files.iter().chain(std::iter::once(&v.license_file)) {
            if !valid_path(p) {
                return Err(format!("path `{p}` is not a plain relative path"));
            }
            if !p.starts_with(VENDOR_DIR) {
                return Err(format!("vendored {n}: {p} is not under {VENDOR_DIR}"));
            }
        }
    }
    Ok(RuntimeManifest {
        name: f.name,
        version: f.version,
        title: f.title,
        description: f.description,
        authors: f.authors,
        license: f.license,
        sources: f.sources,
        options: f.options,
        capabilities: f.capabilities,
        vendored: f.vendored,
    })
}

fn extension(path: &str) -> Option<String> {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| !e.is_empty())
}

/// The whole package (every file of its folder, by `/`-separated relative
/// path) checked: its manifest, the folder name, the entry, the vendored
/// files and licences, and a sample per required role.
pub fn check_package(
    reference: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<RuntimeManifest, String> {
    let fail = |e: String| format!("runtime {reference}: {e}");
    if let Some(p) = files.keys().find(|p| !valid_path(p)) {
        return Err(fail(format!("path `{p}` is not a plain relative path")));
    }
    let bytes = files
        .get(MANIFEST)
        .ok_or_else(|| fail(format!("{MANIFEST} is missing")))?;
    if bytes.len() > crate::widgets::MAX_MANIFEST_BYTES {
        return Err(fail(format!("{MANIFEST} is larger than 64 KiB")));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|e| fail(format!("{MANIFEST} is not valid JSON: {e}")))?;
    let m = parse_manifest(reference, text)?;
    if !files.contains_key(crate::bundle::fold::ENTRY) {
        return Err(fail(format!("{} is missing", crate::bundle::fold::ENTRY)));
    }
    for v in &m.vendored {
        for p in v.files.iter().chain(std::iter::once(&v.license_file)) {
            if !files.contains_key(p) {
                return Err(fail(format!("vendored {}: {p} is missing", v.name)));
            }
        }
    }
    for (role, s) in m.sources.iter().filter(|(_, s)| s.required) {
        let found = files.keys().any(|p| {
            p.starts_with(SAMPLES_DIR) && extension(p).is_some_and(|e| s.extensions.contains(&e))
        });
        if !found {
            let exts: Vec<String> = s.extensions.iter().map(|e| format!(".{e}")).collect();
            return Err(fail(format!(
                "samples/ has no {} file for required role {role}",
                exts.join(" or ")
            )));
        }
    }
    Ok(m)
}

// ---- binding a widget -------------------------------------------------------

/// One widget bound to its runtime: its sources under the runtime's role
/// names, and its options as typed, defaulted JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    pub sources: Vec<(String, WidgetSource)>,
    pub options: Map<String, Value>,
}

/// Layout keys the macro records with the options; never runtime options.
const LAYOUT_KEYS: [&str; 2] = ["height", "width"];

/// Check `w`'s sidecar record against the manifest of the runtime it names:
/// the recorded `primary` becomes the primary role, every other role must
/// be declared and given once with an accepted extension, every required
/// role must be given, and every option must be declared and convert to
/// its type. Messages are prefixed `widget <id>: `.
pub fn bind(w: &Widget, m: &RuntimeManifest) -> Result<Bound, String> {
    let id = &w.id;
    let r = w.runtime.as_deref().unwrap_or("");
    let fail = |e: String| format!("widget {id}: {e}");
    let mut sources: Vec<(String, WidgetSource)> = Vec::new();
    for s in &w.sources {
        let role = if s.role == PRIMARY {
            m.primary_role().to_string()
        } else {
            s.role.clone()
        };
        let Some(spec) = m
            .sources
            .get(&role)
            .filter(|_| s.role == PRIMARY || valid_role(&role))
        else {
            return Err(fail(format!("runtime {r} has no source role `{role}`")));
        };
        if s.role != PRIMARY && spec.primary {
            return Err(fail(format!("source role `{role}` is given twice")));
        }
        if sources.iter().any(|(k, _)| *k == role) {
            return Err(fail(format!("source role `{role}` is given twice")));
        }
        let ext = extension(&s.path).unwrap_or_default();
        if !spec.extensions.contains(&ext) {
            return Err(fail(format!(
                "source {role}: `{}` has extension `.{ext}`; runtime {r} accepts {}",
                s.path,
                spec.extensions
                    .iter()
                    .map(|e| format!(".{e}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        sources.push((
            role.clone(),
            WidgetSource {
                role,
                path: s.path.clone(),
            },
        ));
    }
    for (role, _) in m.sources.iter().filter(|(_, s)| s.required) {
        if !sources.iter().any(|(k, _)| k == role) {
            return Err(fail(format!("runtime {r} requires source {role}")));
        }
    }
    let empty = BTreeMap::new();
    let declared = m.options.as_ref().map_or(&empty, |o| &o.properties);
    let mut options = Map::new();
    for o in w
        .options
        .iter()
        .filter(|o| !LAYOUT_KEYS.contains(&o.key.as_str()))
    {
        let Some(spec) = declared.get(&o.key) else {
            return Err(fail(format!("runtime {r} has no option `{}`", o.key)));
        };
        if options.contains_key(&o.key) {
            return Err(fail(format!("option {} is given twice", o.key)));
        }
        let v = convert(&o.key, spec, &o.value).map_err(fail)?;
        options.insert(o.key.clone(), v);
    }
    for (k, spec) in declared {
        if let (false, Some(d)) = (options.contains_key(k), &spec.default) {
            options.insert(k.clone(), d.clone());
        }
    }
    Ok(Bound { sources, options })
}

/// A bound source's size against its role's cap (the exporter knows the
/// size once it hashes the file).
pub fn check_size(w: &Widget, m: &RuntimeManifest, role: &str, bytes: u64) -> Result<(), String> {
    let r = w.runtime.as_deref().unwrap_or("");
    let max = m.sources.get(role).map_or(0, |s| s.max_bytes);
    if bytes > max {
        return Err(format!(
            "widget {}: source {role} is {bytes} bytes; runtime {r} accepts at most {max}",
            w.id
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
