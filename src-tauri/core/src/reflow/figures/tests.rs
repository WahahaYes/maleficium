use super::*;
use std::fs;

const PROBE: &str = include_str!("../fixtures/figures-probe.frag");
const EYE: &str = include_str!("../fixtures/figures-eye-gaze.frag");

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake-png-body";
const JPG: &[u8] = b"\xFF\xD8\xFF\xE0fake-jpeg";
const GIF: &[u8] = b"GIF89afake";
const WEBP: &[u8] = b"RIFF\x00\x00\x00\x00WEBPfake";
const SVG: &[u8] = b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>";

/// A fresh canonical project dir plus a sibling dir for outputs and outsiders.
fn project(name: &str) -> (PathBuf, PathBuf) {
    let base = crate::test_scratch::dir(&format!("figures-{name}"));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("proj");
    let other = base.join("other");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&other).unwrap();
    (
        dunce::canonicalize(&root).unwrap(),
        dunce::canonicalize(&other).unwrap(),
    )
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, bytes).unwrap();
}

/// What the converter must emit: the path in `data-graphic`.
fn img(graphic: &str) -> String {
    format!(
        "<img src=\"\" id=\"S1.F1.g1\" data-graphic=\"{}\" class=\"ltx_graphics ltx_centering ltx_missing ltx_missing_image\" alt=\"Refer to caption\">",
        escape(graphic)
    )
}

fn single(html: &str, root: &Path, opts: &Options) -> Embedded {
    embed(html, root, &Mode::SingleFile, opts).unwrap()
}

fn src_of(html: &str) -> String {
    parse_attrs(img_re().find(html).unwrap().as_str())
        .into_iter()
        .find(|(k, _)| k == "src")
        .unwrap()
        .1
}

fn only_warning(e: &Embedded) -> &FigureWarning {
    assert_eq!(e.warnings.len(), 1, "{:?}", e.warnings);
    &e.warnings[0]
}

fn assert_placeholder(e: &Embedded) {
    assert!(e.html.contains("ltx_missing_figure"), "{}", e.html);
    assert!(!e.html.contains("<img"), "{}", e.html);
}

#[test]
fn raster_formats_inline_as_data_uris() {
    let (root, _) = project("raster");
    for (name, bytes, mime) in [
        ("a.png", PNG, "image/png"),
        ("b.jpg", JPG, "image/jpeg"),
        ("c.jpeg", JPG, "image/jpeg"),
        ("d.gif", GIF, "image/gif"),
        ("e.webp", WEBP, "image/webp"),
        ("f.svg", SVG, "image/svg+xml"),
    ] {
        write(&root, &format!("figures/{name}"), bytes);
        let e = single(&img(&format!("figures/{name}")), &root, &Options::default());
        assert!(e.warnings.is_empty(), "{name}: {:?}", e.warnings);
        let src = src_of(&e.html);
        let prefix = format!("data:{mime};base64,");
        assert!(src.starts_with(&prefix), "{name}: {src}");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&src[prefix.len()..])
            .unwrap();
        assert_eq!(decoded, bytes, "{name}");
    }
}

#[test]
fn the_rewritten_tag_keeps_its_attributes_and_drops_the_bookkeeping() {
    let (root, _) = project("tag");
    write(&root, "a.png", PNG);
    let e = single(&img("a.png"), &root, &Options::default());
    assert!(e.html.contains("id=\"S1.F1.g1\""));
    assert!(e.html.contains("alt=\"Refer to caption\""));
    assert!(e.html.contains("class=\"ltx_graphics ltx_centering\""));
    assert!(!e.html.contains("data-graphic"));
    assert!(!e.html.contains("ltx_missing"));
}

#[test]
fn text_around_figures_and_other_images_is_untouched() {
    let (root, _) = project("around");
    write(&root, "a.png", PNG);
    let logo = "<img src=\"data:image/png;base64,AAAA\" alt=\"Mascot\">";
    let html = format!(
        "<p>before</p>{}<p>between</p>{logo}<p>after</p>",
        img("a.png")
    );
    let e = single(&html, &root, &Options::default());
    assert!(e.html.starts_with("<p>before</p><img src=\"data:image/png"));
    assert!(e.html.contains(
        "<p>between</p><img src=\"data:image/png;base64,AAAA\" alt=\"Mascot\"><p>after</p>"
    ));
}

