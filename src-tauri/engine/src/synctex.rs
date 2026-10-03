//! `synctex view -i line:col:input -o output [-d dir]` and
//! `synctex edit -o page:x:y:output [-d dir]`: forward and inverse SyncTeX
//! queries over the vendored parser, printing the records the app reads
//! (`Output:`, `Page:`, `x:`, `y:`, `h:`, `v:`, `W:`, `H:`; `Input:`, `Line:`,
//! `Column:`) between `SyncTeX result begin` and `SyncTeX result end`. A query
//! with no match prints nothing. Unlike upstream's command line tool this has
//! no `-x` option, so it never runs another program.

use std::ffi::{c_char, c_float, c_int, c_void, CStr, CString, OsString};

use libz_sys as _;

type Scanner = *mut c_void;
type Node = *mut c_void;

extern "C" {
    fn synctex_scanner_new_with_output_file(
        output: *const c_char,
        build_directory: *const c_char,
        parse: c_int,
    ) -> Scanner;
    fn synctex_scanner_free(scanner: Scanner) -> c_int;
    fn synctex_display_query(
        scanner: Scanner,
        input: *const c_char,
        line: c_int,
        column: c_int,
        page_hint: c_int,
    ) -> c_int;
    fn synctex_edit_query(scanner: Scanner, page: c_int, h: c_float, v: c_float) -> c_int;
    fn synctex_scanner_next_result(scanner: Scanner) -> Node;
    fn synctex_scanner_get_name(scanner: Scanner, tag: c_int) -> *const c_char;
    fn synctex_node_page(node: Node) -> c_int;
    fn synctex_node_tag(node: Node) -> c_int;
    fn synctex_node_line(node: Node) -> c_int;
    fn synctex_node_column(node: Node) -> c_int;
    fn synctex_node_visible_h(node: Node) -> c_float;
    fn synctex_node_visible_v(node: Node) -> c_float;
    fn synctex_node_box_visible_h(node: Node) -> c_float;
    fn synctex_node_box_visible_v(node: Node) -> c_float;
    fn synctex_node_box_visible_width(node: Node) -> c_float;
    fn synctex_node_box_visible_height(node: Node) -> c_float;
    fn synctex_node_box_visible_depth(node: Node) -> c_float;
}

pub fn run(args: Vec<OsString>) -> i32 {
    match execute(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

/// Leading integer (optional sign) and the rest after it.
fn take_int(s: &str) -> Option<(i32, &str)> {
    let end = s
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+'))))
        .map_or(s.len(), |(i, _)| i);
    Some((s[..end].parse().ok()?, &s[end..]))
}

/// Leading decimal number and the rest after it.
fn take_float(s: &str) -> Option<(f32, &str)> {
    let end = s
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || (i == 0 && (c == '-' || c == '+'))))
        .map_or(s.len(), |(i, _)| i);
    Some((s[..end].parse().ok()?, &s[end..]))
}

/// `line:column[:page]:input`; the page is optional, and the input may itself
/// hold colons (a Windows drive).
fn parse_view_input(arg: &str) -> Result<(i32, i32, i32, String), String> {
    let bad = || format!("bad -i argument `{arg}`");
    let (line, rest) = take_int(arg).ok_or_else(bad)?;
    let rest = rest.strip_prefix(':').ok_or_else(bad)?;
    let (column, rest) = match take_int(rest) {
        Some((c, r)) if c >= 0 => (c, r),
        _ => (0, rest),
    };
    let rest = rest.strip_prefix(':').ok_or_else(bad)?;
    let (page, input) = match take_int(rest) {
        Some((p, r)) if r.starts_with(':') => (p, &r[1..]),
        _ => (0, rest),
    };
    let input = input
        .strip_prefix('"')
        .and_then(|i| i.strip_suffix('"'))
        .unwrap_or(input);
    Ok((line, column, page, input.to_string()))
}

/// `page:x:y:output`.
fn parse_edit_output(arg: &str) -> Result<(i32, f32, f32, String), String> {
    let bad = || format!("bad -o argument `{arg}`");
    let (page, rest) = take_int(arg).ok_or_else(bad)?;
    let rest = rest.strip_prefix(':').ok_or_else(bad)?;
    let (x, rest) = take_float(rest).ok_or_else(bad)?;
    let rest = rest.strip_prefix(':').ok_or_else(bad)?;
    let (y, rest) = take_float(rest).ok_or_else(bad)?;
    let output = rest.strip_prefix(':').ok_or_else(bad)?;
    Ok((page, x, y, output.to_string()))
}

fn cstring(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|_| format!("`{s}` holds a NUL byte"))
}

