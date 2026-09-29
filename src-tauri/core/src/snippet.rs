//! A rendered region of a main file's compiled pdf, with the source that
//! produced it: what an agent asks for when it needs to see its layout.
//! Addressed by a source line, a label, or a whole page; the region comes
//! from SyncTeX, the pixels from hayro. Everything crosses back
//! root-relative, and the image is optional (it costs model context).

use crate::Core;

use serde::Serialize;

/// What to show.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Line { tex_rel: String, line: u32 },
    Label { label: String },
    Page { page: u32 },
}

/// A region on a page, in pdf points from the page's top left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// The source lines around the target.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceExcerpt {
    pub rel: String,
    pub line: u32,
    /// Lines `first_line`.. of `rel`, the target line among them.
    pub first_line: u32,
    pub excerpt: String,
}

/// The rendered image, when one was asked for and made.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub main: String,
    /// The page shown, 1-based, and how many the pdf has.
    pub page: u32,
    pub pages: u32,
    /// The target's region on `page`; `None` for a whole page, or when
    /// SyncTeX placed the line nowhere (then the whole page is shown).
    pub region: Option<Region>,
    pub source: Option<SourceExcerpt>,
    /// The source file changed after the pdf was written: recompile first.
    pub stale: bool,
    pub image: Option<ImageInfo>,
    /// Why no image came back although one was asked for.
    pub image_error: Option<String>,
    /// PNG bytes for the adapter to carry as image content.
    #[serde(skip)]
    #[schemars(skip)]
    pub png: Option<Vec<u8>>,
}

/// Source lines shown on each side of the target.
const CONTEXT_LINES: u32 = 3;
/// Excerpt size cap, in bytes.
const MAX_EXCERPT: usize = 2000;
/// Vertical padding around a region, in pdf points (about a line).
const PAD_PT: f32 = 14.0;
/// Render scale before caps, and the caps.
const SCALE: f32 = 2.0;
const MAX_WIDTH_PX: f32 = 1600.0;
const MAX_PNG_BYTES: usize = 400 * 1024;

pub fn snippet(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    target: &Target,
    with_image: bool,
) -> Result<Snippet, String> {
    let out = super::outputs_of(cx, root_id, main_rel)?;
    let pdf_path = out.outdir.join(&out.pdf_name);
    let pdf_bytes = std::fs::read(&pdf_path)
        .map_err(|_| format!("no compiled output for {}: compile it first", main_rel))?;
    let pdf_mtime = mtime(&pdf_path);

    let (page, region, source, stale) = match target {
        Target::Page { page } => (*page, None, None, false),
        Target::Line { tex_rel, line } => {
            at_line(cx, root_id, main_rel, tex_rel, *line, pdf_mtime)?
        }
        Target::Label { label } => {
            let labels = super::structure::labels_refs(cx, root_id, main_rel)?;
            let def = labels
                .labels
                .iter()
                .find(|l| l.key == *label)
                .ok_or_else(|| format!("no label {} in the document of {}", label, main_rel))?;
            at_line(cx, root_id, main_rel, &def.rel, def.line, pdf_mtime)?
        }
    };

    let pdf = hayro::hayro_syntax::Pdf::new(pdf_bytes)
        .map_err(|e| format!("cannot read the pdf of {}: {:?}", main_rel, e))?;
    let pages = pdf.pages().len() as u32;
    if page == 0 || page > pages {
        return Err(format!(
            "page {} is out of range: the pdf has {} pages",
            page, pages
        ));
    }

    let (mut image, mut image_error, mut png) = (None, None, None);
    if with_image {
        let rendered =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(&pdf, page, region)));
        match rendered {
            Ok(Ok((bytes, width, height))) => {
                image = Some(ImageInfo {
                    width,
                    height,
                    bytes: bytes.len(),
                });
                png = Some(bytes);
            }
            Ok(Err(e)) => image_error = Some(e),
            Err(_) => image_error = Some(format!("the renderer failed on page {}", page)),
        }
    }

    Ok(Snippet {
        main: main_rel.to_string(),
        page,
        pages,
        region,
        source,
        stale,
        image,
        image_error,
        png,
    })
}

type Placed = (u32, Option<Region>, Option<SourceExcerpt>, bool);

