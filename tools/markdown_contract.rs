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
    (width >= 3).then(|| (marker, width, trimmed[width..].trim().is_empty()))
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

/// Removes inline-code spans because code-shaped text is not an executable Markdown link.
pub(crate) fn without_inline_code(line: &str) -> String {
    fn exact_run(input: &str, marker: u8, width: usize) -> Option<usize> {
        let bytes = input.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != marker {
                index += 1;
                continue;
            }
            let start = index;
            while index < bytes.len() && bytes[index] == marker {
                index += 1;
            }
            if index - start == width {
                return Some(start);
            }
        }
        None
    }

    let mut visible = String::with_capacity(line.len());
    let mut remainder = line;
    while let Some(start) = remainder.find('`') {
        visible.push_str(&remainder[..start]);
        let opening = &remainder[start..];
        let width = opening.bytes().take_while(|byte| *byte == b'`').count();
        let code = &opening[width..];
        let Some(end) = exact_run(code, b'`', width) else {
            visible.push_str(opening);
            return visible;
        };
        remainder = &code[end + width..];
    }
    visible.push_str(remainder);
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
    }
}