#[test]
fn extensionless_references_resolve_like_latex() {
    let (root, _) = project("noext");
    write(&root, "figs/plot.png", PNG);
    write(&root, "figs/both.png", PNG);
    write(&root, "figs/both.jpg", JPG);
    write(&root, "figs/shout.PNG", PNG);
    write(&root, "figs/fig.v2.png", PNG);
    let o = Options::default();
    assert!(src_of(&single(&img("figs/plot"), &root, &o).html).starts_with("data:image/png"));
    // pdf, png, jpg order: png wins over jpg.
    assert!(src_of(&single(&img("figs/both"), &root, &o).html).starts_with("data:image/png"));
    assert!(src_of(&single(&img("figs/shout"), &root, &o).html).starts_with("data:image/png"));
    // An interior dot is not an extension.
    assert!(single(&img("figs/fig.v2"), &root, &o).warnings.is_empty());
}

#[test]
fn an_extensionless_pdf_is_found_and_rasterized() {
    let (root, _) = project("noext-pdf");
    write(&root, "plot.pdf", &crate::snippet::tests::tiny_pdf());
    let e = single(&img("plot"), &root, &Options::default());
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
    assert!(src_of(&e.html).starts_with("data:image/png;base64,"));
}

#[test]
fn graphicspath_is_searched_after_the_root_in_order() {
    let (root, _) = project("gpath");
    write(&root, "img/a.png", PNG);
    write(&root, "img2/a.png", JPG);
    write(&root, "img2/only.jpg", JPG);
    let o = Options {
        graphics_paths: vec!["./img/".into(), "img2".into()],
        ..Options::default()
    };
    let e = single(&img("a"), &root, &o);
    assert!(
        src_of(&e.html).starts_with("data:image/png"),
        "first entry wins"
    );
    let e = single(&img("only.jpg"), &root, &o);
    assert!(src_of(&e.html).starts_with("data:image/jpeg"));
    // The root itself comes before any graphicspath entry.
    write(&root, "a.gif", GIF);
    let e = single(&img("a"), &root, &o);
    assert!(
        src_of(&e.html).starts_with("data:image/gif"),
        "root before graphicspath"
    );
}

#[test]
fn a_hostile_graphicspath_entry_is_dropped_with_a_warning() {
    let (root, other) = project("gpath-bad");
    write(&other, "a.png", PNG);
    let o = Options {
        graphics_paths: vec!["../other/".into(), "/etc/".into()],
        ..Options::default()
    };
    let e = single(&img("a"), &root, &o);
    assert_eq!(e.warnings.len(), 3, "{:?}", e.warnings);
    assert_eq!(e.warnings[0].reason, FigureReason::Escapes);
    assert_eq!(e.warnings[1].reason, FigureReason::Escapes);
    assert_eq!(e.warnings[2].reason, FigureReason::Missing);
}

#[test]
fn pdf_figures_become_a_capped_png() {
    let (root, _) = project("pdf");
    write(&root, "plot.pdf", &crate::snippet::tests::tiny_pdf());
    let e = single(&img("plot.pdf"), &root, &Options::default());
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
    let src = src_of(&e.html);
    let png = base64::engine::general_purpose::STANDARD
        .decode(src.strip_prefix("data:image/png;base64,").unwrap())
        .unwrap();
    let n = |i: usize| u32::from_be_bytes(png[i..i + 4].try_into().unwrap());
    let (w, h) = (n(16), n(20));
    assert!(w.abs_diff(1275) <= 1 && h.abs_diff(1650) <= 1, "{w}x{h}");
    assert!(w.max(h) <= PDF_MAX_PX);
}

#[test]
fn an_unreadable_pdf_is_a_placeholder_and_a_warning() {
    let (root, _) = project("pdf-bad");
    write(&root, "bad.pdf", b"%PDF-1.4\nnot really");
    let e = single(&img("bad.pdf"), &root, &Options::default());
    assert_placeholder(&e);
    assert!(matches!(only_warning(&e).reason, FigureReason::Failed(_)));
}