/// Where a source line landed, its excerpt, and whether the pdf predates it.
fn at_line(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    tex_rel: &str,
    line: u32,
    pdf_mtime: Option<std::time::SystemTime>,
) -> Result<Placed, String> {
    if line == 0 {
        return Err("line is 1-based".to_string());
    }
    let path = super::fs::resolve_in(cx, root_id, tex_rel)?;
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", tex_rel, e))?;
    let stale = matches!((mtime(&path), pdf_mtime), (Some(src), Some(pdf)) if src > pdf);
    let source = Some(excerpt(tex_rel, &text, line));
    let boxes = super::synctex::forward_boxes(cx, root_id, main_rel, tex_rel, line)?;
    let Some(first) = boxes.first() else {
        return Err(format!(
            "{} line {} is not in the compiled pdf (not typeset, or compiled before it existed)",
            tex_rel, line
        ));
    };
    let page = first.page;
    let on_page: Vec<_> = boxes.iter().filter(|b| b.page == page).collect();
    let x0 = on_page.iter().map(|b| b.x).fold(f32::MAX, f32::min);
    let y0 = on_page.iter().map(|b| b.y).fold(f32::MAX, f32::min);
    let x1 = on_page
        .iter()
        .map(|b| b.x + b.width)
        .fold(f32::MIN, f32::max);
    let y1 = on_page
        .iter()
        .map(|b| b.y + b.height)
        .fold(f32::MIN, f32::max);
    let region = Region {
        x: x0,
        y: y0,
        width: (x1 - x0).max(0.0),
        height: (y1 - y0).max(0.0),
    };
    Ok((page, Some(region), source, stale))
}

fn excerpt(rel: &str, text: &str, line: u32) -> SourceExcerpt {
    let first_line = line.saturating_sub(CONTEXT_LINES).max(1);
    let mut excerpt = text
        .lines()
        .skip(first_line as usize - 1)
        .take((line - first_line + 1 + CONTEXT_LINES) as usize)
        .collect::<Vec<_>>()
        .join("\n");
    if excerpt.len() > MAX_EXCERPT {
        let mut cut = MAX_EXCERPT;
        while !excerpt.is_char_boundary(cut) {
            cut -= 1;
        }
        excerpt.truncate(cut);
    }
    SourceExcerpt {
        rel: rel.to_string(),
        line,
        first_line,
        excerpt,
    }
}

fn mtime(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).ok()?.modified().ok()
}

