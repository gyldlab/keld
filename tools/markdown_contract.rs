//! Shared Markdown visibility primitives for standalone repository contract checkers.

/// Returns the fence byte, width, and whether the remaining text is a valid closing tail.
pub(crate) fn fence_marker(line: &str) -> Option<(u8, usize, bool)> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    if indent > 3 || line.starts_with('\t') {
        return None;
    }
    let trimmed = &line[indent..];
    let marker = *trimmed.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let width = trimmed.bytes().take_while(|byte| *byte == marker).count();
    let tail = &trimmed[width..];
    // A backtick in the info string makes the line inline code, not a fence.
    (width >= 3 && !(marker == b'`' && tail.contains('`')))
        .then(|| (marker, width, tail.trim_matches([' ', '\t']).is_empty()))
}

/// Removes struck-through text because it is not binding visible prose.
pub(crate) fn without_struck_text(line: &str) -> String {
    let mut visible = String::with_capacity(line.len());
    let mut remainder = line;
    while let Some(start) = remainder.find("~~") {
        visible.push_str(&remainder[..start]);
        let struck = &remainder[start + 2..];
        let Some(end) = struck.find("~~") else {
            visible.push_str(&remainder[start..]);
            return visible;
        };
        remainder = &struck[end + 2..];
    }
    visible.push_str(remainder);
    visible
}

/// One inline-code span: `start..end` holds its delimiters, `content` the code text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodeSpan {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) content: std::ops::Range<usize>,
}

/// The code span a backtick run at `at` opens, or the end of a run with no partner. A
/// closing run has the opening width. One space each side is padding, unless the code is
/// only spaces.
pub(crate) fn code_span_at(line: &str, at: usize) -> Result<CodeSpan, usize> {
    fn run_width(bytes: &[u8], at: usize) -> usize {
        bytes[at..].iter().take_while(|byte| **byte == b'`').count()
    }

    let bytes = line.as_bytes();
    let width = run_width(bytes, at);
    let body = at + width;
    let mut scan = body;
    let closing = loop {
        let Some(offset) = bytes[scan..].iter().position(|byte| *byte == b'`') else {
            return Err(body);
        };
        let run = scan + offset;
        let found = run_width(bytes, run);
        if found == width {
            break run;
        }
        scan = run + found;
    };
    let text = &line[body..closing];
    let padded =
        text.len() > 2 && text.starts_with(' ') && text.ends_with(' ') && !text.trim().is_empty();
    Ok(CodeSpan {
        start: at,
        end: closing + width,
        content: if padded {
            body + 1..closing - 1
        } else {
            body..closing
        },
    })
}

/// CommonMark code spans of one line, left to right: a backslash escapes an opening
/// backtick, and a run with no partner is literal.
pub(crate) fn inline_code_spans(line: &str) -> Vec<CodeSpan> {
    let bytes = line.as_bytes();
    let mut spans = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'`' => match code_span_at(line, index) {
                Ok(span) => {
                    index = span.end;
                    spans.push(span);
                }
                Err(end) => index = end,
            },
            _ => index += 1,
        }
    }
    spans
}

/// Removes inline-code spans because code-shaped text is not an executable Markdown link.
pub(crate) fn without_inline_code(line: &str) -> String {
    let mut visible = String::with_capacity(line.len());
    let mut kept = 0;
    for span in inline_code_spans(line) {
        visible.push_str(&line[kept..span.start]);
        kept = span.end;
    }
    visible.push_str(&line[kept..]);
    visible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_require_supported_indent_and_whitespace_only_closing_tail() {
        assert_eq!(fence_marker("    ```"), None);
        assert_eq!(fence_marker("\t```"), None);
        assert_eq!(fence_marker("```rust"), Some((b'`', 3, false)));
        assert_eq!(fence_marker("````not-a-close"), Some((b'`', 4, false)));
        assert_eq!(fence_marker("````  "), Some((b'`', 4, true)));
        assert_eq!(fence_marker("```a```"), None, "one line of inline code");
        assert_eq!(fence_marker("~~~ a`b"), Some((b'~', 3, false)));
        assert_eq!(
            fence_marker("```\u{a0}"),
            Some((b'`', 3, false)),
            "a no-break space is text"
        );
    }

    #[test]
    fn struck_text_removes_only_closed_spans() {
        assert_eq!(
            without_struck_text("before ~~gone~~ after"),
            "before  after"
        );
        assert_eq!(without_struck_text("before ~~visible"), "before ~~visible");
    }

    #[test]
    fn inline_code_requires_an_equal_width_closing_run() {
        assert_eq!(without_inline_code("before `gone` after"), "before  after");
        assert_eq!(
            without_inline_code("before ``gone`` after"),
            "before  after"
        );
        assert_eq!(without_inline_code("before `visible"), "before `visible");
        assert_eq!(
            without_inline_code("before `not-closed-by-two`` after"),
            "before `not-closed-by-two`` after"
        );
        assert_eq!(
            without_inline_code("before ``not-closed-by-one` after"),
            "before ``not-closed-by-one` after"
        );
        assert_eq!(without_inline_code("a `` b `c` d"), "a `` b  d");
        assert_eq!(without_inline_code(r"a \`b` c"), r"a \`b` c");
        assert_eq!(without_inline_code(r"a \\`b` c"), r"a \\ c");
    }

    #[test]
    fn code_spans_report_their_content_without_padding() {
        let content = |line: &'static str| -> Vec<&'static str> {
            inline_code_spans(line)
                .into_iter()
                .map(|span| &line[span.content])
                .collect()
        };
        assert_eq!(content("`a` and ``b``"), ["a", "b"]);
        assert_eq!(content("`` a`b ``"), ["a`b"]);
        assert_eq!(content("`  `"), ["  "]);
        assert_eq!(content("` a`"), [" a"]);
    }
}