#[test]
fn eps_is_a_placeholder_with_a_typed_warning() {
    let (root, _) = project("eps");
    write(&root, "d.eps", b"%!PS-Adobe-3.0 EPSF-3.0\n");
    for name in ["d.eps", "d"] {
        let e = single(&img(name), &root, &Options::default());
        assert_placeholder(&e);
        assert!(e.html.contains("EPS is not supported"));
        assert!(e.html.contains("id=\"S1.F1.g1\""));
        let w = only_warning(&e);
        assert_eq!(w.reason, FigureReason::Eps);
        assert_eq!(w.file, name);
    }
}

#[test]
fn unsupported_content_is_decided_by_bytes_not_by_name() {
    let (root, _) = project("sniff");
    write(&root, "notes.png", b"just text, not a png");
    write(&root, "real.dat", PNG);
    write(&root, "disguised.jpg", PNG);
    let e = single(&img("notes.png"), &root, &Options::default());
    assert_placeholder(&e);
    assert!(matches!(
        only_warning(&e).reason,
        FigureReason::Unsupported(_)
    ));
    // A recognized image under another name is shown as what it is.
    let e = single(&img("real.dat"), &root, &Options::default());
    assert!(src_of(&e.html).starts_with("data:image/png"));
    let e = single(&img("disguised.jpg"), &root, &Options::default());
    assert!(src_of(&e.html).starts_with("data:image/png"));
}

#[test]
fn a_missing_file_is_a_placeholder_and_a_warning() {
    let (root, _) = project("missing");
    let e = single(&img("nope.png"), &root, &Options::default());
    assert_placeholder(&e);
    assert!(e.html.contains("nope.png"));
    let w = only_warning(&e);
    assert_eq!(
        (w.file.as_str(), &w.reason),
        ("nope.png", &FigureReason::Missing)
    );
}

#[test]
fn a_directory_is_not_a_figure() {
    let (root, _) = project("isdir");
    fs::create_dir_all(root.join("figs.png")).unwrap();
    let e = single(&img("figs.png"), &root, &Options::default());
    assert_eq!(only_warning(&e).reason, FigureReason::Missing);
}

#[test]
fn absolute_paths_are_rejected_even_when_the_file_exists() {
    let (root, other) = project("abs");
    write(&other, "secret.png", PNG);
    let abs = other.join("secret.png").to_string_lossy().into_owned();
    for raw in [
        abs.as_str(),
        "/etc/passwd",
        "\\windows\\x.png",
        "C:\\x.png",
        "C:/x.png",
    ] {
        let e = single(&img(raw), &root, &Options::default());
        assert_placeholder(&e);
        assert_eq!(only_warning(&e).reason, FigureReason::Escapes, "{raw}");
    }
}

#[test]
fn dotdot_is_rejected_even_when_it_stays_inside() {
    let (root, other) = project("dotdot");
    write(&root, "a/b.png", PNG);
    write(&other, "out.png", PNG);
    for raw in [
        "../other/out.png",
        "a/../a/b.png",
        "a/..\\a/b.png",
        "figs/../../other/out.png",
        "..",
    ] {
        let e = single(&img(raw), &root, &Options::default());
        assert_placeholder(&e);
        assert_eq!(only_warning(&e).reason, FigureReason::Escapes, "{raw}");
    }
}