/// PNG of `page`: a full-width band around `region`, or the whole page.
/// Scale drops until the image fits the width and byte caps.
fn render(
    pdf: &hayro::hayro_syntax::Pdf,
    page: u32,
    region: Option<Region>,
) -> Result<(Vec<u8>, u32, u32), String> {
    use hayro::vello_cpu::color::palette::css::WHITE;
    use hayro::vello_cpu::Pixmap;

    let p = &pdf.pages()[page as usize - 1];
    let (page_w, page_h) = p.render_dimensions();
    let mut scale = SCALE.min(MAX_WIDTH_PX / page_w.max(1.0));
    for _ in 0..4 {
        let settings = hayro::RenderSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
            ..Default::default()
        };
        let full = hayro::render(
            p,
            &hayro::RenderCache::new(),
            &hayro::hayro_interpret::InterpreterSettings::default(),
            &settings,
        );
        let (w, h) = (full.width() as usize, full.height() as usize);
        let (top, bottom) = match region {
            Some(r) => {
                let top = ((r.y - PAD_PT).max(0.0) * scale) as usize;
                let bottom = (((r.y + r.height + PAD_PT).min(page_h)) * scale).ceil() as usize;
                (
                    top.min(h.saturating_sub(1)),
                    bottom.clamp(top.min(h) + 1, h),
                )
            }
            None => (0, h),
        };
        let band: Vec<_> = full.data()[top * w..bottom * w].to_vec();
        let bytes = Pixmap::from_parts(band, w as u16, (bottom - top) as u16)
            .into_png()
            .map_err(|e| format!("cannot encode page {}: {}", page, e))?;
        if bytes.len() <= MAX_PNG_BYTES {
            return Ok((bytes, w as u32, (bottom - top) as u32));
        }
        scale *= 0.75;
    }
    Err(format!("page {} does not fit the image size cap", page))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-page pdf with a filled box, written by hand so tests need no engine.
    fn tiny_pdf() -> Vec<u8> {
        let content = b"0 0 1 rg 100 600 200 100 re f";
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>".to_string(),
            format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                content.len(),
                std::str::from_utf8(content).unwrap()
            ),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).as_bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for off in offsets {
            out.extend(format!("{:010} 00000 n \n", off).as_bytes());
        }
        out.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                objs.len() + 1,
                xref
            )
            .as_bytes(),
        );
        out
    }

    fn png_size(bytes: &[u8]) -> (u32, u32) {
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let n = |i: usize| u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap());
        (n(16), n(20))
    }

    #[test]
    fn a_whole_page_renders_within_the_caps() {
        let pdf = hayro::hayro_syntax::Pdf::new(tiny_pdf()).unwrap();
        let (bytes, w, h) = render(&pdf, 1, None).unwrap();
        assert_eq!(png_size(&bytes), (w, h));
        assert_eq!((w, h), (1224, 1584));
        assert!(bytes.len() <= MAX_PNG_BYTES);
    }

    #[test]
    fn a_region_renders_as_a_padded_full_width_band() {
        let pdf = hayro::hayro_syntax::Pdf::new(tiny_pdf()).unwrap();
        // The box spans y 92..192 from the top (pdf y 600..700).
        let r = Region {
            x: 100.0,
            y: 92.0,
            width: 200.0,
            height: 100.0,
        };
        let (bytes, w, h) = render(&pdf, 1, Some(r)).unwrap();
        assert_eq!(png_size(&bytes), (w, h));
        assert_eq!(w, 1224);
        assert_eq!(h, ((100.0 + 2.0 * PAD_PT) * SCALE) as u32);
        // A region at the page edge is clamped, never empty.
        let edge = Region {
            x: 0.0,
            y: 790.0,
            width: 10.0,
            height: 30.0,
        };
        let (_, _, h) = render(&pdf, 1, Some(edge)).unwrap();
        assert!(h >= 1 && h <= ((2.0 + PAD_PT) * SCALE) as u32 + 1, "{h}");
    }

    #[test]
    fn excerpts_keep_context_and_stay_bounded() {
        let text = (1..=20)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let e = excerpt("a.tex", &text, 10);
        assert_eq!(e.first_line, 7);
        assert_eq!(e.excerpt.lines().count(), 7);
        assert!(e.excerpt.starts_with("line 7") && e.excerpt.ends_with("line 13"));
        let top = excerpt("a.tex", &text, 1);
        assert_eq!((top.first_line, top.excerpt.lines().count()), (1, 4));
        let long = "é".repeat(5000);
        assert!(excerpt("a.tex", &long, 1).excerpt.len() <= MAX_EXCERPT);
    }

    #[test]
    fn targets_are_checked_before_any_rendering() {
        let cx = &Core::default();
        let dir = crate::test_scratch::dir("snippet-targets");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.tex"), "x").unwrap();
        let root = dunce::canonicalize(&dir).unwrap();
        crate::fs::grant_root(cx, "snip", &root.to_string_lossy()).unwrap();
        let err = snippet(cx, "snip", "main.tex", &Target::Page { page: 1 }, false).unwrap_err();
        assert!(err.contains("compile it first"), "{err}");
        for bad in crate::test_scratch::escapes() {
            assert!(
                snippet(cx, "snip", bad, &Target::Page { page: 1 }, false).is_err(),
                "{bad}"
            );
        }
        let out = crate::outputs_of(cx, "snip", "main.tex").unwrap();
        std::fs::create_dir_all(&out.outdir).unwrap();
        std::fs::write(out.outdir.join(&out.pdf_name), tiny_pdf()).unwrap();
        let s = snippet(cx, "snip", "main.tex", &Target::Page { page: 1 }, true).unwrap();
        assert_eq!((s.page, s.pages, s.region), (1, 1, None));
        assert!(
            s.png.is_some() && s.image.as_ref().unwrap().bytes == s.png.as_ref().unwrap().len()
        );
        let plain = snippet(cx, "snip", "main.tex", &Target::Page { page: 1 }, false).unwrap();
        assert!(plain.png.is_none() && plain.image.is_none());
        let err = snippet(cx, "snip", "main.tex", &Target::Page { page: 2 }, false).unwrap_err();
        assert!(err.contains("out of range"), "{err}");
        let line0 = Target::Line {
            tex_rel: "main.tex".into(),
            line: 0,
        };
        assert!(snippet(cx, "snip", "main.tex", &line0, false).is_err());
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains(&*root.to_string_lossy()), "{json}");
        assert!(!json.contains("png"), "{json}");
        let _ = std::fs::remove_dir_all(&out.outdir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
