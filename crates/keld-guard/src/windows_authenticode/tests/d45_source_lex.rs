//! Test-only D-45 Rust source mask, adapted from D-23's lexical negative oracle.
//!
//! This is a bounded source-shape aid, not a Rust parser or a `WinTrust` proof.
//! It keeps byte offsets intact and refuses unexpected syntax rather than
//! accepting an inert string, character, comment or macro as live Rust code.

pub(super) fn hide_inert_text(source: &str) -> Result<String, &'static str> {
    mask_source(source, true)
}

// The publisher-scope function must not invoke active macros at all. Keep
// macros visible while masking comments and literals so a macro expansion that
// returns a fabricated identity cannot disappear from its source contract.
pub(super) fn hide_comments_and_literals(source: &str) -> Result<String, &'static str> {
    mask_source(source, false)
}

fn mask_source(source: &str, hide_macros: bool) -> Result<String, &'static str> {
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
                return Err("unterminated D-45 source comment");
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
    // Rust's lexer recognizes Pattern_White_Space as interchangeable token
    // separators. Preserve byte offsets while making its non-ASCII members
    // visible to the ASCII token scanners below. Comments and literals have
    // already been masked, so their contents cannot become active code.
    normalize_rust_whitespace(&mut visible);
    if hide_macros {
        hide_macro_trees(&mut visible)?;
    }
    String::from_utf8(visible).map_err(|_| "invalid D-45 source encoding")
}

fn normalize_rust_whitespace(visible: &mut [u8]) {
    let mut at = 0;
    while at < visible.len() {
        let width = match visible.get(at..) {
            Some([0xc2, 0x85, ..]) => 2, // U+0085
            Some([0xe2, 0x80, 0x8e | 0x8f | 0xa8 | 0xa9, ..]) => 3,
            // U+200E, U+200F, U+2028 and U+2029.
            _ => {
                at += 1;
                continue;
            }
        };
        visible[at..at + width].fill(b' ');
        at += width;
    }
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
                return Err("unterminated D-45 raw string");
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
            return Err("unterminated D-45 string");
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

// Treat macro invocations and unused macro_rules bodies as inert. In particular,
// stringify! can contain tokens for a removed security call without invoking it.
// Rust permits spaces, comments and raw/Unicode macro names around the bang.
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
                        return Err("invalid D-45 macro token tree");
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
            return Err("unterminated D-45 macro token tree");
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

// The mask runs first: a '}' character or inert macro brace cannot terminate a
// function. This extracts only the named live function, not matching comments.
pub(super) fn function_body<'a>(visible: &'a str, name: &str) -> Result<&'a str, &'static str> {
    let signature = format!("fn {name}(");
    let start = visible.find(&signature).ok_or("D-45 function missing")?;
    let open = visible[start + signature.len()..]
        .find('{')
        .map(|at| start + signature.len() + at)
        .ok_or("D-45 function body missing")?;
    let mut depth = 0_usize;
    for (offset, byte) in visible.as_bytes()[open..].iter().enumerate() {
        match *byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(&visible[open + 1..open + offset]);
                }
            }
            _ => {}
        }
    }
    Err("D-45 function body incomplete")
}

pub(super) fn compact(source: &str) -> String {
    source.split_whitespace().collect()
}