#[test]
fn empty_and_nul_references_are_rejected() {
    let (root, _) = project("nul");
    for raw in ["", "a\0b.png"] {
        let e = single(&img(raw), &root, &Options::default());
        assert_eq!(only_warning(&e).reason, FigureReason::Escapes);
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_leaving_the_root_is_rejected() {
    use std::os::unix::fs::symlink;
    let (root, other) = project("symlink");
    write(&other, "secret.png", PNG);
    write(&other, "dir/inner.png", PNG);
    symlink(other.join("secret.png"), root.join("link.png")).unwrap();
    symlink(other.join("dir"), root.join("linkdir")).unwrap();
    // A link to a file, a path through a linked directory, and a dangling link
    // are all kept out (the dangling one reads as missing).
    for raw in ["link.png", "linkdir/inner.png", "link"] {
        let e = single(&img(raw), &root, &Options::default());
        assert_placeholder(&e);
        assert_eq!(only_warning(&e).reason, FigureReason::Escapes, "{raw}");
        assert!(!e.html.contains("data:"), "{raw}");
    }
    symlink(root.join("gone.png"), root.join("dangling.png")).unwrap();
    let e = single(&img("dangling.png"), &root, &Options::default());
    assert_eq!(only_warning(&e).reason, FigureReason::Missing);
}

#[cfg(unix)]
#[test]
fn a_symlink_staying_inside_the_root_is_followed() {
    use std::os::unix::fs::symlink;
    let (root, _) = project("symlink-in");
    write(&root, "real/a.png", PNG);
    symlink(root.join("real/a.png"), root.join("alias.png")).unwrap();
    let e = single(&img("alias.png"), &root, &Options::default());
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
}

#[test]
fn remote_and_other_schemes_are_never_fetched_or_kept() {
    let (root, _) = project("remote");
    for (raw, reason) in [
        ("http://example.com/x.png", FigureReason::Remote),
        ("HTTPS://example.com/x.png", FigureReason::Remote),
        ("//example.com/x.png", FigureReason::Remote),
        ("file:///etc/passwd", FigureReason::Escapes),
        ("data:image/png;base64,AAAA", FigureReason::Escapes),
    ] {
        let html = format!(
            "<img src=\"{}\" data-graphic=\"{}\" class=\"ltx_graphics\">",
            escape(raw),
            escape(raw)
        );
        let e = single(&html, &root, &Options::default());
        assert_placeholder(&e);
        assert!(!e.html.contains("src="), "{raw}: {}", e.html);
        assert_eq!(only_warning(&e).reason, reason, "{raw}");
    }
}

#[test]
fn a_figure_with_no_recorded_path_never_keeps_its_src() {
    let (root, _) = project("nosrc");
    let html = "<img src=\"http://evil.example/x.png\" class=\"ltx_graphics\" alt=\"x\">";
    let e = single(html, &root, &Options::default());
    assert_placeholder(&e);
    assert!(!e.html.contains("evil.example"));
    assert_eq!(only_warning(&e).reason, FigureReason::NoSource);
}

#[test]
fn real_latexml_output_has_no_path_and_every_figure_is_flagged() {
    let (root, _) = project("real");
    let e = single(PROBE, &root, &Options::default());
    assert_eq!(e.warnings.len(), 8);
    assert!(e
        .warnings
        .iter()
        .all(|w| w.reason == FigureReason::NoSource));
    assert!(!e.html.contains("<img"));
    assert_eq!(e.html.matches("ltx_missing_figure").count(), 8);
    // Captions and structure survive.
    assert!(e.html.contains("png via graphicspath"));
    let e = single(EYE, &root, &Options::default());
    assert_eq!(e.warnings.len(), 1);
    assert!(e.html.contains("Illustration of eye tracking"));
}

#[test]
fn the_per_file_limit_is_enforced_by_size() {
    let (root, _) = project("cap-file");
    write(&root, "big.png", &[PNG, &[0u8; 600]].concat());
    let o = Options {
        max_file_bytes: 512,
        ..Options::default()
    };
    let e = single(&img("big.png"), &root, &o);
    assert_placeholder(&e);
    assert!(matches!(only_warning(&e).reason, FigureReason::TooLarge(n) if n > 512));
}

#[test]
fn the_total_limit_stops_further_figures_not_earlier_ones() {
    let (root, _) = project("cap-total");
    write(&root, "a.png", &[PNG, &[0u8; 300]].concat());
    write(&root, "b.png", &[PNG, &[1u8; 300]].concat());
    let o = Options {
        max_total_bytes: 500,
        ..Options::default()
    };
    let html = format!("{}{}", img("a.png"), img("b.png"));
    let e = single(&html, &root, &o);
    assert!(e.html.contains("data:image/png"));
    assert!(e.html.contains("ltx_missing_figure"));
    let w = only_warning(&e);
    assert_eq!(
        (w.file.as_str(), &w.reason),
        ("b.png", &FigureReason::TotalCap)
    );
}

#[test]
fn single_file_counts_every_inlined_copy_but_folder_counts_a_file_once() {
    let (root, other) = project("cap-dup");
    write(&root, "a.png", &[PNG, &[0u8; 300]].concat());
    let o = Options {
        max_total_bytes: 500,
        ..Options::default()
    };
    let html = format!("{}{}", img("a.png"), img("a.png"));
    let e = single(&html, &root, &o);
    assert_eq!(only_warning(&e).reason, FigureReason::TotalCap);
    let e = embed(&html, &root, &Mode::Folder { dir: other }, &o).unwrap();
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
}

#[test]
fn folder_mode_writes_hash_named_files_and_deduplicates() {
    let (root, out) = project("folder");
    write(&root, "a.png", PNG);
    write(&root, "copy/a-again.png", PNG);
    write(&root, "b.jpg", JPG);
    write(&root, "p.pdf", &crate::snippet::tests::tiny_pdf());
    let html = format!(
        "{}{}{}{}{}",
        img("a.png"),
        img("copy/a-again.png"),
        img("b.jpg"),
        img("p.pdf"),
        img("a.png")
    );
    let e = embed(
        &html,
        &root,
        &Mode::Folder { dir: out.clone() },
        &Options::default(),
    )
    .unwrap();
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
    let srcs: Vec<String> = img_re()
        .find_iter(&e.html)
        .map(|m| {
            parse_attrs(m.as_str())
                .into_iter()
                .find(|(k, _)| k == "src")
                .unwrap()
                .1
        })
        .collect();
    assert_eq!(srcs.len(), 5);
    assert_eq!(srcs[0], srcs[1], "byte-identical files share one name");
    assert_eq!(srcs[0], srcs[4]);
    assert!(srcs
        .iter()
        .all(|s| s.starts_with("figures/") && !s.contains("..")));
    assert!(srcs[2].ends_with(".jpg"));
    assert!(srcs[3].ends_with(".png"), "a pdf figure is stored as png");
    let mut files: Vec<String> = fs::read_dir(out.join("figures"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(files.len(), 3, "{files:?}");
    assert!(files.iter().all(|f| !f.ends_with(".part")));
    assert_eq!(fs::read(out.join(&srcs[0])).unwrap(), PNG);
    // Nothing is inlined and the project is untouched.
    assert!(!e.html.contains("data:"));
    assert!(!root.join("figures").exists());
    // A second run reuses the files.
    let again = embed(
        &html,
        &root,
        &Mode::Folder { dir: out },
        &Options::default(),
    )
    .unwrap();
    assert_eq!(again.html, e.html);
}

#[test]
fn the_figure_file_names_do_not_come_from_the_author() {
    let (root, out) = project("names");
    // Windows forbids `"` in a file name; the rest still needs escaping.
    let name = if cfg!(windows) {
        "we ird name'.png"
    } else {
        "we ird\"name'.png"
    };
    write(&root, name, PNG);
    let e = embed(
        &img(name),
        &root,
        &Mode::Folder { dir: out },
        &Options::default(),
    )
    .unwrap();
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
    let src = src_of(&e.html);
    assert!(src
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.')));
}

#[test]
fn a_hostile_file_name_cannot_break_out_of_the_placeholder_markup() {
    let (root, _) = project("xss");
    let name = "x\"><script>alert(1)</script>.png";
    let e = single(&img(name), &root, &Options::default());
    assert_placeholder(&e);
    assert!(!e.html.contains("<script>"), "{}", e.html);
    assert_eq!(only_warning(&e).file, name);
}

#[test]
fn an_unusable_root_is_an_error() {
    let (root, _) = project("badroot");
    assert!(embed(
        "",
        &root.join("nope"),
        &Mode::SingleFile,
        &Options::default()
    )
    .is_err());
    assert!(embed(
        "",
        Path::new("relative"),
        &Mode::SingleFile,
        &Options::default()
    )
    .is_err());
}