fn execute(args: Vec<OsString>) -> Result<(), String> {
    let mut args = args.into_iter().map(|a| a.to_string_lossy().into_owned());
    let command = args.next().ok_or("usage: synctex view|edit ...")?;
    let mut input = None;
    let mut output = None;
    let mut directory = None;
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "-i" => input = Some(value),
            "-o" => output = Some(value),
            "-d" => directory = Some(value),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    let output = output.ok_or("missing -o")?;
    match command.as_str() {
        "view" => {
            let (line, column, page, name) = parse_view_input(&input.ok_or("missing -i")?)?;
            query(
                &output,
                directory,
                |scanner| {
                    let name = cstring(&name)?;
                    // SAFETY: `scanner` is a live scanner and `name` a C string.
                    let n = unsafe {
                        synctex_display_query(scanner, name.as_ptr(), line, column, page)
                    };
                    Ok(n > 0)
                },
                view_record,
            )
        }
        "edit" => {
            let (page, x, y, file) = parse_edit_output(&output)?;
            query(
                &file,
                directory,
                |scanner| {
                    // SAFETY: `scanner` is a live scanner.
                    let n = unsafe { synctex_edit_query(scanner, page, x, y) };
                    Ok(n > 0)
                },
                edit_record,
            )
        }
        other => Err(format!("unknown synctex command {other}")),
    }
}

/// Open the scanner for `output`, run `ask`, and print every result `record`
/// renders. No result, no output.
fn query(
    output: &str,
    directory: Option<String>,
    ask: impl FnOnce(Scanner) -> Result<bool, String>,
    record: impl Fn(Scanner, Node, &str) -> Option<String>,
) -> Result<(), String> {
    let out_c = cstring(output)?;
    let dir_c = directory.as_deref().map(cstring).transpose()?;
    // SAFETY: both pointers are NUL terminated and outlive the call.
    let scanner = unsafe {
        synctex_scanner_new_with_output_file(
            out_c.as_ptr(),
            dir_c.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
            1,
        )
    };
    if scanner.is_null() {
        return Ok(());
    }
    let result = ask(scanner).map(|hit| {
        if !hit {
            return;
        }
        let mut text = String::new();
        loop {
            // SAFETY: `scanner` is live; null ends the results.
            let node = unsafe { synctex_scanner_next_result(scanner) };
            if node.is_null() {
                break;
            }
            if let Some(r) = record(scanner, node, output) {
                text.push_str(&r);
            }
        }
        if !text.is_empty() {
            print!("SyncTeX result begin\n{text}SyncTeX result end\n");
        }
    });
    // SAFETY: `scanner` came from the constructor and is not used again.
    unsafe { synctex_scanner_free(scanner) };
    result
}

fn view_record(_scanner: Scanner, node: Node, output: &str) -> Option<String> {
    // SAFETY: `node` is a live result node of the scanner.
    let (page, x, y, h, v, w, hh) = unsafe {
        let depth = synctex_node_box_visible_depth(node);
        (
            synctex_node_page(node),
            synctex_node_visible_h(node),
            synctex_node_visible_v(node),
            synctex_node_box_visible_h(node),
            synctex_node_box_visible_v(node) + depth,
            synctex_node_box_visible_width(node),
            synctex_node_box_visible_height(node) + depth,
        )
    };
    Some(format!(
        "Output:{output}\nPage:{page}\nx:{x:.6}\ny:{y:.6}\nh:{h:.6}\nv:{v:.6}\nW:{w:.6}\nH:{hh:.6}\n"
    ))
}

fn edit_record(scanner: Scanner, node: Node, output: &str) -> Option<String> {
    // SAFETY: `node` is a live result node; the name pointer is owned by the
    // scanner and copied before the scanner is freed.
    let (input, line, column) = unsafe {
        let name = synctex_scanner_get_name(scanner, synctex_node_tag(node));
        if name.is_null() {
            return None;
        }
        (
            CStr::from_ptr(name).to_string_lossy().into_owned(),
            synctex_node_line(node),
            synctex_node_column(node),
        )
    };
    Some(format!(
        "Output:{output}\nInput:{input}\nLine:{line}\nColumn:{column}\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_input_takes_line_column_and_a_path() {
        assert_eq!(
            parse_view_input("5:1:/tmp/a/smoke.tex").unwrap(),
            (5, 1, 0, "/tmp/a/smoke.tex".to_string())
        );
        assert_eq!(
            parse_view_input(r"7:0:C:\a\main.tex").unwrap(),
            (7, 0, 0, r"C:\a\main.tex".to_string())
        );
        assert_eq!(
            parse_view_input("7:0:3:main.tex").unwrap(),
            (7, 0, 3, "main.tex".to_string())
        );
        assert!(parse_view_input("main.tex").is_err());
    }

    #[test]
    fn edit_output_takes_page_position_and_a_file() {
        assert_eq!(
            parse_edit_output("1:100.5:200:smoke.pdf").unwrap(),
            (1, 100.5, 200.0, "smoke.pdf".to_string())
        );
        assert!(parse_edit_output("1:100").is_err());
    }
}
