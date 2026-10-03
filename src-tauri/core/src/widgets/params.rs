//! Poster parameters a widget may carry: what its runtime shows when the
//! app renders it to a still image, and where a live view starts. The
//! package records them as the author wrote them (commas in a camera turned
//! into spaces, since the sidecar splits options on commas); here they are
//! checked and put in one canonical form, so every reader of the widget list
//! (the poster renderer, the bundle exporter, the runtimes) sees the same
//! values.
//!
//! - `camera` (model): a camera-to-world matrix, 16 numbers in column-major
//!   order (as glTF and three.js store it), or `pos=x,y,z target=x,y,z
//!   up=x,y,z` (`up` defaults to +y). Canonical: 16 numbers of a rigid
//!   transform, space separated.
//! - `size` (model): the poster's pixel size, `WxH`.
//! - `background` (model): `transparent` or an HTML hex colour without `#`
//!   (the package cannot carry `#`). Canonical: `transparent` or `#rrggbb`.
//! - `scale` (chart): the poster's pixels per CSS pixel.
//! - `framedomains`, `resourcedomains` (html): origins the widget declares,
//!   space separated, as `widget.json`'s `frameDomains` and
//!   `resourceDomains`. Canonical: sorted, deduplicated, space separated.

use super::WidgetType;

/// The smallest and largest poster side in pixels.
pub const MIN_SIDE: u32 = 16;
pub const MAX_SIDE: u32 = 4096;
/// The largest chart scale.
const MAX_SCALE: f64 = 8.0;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> Option<V3> {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    (l.is_finite() && l > 1e-9).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

/// Numbers separated by commas and/or whitespace.
fn numbers(s: &str) -> Result<Vec<f64>, String> {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .map(|t| {
            t.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("`{t}` is not a number"))
        })
        .collect()
}

/// A rigid camera-to-world matrix (column-major) looking from `eye` along
/// `forward`, with `up` as the screen's up direction.
fn rigid(eye: V3, forward: V3, up: V3) -> Result<[f64; 16], String> {
    // The camera looks down its -z axis.
    let z =
        norm([-forward[0], -forward[1], -forward[2]]).ok_or("the camera has no view direction")?;
    let x = norm(cross(up, z)).ok_or("the camera's up is parallel to its view direction")?;
    let y = cross(z, x);
    Ok([
        x[0], x[1], x[2], 0.0, y[0], y[1], y[2], 0.0, z[0], z[1], z[2], 0.0, eye[0], eye[1],
        eye[2], 1.0,
    ])
}

fn three(key: &str, v: &[f64]) -> Result<V3, String> {
    match v {
        [a, b, c] => Ok([*a, *b, *c]),
        _ => Err(format!("{key}= needs three numbers, found {}", v.len())),
    }
}

/// The camera as a canonical rigid camera-to-world matrix.
pub fn camera(s: &str) -> Result<[f64; 16], String> {
    let s = s.trim();
    if s.contains('=') {
        let mut groups: Vec<(String, String)> = Vec::new();
        for tok in s.split_whitespace() {
            match tok.split_once('=') {
                Some((k, rest)) => groups.push((k.to_string(), rest.to_string())),
                None => match groups.last_mut() {
                    Some((_, v)) => {
                        v.push(' ');
                        v.push_str(tok);
                    }
                    None => return Err(format!("`{tok}` is not part of pos=, target= or up=")),
                },
            }
        }
        let (mut pos, mut target, mut up) = (None, None, None);
        for (k, v) in &groups {
            let slot = match k.as_str() {
                "pos" => &mut pos,
                "target" => &mut target,
                "up" => &mut up,
                _ => return Err(format!("unknown camera key `{k}` (use pos, target and up)")),
            };
            if slot.is_some() {
                return Err(format!("camera key `{k}` is given twice"));
            }
            *slot = Some(three(k, &numbers(v)?)?);
        }
        let pos = pos.ok_or("the camera needs pos=x,y,z")?;
        let target = target.ok_or("the camera needs target=x,y,z")?;
        let up = up.unwrap_or([0.0, 1.0, 0.0]);
        return rigid(pos, sub(target, pos), up).map_err(|e| {
            format!("{e} (pos and target must differ, up must not point along them)")
        });
    }
    let m = numbers(s)?;
    if m.len() != 16 {
        return Err(format!(
            "a camera matrix has 16 numbers, found {} (or use pos=x,y,z target=x,y,z up=x,y,z)",
            m.len()
        ));
    }
    let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
    if !(close(m[3], 0.0) && close(m[7], 0.0) && close(m[11], 0.0) && close(m[15], 1.0)) {
        return Err(
            "a camera matrix is column-major with a last row of 0 0 0 1 (m[3], m[7], m[11], m[15])"
                .into(),
        );
    }
    // Scale and shear are dropped: the view direction (-z column) and the
    // up column fix the camera, rebuilt orthonormal.
    let forward = [-m[8], -m[9], -m[10]];
    let up = [m[4], m[5], m[6]];
    rigid([m[12], m[13], m[14]], forward, up)
        .map_err(|e| format!("a degenerate camera matrix: {e}"))
}

