//! Tiny JSONC comment stripper (`//` and `/* */`). Not a JSON parser.

/// Returns `input` with `//` line comments and `/* */` block comments replaced.
///
/// Comments inside JSON strings are left untouched so values like
/// `https://example.com` stay valid. The output is not otherwise rewritten
/// (no trailing-comma support). Comment bytes become spaces while newlines are
/// retained, preserving token separation and useful parser locations.
pub(crate) fn strip_jsonc_comments(input: &str) -> Result<String, &'static str> {
    let bytes = input.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    let mut in_string = false;
    let mut escape = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
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
            i += 1;
            continue;
        }
        if b == b'/'
            && let Some(next) = bytes.get(i + 1).copied()
        {
            if next == b'/' {
                out[i] = b' ';
                out[i + 1] = b' ';
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
                    out[i] = b' ';
                    i += 1;
                }
                continue;
            }
            if next == b'*' {
                let comment_start = i;
                out[i] = b' ';
                out[i + 1] = b' ';
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    if bytes[i] != b'\n' && bytes[i] != b'\r' {
                        out[i] = b' ';
                    }
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    out[i] = b' ';
                    out[i + 1] = b' ';
                    i += 2;
                } else {
                    return Err(if comment_start == 0 {
                        "unterminated block comment at byte 0"
                    } else {
                        "unterminated block comment"
                    });
                }
                continue;
            }
        }
        i += 1;
    }
    String::from_utf8(out).map_err(|_| "JSONC comment stripping produced invalid UTF-8")
}
