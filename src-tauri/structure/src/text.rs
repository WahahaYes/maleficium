//! Shared scanning helpers: comment blanking and offset → line lookup.

/// Blank every `%` comment (an unescaped `%` to end of line) with spaces.
/// Byte length is preserved, so offsets into the result are offsets into
/// the source.
pub(crate) fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        match comment_start(line) {
            Some(i) => {
                out.push_str(&line[..i]);
                for b in line[i..].bytes() {
                    out.push(if b == b'\n' { '\n' } else { ' ' });
                }
            }
            None => out.push_str(line),
        }
    }
    out
}

/// Byte index of the first comment `%` in `line`: a backslash escapes the
/// byte after it, so `\%` is text and `\\%` starts a comment.
fn comment_start(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'%' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// 1-based line numbers for byte offsets.
pub(crate) struct Lines {
    starts: Vec<usize>,
}

impl Lines {
    pub(crate) fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Lines { starts }
    }

    pub(crate) fn line_of(&self, offset: usize) -> u32 {
        self.starts.partition_point(|&s| s <= offset) as u32
    }
}

/// Last path segment, or the whole string when it has none.
pub(crate) fn base_name(p: &str) -> &str {
    match p.rsplit('/').next() {
        Some(s) if !s.is_empty() => s,
        _ => p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_blank_to_same_length() {
        let src = "a % c\n\\% keep\n\\\\% gone\nλ%é\n";
        let out = strip_comments(src);
        assert_eq!(out.len(), src.len());
        assert_eq!(out, "a    \n\\% keep\n\\\\      \nλ   \n");
    }

    #[test]
    fn lines_are_one_based() {
        let l = Lines::new("ab\ncd\n\nx");
        assert_eq!(l.line_of(0), 1);
        assert_eq!(l.line_of(2), 1);
        assert_eq!(l.line_of(3), 2);
        assert_eq!(l.line_of(6), 3);
        assert_eq!(l.line_of(7), 4);
    }
}