/// A number as short decimal text: at most 6 decimals, no trailing zeros.
pub fn num(v: f64) -> String {
    let v = if v.abs() < 5e-7 { 0.0 } else { v };
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// `WxH` in pixels.
pub fn size(s: &str) -> Result<(u32, u32), String> {
    let bad = || format!("size= is WxH in pixels (e.g. 800x600), not `{s}`");
    let (w, h) = s.trim().split_once('x').ok_or_else(bad)?;
    let p = |t: &str| {
        (!t.is_empty() && t.bytes().all(|c| c.is_ascii_digit()))
            .then(|| t.parse::<u32>().ok())
            .flatten()
    };
    let (w, h) = (p(w).ok_or_else(bad)?, p(h).ok_or_else(bad)?);
    for v in [w, h] {
        if !(MIN_SIDE..=MAX_SIDE).contains(&v) {
            return Err(format!(
                "size= sides must be {MIN_SIDE} to {MAX_SIDE} pixels, not {v}"
            ));
        }
    }
    Ok((w, h))
}

/// `transparent` or `#rrggbb`.
pub fn background(s: &str) -> Result<String, String> {
    let t = s.trim().to_ascii_lowercase();
    if t == "transparent" {
        return Ok(t);
    }
    let hex = t.strip_prefix('#').unwrap_or(&t);
    if !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "background= is transparent or a hex colour like ffffff, not `{s}`"
        ));
    }
    match hex.len() {
        6 => Ok(format!("#{hex}")),
        3 => Ok(format!(
            "#{}",
            hex.chars().flat_map(|c| [c, c]).collect::<String>()
        )),
        _ => Err(format!(
            "background= is transparent or a hex colour like ffffff, not `{s}`"
        )),
    }
}

/// A chart poster's pixels per CSS pixel, above 0 and at most 8.
pub fn scale(s: &str) -> Result<f64, String> {
    let v: f64 = s
        .trim()
        .parse()
        .map_err(|_| format!("scale= is a number like 2, not `{s}`"))?;
    if !(v.is_finite() && v > 0.0 && v <= MAX_SCALE) {
        return Err(format!(
            "scale= must be above 0 and at most {MAX_SCALE}, not {s}"
        ));
    }
    Ok(v)
}

/// The widget types a poster parameter applies to.
fn applies(key: &str) -> Option<&'static [WidgetType]> {
    match key {
        "camera" | "size" | "background" => Some(&[WidgetType::Model]),
        "scale" => Some(&[WidgetType::Chart]),
        "framedomains" | "resourcedomains" => Some(&[WidgetType::Html]),
        _ => None,
    }
}

