//! Tiny JSONC comment stripper (`//` and `/* */`). Not a JSON parser.

/// Returns `input` with `//` line comments and `/* */` block comments removed.
///
/// Comments inside JSON strings are left untouched so values like
/// `https://example.com` stay valid. The output is not otherwise rewritten
/// (no trailing-comma support).
pub(crate) fn strip_jsonc_comments(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut in_string = false;
    let mut escape = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            out.push(b);
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' {
            in_string = true;
            out.push(b);
            i += 1;
            continue;
        }
        if b == b'/'
            && let Some(next) = bytes.get(i + 1).copied()
        {
            if next == b'/' {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if next == b'*' {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    i += 2;
                } else {
                    i = bytes.len();
                }
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}
