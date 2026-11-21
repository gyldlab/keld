//! Bounded Rust source-token masking for D-23's test-only callsite oracle.
//!
//! This is a source-shape guard, not a Rust parser or a security argument.
//! Keep every byte offset stable to apply single-call mutants to the actual
//! method source. Unknown syntax may conservatively refuse a future edit.

use std::ops::Range;

pub(super) fn hide_inert_text(source: &str) -> Result<String, &'static str> {
    let bytes = source.as_bytes();
    let mut visible = bytes.to_vec();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let end = if bytes[at..].starts_with(b"//") {
            at = (at + 2..bytes.len())
                .find(|&index| bytes[index] == b'\n')
                .unwrap_or(bytes.len());
            Some(at)
        } else if bytes[at..].starts_with(b"/*") {
            at += 2;
            let mut depth = 1_usize;
            while at < bytes.len() && depth != 0 {
                if bytes[at..].starts_with(b"/*") {
                    depth += 1;
                    at += 2;
                } else if bytes[at..].starts_with(b"*/") {
                    depth -= 1;
                    at += 2;
                } else {
                    at += 1;
                }
            }
            if depth != 0 {
                return Err("unterminated D23 source comment");
            }
            Some(at)
        } else {
            scan_literal(source, &mut at)?
        };
        if let Some(end) = end {
            for byte in &mut visible[start..end] {
                if !matches!(*byte, b'\n' | b'\r') {
                    *byte = b' ';
                }
            }
        }
    }
    hide_macro_trees(&mut visible)?;
    String::from_utf8(visible).map_err(|_| "invalid D23 source encoding")
}

fn scan_literal(source: &str, at: &mut usize) -> Result<Option<usize>, &'static str> {
    let bytes = source.as_bytes();
    let raw_after_prefix = match bytes[*at..] {
        [b'b' | b'c', b'r', ..] => Some(*at + 2),
        [b'r', ..] => Some(*at + 1),
        _ => None,
    };
    let mut raw = None;
    if let Some(hash_start) = raw_after_prefix {
        let mut quote = hash_start;
        while bytes.get(quote) == Some(&b'#') {
            quote += 1;
        }
        if bytes.get(quote) == Some(&b'"') {
            let hashes = quote - hash_start;
            *at = quote + 1;
            while *at < bytes.len() {
                if bytes[*at] == b'"'
                    && bytes
                        .get(*at + 1..*at + 1 + hashes)
                        .is_some_and(|suffix| suffix.iter().all(|&byte| byte == b'#'))
                {
                    raw = Some(*at + 1 + hashes);
                    break;
                }
                *at += 1;
            }
            if raw.is_none() {
                return Err("unterminated D23 raw string");
            }
        }
    }
    if let Some(end) = raw {
        *at = end;
        return Ok(Some(end));
    }
    if bytes[*at] == b'"' || matches!(bytes[*at..], [b'b' | b'c', b'"', ..]) {
        let quote = if bytes[*at] == b'"' { *at } else { *at + 1 };
        *at = quote + 1;
        while *at < bytes.len() && bytes[*at] != b'"' {
            if bytes[*at] == b'\\' {
                *at += 1;
            }
            *at += 1;
        }
        if *at >= bytes.len() {
            return Err("unterminated D23 source string");
        }
        *at += 1;
        return Ok(Some(*at));
    }
    let quote = if bytes[*at] == b'\'' {
        Some(*at)
    } else if matches!(bytes[*at..], [b'b', b'\'', ..]) {
        Some(*at + 1)
    } else {
        None
    };
    if let Some(end) = quote.and_then(|quote| char_literal_end(source, quote)) {
        *at = end;
        Ok(Some(end))
    } else {
        *at += 1;
        Ok(None)
    }
}

fn char_literal_end(source: &str, quote: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let first = quote + 1;
    let end = if bytes.get(first) == Some(&b'\\') {
        match bytes.get(first + 1)? {
            b'u' if bytes.get(first + 2) == Some(&b'{') => {
                let close = (first + 3..bytes.len()).find(|&at| bytes[at] == b'}')?;
                close + 1
            }
            b'x' => first + 4,
            _ => first + 2,
        }
    } else {
        first + source.get(first..)?.chars().next()?.len_utf8()
    };
    (bytes.get(end) == Some(&b'\'')).then_some(end + 1)
}

// Mask every macro token tree, including uninvoked definitions. A Rust macro
// can carry a convincing guard call inside stringify, but never run it.
fn hide_macro_trees(visible: &mut [u8]) -> Result<(), &'static str> {
    let mut at = 0;
    while at < visible.len() {
        if visible[at] != b'!' {
            at += 1;
            continue;
        }
        let mut open = at + 1;
        while visible.get(open).is_some_and(u8::is_ascii_whitespace) {
            open += 1;
        }
        if visible.get(open) == Some(&b'r') && visible.get(open + 1) == Some(&b'#') {
            open += 2;
        }
        if visible
            .get(open)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_' || *byte >= 0x80)
        {
            open += 1;
            while visible
                .get(open)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte >= 0x80)
            {
                open += 1;
            }
            while visible.get(open).is_some_and(u8::is_ascii_whitespace) {
                open += 1;
            }
        }
        let close = match visible.get(open).copied() {
            Some(b'(') => b')',
            Some(b'[') => b']',
            Some(b'{') => b'}',
            _ => {
                at += 1;
                continue;
            }
        };
        let mut stack = vec![close];
        let mut cursor = open + 1;
        while let Some(&byte) = visible.get(cursor) {
            match byte {
                b'(' => stack.push(b')'),
                b'[' => stack.push(b']'),
                b'{' => stack.push(b'}'),
                b')' | b']' | b'}' => {
                    if stack.pop() != Some(byte) {
                        return Err("invalid D23 macro token tree");
                    }
                    if stack.is_empty() {
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        if !stack.is_empty() {
            return Err("unterminated D23 macro token tree");
        }
        for byte in &mut visible[at..=cursor] {
            if !matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
        at = cursor + 1;
    }
    Ok(())
}

pub(super) fn method_span(source: &str, name: &str) -> Result<Range<usize>, &'static str> {
    let signature = format!("pub(super) fn {name}(");
    let start = source.find(&signature).ok_or("D23 writer method missing")?;
    let open = source[start..]
        .find('{')
        .map(|at| start + at)
        .ok_or("D23 writer method body missing")?;
    let mut depth = 0_usize;
    for (offset, byte) in source.as_bytes()[open..].iter().enumerate() {
        match *byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(open + 1..open + offset);
                }
            }
            _ => {}
        }
    }
    Err("D23 writer method body incomplete")
}

pub(super) fn compact(body: &str) -> String {
    body.split_whitespace().collect::<Vec<_>>().join(" ")
}