/// Checks one recorded option and returns its canonical value. Options
/// that are not poster parameters pass through unchanged.
pub fn canonical(kind: WidgetType, key: &str, value: &str) -> Result<String, String> {
    let Some(types) = applies(key) else {
        return Ok(value.to_string());
    };
    if !types.contains(&kind) {
        return Err(format!("{key}= does not apply to this widget type"));
    }
    Ok(match key {
        "camera" => camera(value)?
            .iter()
            .map(|v| num(*v))
            .collect::<Vec<_>>()
            .join(" "),
        "size" => {
            let (w, h) = size(value)?;
            format!("{w}x{h}")
        }
        "background" => background(value)?,
        "framedomains" | "resourcedomains" => super::canonical_origins(key, value)?,
        _ => num(scale(value)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn identity_matrix_stays_identity() {
        let id = "1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1";
        let m = camera(id).unwrap();
        assert!(close(
            &m,
            &[1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]
        ));
        // Space separated (what the package records) reads the same.
        assert_eq!(camera(&id.replace(',', " ")).unwrap(), m);
    }

    #[test]
    fn a_scaled_matrix_is_made_rigid_and_keeps_its_position() {
        let m = camera("2 0 0 0  0 3 0 0  0 0 4 0  1 2 5 1").unwrap();
        assert!(close(
            &m,
            &[1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 1., 2., 5., 1.]
        ));
    }

    #[test]
    fn pos_target_up_builds_a_look_at_matrix() {
        // From +z looking at the origin: the identity rotation.
        let m = camera("pos=0 0 5 target=0 0 0 up=0 1 0").unwrap();
        assert!(close(
            &m,
            &[1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 5., 1.]
        ));
        // From +x: the camera's -z points along -x, its x axis along -z.
        let m = camera("pos=3,0,0 target=0,0,0").unwrap();
        assert!(close(
            &m,
            &[0., 0., -1., 0., 0., 1., 0., 0., 1., 0., 0., 0., 3., 0., 0., 1.]
        ));
        // Commas, extra spaces and key order do not matter.
        assert_eq!(camera("target=0,0,0  pos=3, 0, 0").unwrap(), m);
    }

    #[test]
    fn matrix_and_look_at_forms_agree() {
        let a = camera("pos=1,2,3 target=0,0.5,0 up=0,1,0").unwrap();
        let b = camera(&a.iter().map(|v| num(*v)).collect::<Vec<_>>().join(",")).unwrap();
        assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-5));
    }

    #[test]
    fn bad_cameras_are_named() {
        for (s, want) in [
            ("1 0 0", "16 numbers"),
            ("1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 2", "last row"),
            ("1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1", "degenerate"),
            ("pos=1,2,3", "target="),
            ("pos=1,2,3 target=1,2,3", "no view direction"),
            ("pos=0,5,0 target=0,0,0 up=0,1,0", "parallel"),
            ("pos=1,2 target=0,0,0", "three numbers"),
            ("eye=1,2,3 target=0,0,0", "unknown camera key"),
            ("pos=1,2,3 pos=1,1,1 target=0,0,0", "twice"),
            ("1 0 0 0 0 1 0 0 0 0 1 0 0 0 nan 1", "not a number"),
            ("1 0 0 0 0 1 0 0 0 0 1 0 0 0 x 1", "not a number"),
        ] {
            let e = camera(s).unwrap_err();
            assert!(e.contains(want), "{s}: {e}");
        }
    }

    #[test]
    fn sizes_backgrounds_and_scales() {
        assert_eq!(size("800x600").unwrap(), (800, 600));
        for bad in [
            "800", "800x", "x600", "-1x5", "8.5x6", "15x600", "4097x600", "800X600",
        ] {
            assert!(size(bad).is_err(), "{bad}");
        }
        assert_eq!(background("FFFFFF").unwrap(), "#ffffff");
        assert_eq!(background("abc").unwrap(), "#aabbcc");
        assert_eq!(background("Transparent").unwrap(), "transparent");
        for bad in ["red", "ffff", "#gggggg", ""] {
            assert!(background(bad).is_err(), "{bad}");
        }
        assert_eq!(scale("2").unwrap(), 2.0);
        assert_eq!(scale("1.5").unwrap(), 1.5);
        for bad in ["0", "-1", "9", "two", "inf"] {
            assert!(scale(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn canonical_forms_and_type_checks() {
        let m = WidgetType::Model;
        assert_eq!(
            canonical(m, "camera", "pos=0,0,5 target=0,0,0").unwrap(),
            "1 0 0 0 0 1 0 0 0 0 1 0 0 0 5 1"
        );
        assert_eq!(canonical(m, "size", "640x480").unwrap(), "640x480");
        assert_eq!(canonical(m, "background", "FFF").unwrap(), "#ffffff");
        assert_eq!(
            canonical(WidgetType::Chart, "scale", "2.50").unwrap(),
            "2.5"
        );
        // Other options pass through verbatim.
        assert_eq!(
            canonical(m, "height", "170.71652pt").unwrap(),
            "170.71652pt"
        );
        for (k, kind) in [
            ("camera", WidgetType::Chart),
            ("size", WidgetType::Video),
            ("background", WidgetType::Html),
            ("scale", WidgetType::Model),
        ] {
            assert!(
                canonical(kind, k, "1")
                    .unwrap_err()
                    .contains("does not apply"),
                "{k}"
            );
        }
        assert_eq!(num(-0.0000001), "0");
        assert_eq!(num(0.1 + 0.2), "0.3");
        assert_eq!(num(-2.5), "-2.5");
    }
}
