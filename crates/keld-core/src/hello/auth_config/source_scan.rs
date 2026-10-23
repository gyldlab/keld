//! Identify the exported root auth literal while ignoring unrelated TypeScript.
use super::{AuthConfigError, ConfigToken, auth_config_error, config_line_terminator};

// This scanner locates a static exported object's root property; it does not
// validate/evaluate unrelated TypeScript or derive provider policy. Strict
// syntax/size checks apply only after a literal root auth property is found.
#[derive(Debug)]
struct SourceToken {
    kind: ConfigToken,
    start: usize,
    end: usize,
}

// This is only a key-location scanner. It over-approximates non-ASCII
// identifier code points to keep unrelated valid Unicode keys atomic;
// the compiler never evaluates the source or uses this as a TS validator.
fn source_space(ch: char) -> bool {
    ch.is_whitespace() || ch == '\u{feff}'
}

fn identifier_start(ch: char) -> bool {
    ch == '$'
        || ch == '_'
        || ch.is_ascii_alphabetic()
        || (!ch.is_ascii() && !source_space(ch) && !ch.is_control())
}

fn identifier_part(ch: char) -> bool {
    identifier_start(ch) || ch.is_ascii_digit()
}

fn string_hex(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, count: usize) -> Option<u32> {
    let mut value = 0_u32;
    for _ in 0..count {
        value = value
            .checked_mul(16)?
            .checked_add(chars.next()?.to_digit(16)?)?;
    }
    Some(value)
}

// ECMAScript key spelling, separate from the deliberately stricter auth
// value grammar in auth_config.rs. Invalid unrelated strings remain opaque.
fn source_string(raw: &str, quote: char) -> Option<String> {
    let mut chars = raw.chars().peekable();
    let mut result = String::new();
    while let Some(ch) = chars.next() {
        if ch == quote {
            return Some(result);
        }
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        let escaped = chars.next()?;
        let decoded = match escaped {
            '\n' | '\u{2028}' | '\u{2029}' => continue,
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                continue;
            }
            'x' => char::from_u32(string_hex(&mut chars, 2)?)?,
            'u' if chars.peek() == Some(&'{') => {
                chars.next();
                let mut value = 0_u32;
                let mut digits = 0;
                while chars.peek().is_some_and(|next| *next != '}') {
                    if digits == 6 {
                        return None;
                    }
                    value = value
                        .checked_mul(16)?
                        .checked_add(chars.next()?.to_digit(16)?)?;
                    digits += 1;
                }
                if digits == 0 || chars.next() != Some('}') {
                    return None;
                }
                char::from_u32(value).unwrap_or('\u{fffd}')
            }
            'u' => char::from_u32(string_hex(&mut chars, 4)?).unwrap_or('\u{fffd}'),
            '0' => '\0',
            'b' => '\u{0008}',
            'f' => '\u{000c}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'v' => '\u{000b}',
            // Includes quote/backslash and ECMAScript identity escapes.
            other => other,
        };
        result.push(decoded);
    }
    None
}

#[derive(Clone)]
struct SourceScan<'a> {
    source: &'a str,
    position: usize,
    regex_allowed: bool,
    last_word: Option<String>,
    last_symbol: Option<char>,
    control_parens: Vec<bool>,
    block_braces: Vec<bool>,
}

impl<'a> SourceScan<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
            regex_allowed: true,
            last_word: None,
            last_symbol: None,
            control_parens: Vec::new(),
            block_braces: Vec::new(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.position..)?.chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.position += ch.len_utf8();
        Some(ch)
    }

    // A valid UnicodeEscapeSequence contributes one identifier code point.
    // A failed lookahead never consumes bytes from the live scanner.
    fn identifier_escape(&mut self, first: bool) -> Option<char> {
        let mut preview = self.clone();
        if preview.advance()? != 'u' {
            return None;
        }
        let value = if preview.peek() == Some('{') {
            preview.advance();
            let mut value = 0_u32;
            let mut digits = 0;
            while preview.peek().is_some_and(|ch| ch != '}') {
                if digits == 6 {
                    return None;
                }
                value = value
                    .checked_mul(16)?
                    .checked_add(preview.advance()?.to_digit(16)?)?;
                digits += 1;
            }
            if digits == 0 || preview.advance() != Some('}') {
                return None;
            }
            value
        } else {
            let mut value = 0_u32;
            for _ in 0..4 {
                value = value
                    .checked_mul(16)?
                    .checked_add(preview.advance()?.to_digit(16)?)?;
            }
            value
        };
        let decoded = char::from_u32(value)?;
        if !(if first {
            identifier_start(decoded)
        } else {
            identifier_part(decoded)
        }) {
            return None;
        }
        self.position = preview.position;
        Some(decoded)
    }

    fn source_word(&mut self, first: char) -> Option<String> {
        let first = if first == '\\' {
            self.identifier_escape(true)?
        } else if identifier_start(first) {
            first
        } else {
            return None;
        };
        let mut word = String::from(first);
        while let Some(ch) = self.peek() {
            if ch == '\\' {
                let mut preview = self.clone();
                preview.advance();
                let Some(decoded) = preview.identifier_escape(false) else {
                    break;
                };
                self.position = preview.position;
                word.push(decoded);
            } else if identifier_part(ch) {
                self.advance();
                word.push(ch);
            } else {
                break;
            }
        }
        Some(word)
    }

    fn raw_string(&mut self, quote: char) {
        while let Some(ch) = self.advance() {
            if ch == '\\' {
                self.advance();
            } else if ch == quote {
                break;
            }
        }
    }

    fn regex(&mut self) {
        let mut class = false;
        while let Some(ch) = self.advance() {
            match ch {
                '\\' => {
                    self.advance();
                }
                '[' => class = true,
                ']' => class = false,
                '/' if !class => break,
                ch if config_line_terminator(ch) => break,
                _ => {}
            }
        }
        while self.peek().is_some_and(char::is_alphabetic) {
            self.advance();
        }
    }

    fn comment(&mut self) -> bool {
        if self.peek() == Some('/') {
            self.advance();
            while self.advance().is_some_and(|ch| !config_line_terminator(ch)) {}
            true
        } else if self.peek() == Some('*') {
            self.advance();
            while let Some(ch) = self.advance() {
                if ch == '*' && self.peek() == Some('/') {
                    self.advance();
                    break;
                }
            }
            true
        } else {
            false
        }
    }

    fn template(&mut self) {
        // Iterative modes, so unrelated template interpolation depth cannot
        // create a new call-stack/depth restriction for absent-auth configs.
        let mut modes = vec![(0_usize, true)];
        while let Some((depth, _)) = modes.last().copied() {
            let Some(ch) = self.advance() else {
                break;
            };
            if depth == 0 {
                match ch {
                    '\\' => {
                        self.advance();
                    }
                    '`' => {
                        modes.pop();
                    }
                    '$' if self.peek() == Some('{') => {
                        self.advance();
                        modes.push((1, true));
                    }
                    _ => {}
                }
                continue;
            }
            match ch {
                '\'' | '"' => {
                    self.raw_string(ch);
                    if let Some(mode) = modes.last_mut() {
                        mode.1 = false;
                    }
                }
                '`' => modes.push((0, true)),
                '{' => {
                    if let Some(mode) = modes.last_mut() {
                        mode.0 += 1;
                        mode.1 = true;
                    }
                }
                '}' => {
                    if let Some(mode) = modes.last_mut() {
                        mode.0 -= 1;
                    }
                    if modes.last().is_some_and(|mode| mode.0 == 0) {
                        modes.pop();
                    }
                }
                '/' => {
                    if !self.comment() {
                        if modes.last().is_some_and(|mode| mode.1) {
                            self.regex();
                        }
                        if let Some(mode) = modes.last_mut() {
                            mode.1 = true;
                        }
                    }
                }
                ch if identifier_start(ch) || ch == '\\' => {
                    let word = self.source_word(ch).unwrap_or_default();
                    if let Some(mode) = modes.last_mut() {
                        mode.1 = matches!(
                            word.as_str(),
                            "return"
                                | "throw"
                                | "case"
                                | "delete"
                                | "typeof"
                                | "void"
                                | "new"
                                | "yield"
                                | "await"
                                | "in"
                                | "instanceof"
                        );
                    }
                }
                ch if source_space(ch) => {}
                ch => {
                    if let Some(mode) = modes.last_mut() {
                        mode.1 = matches!(
                            ch,
                            '=' | ':' | ',' | '(' | '[' | '?' | '!' | '+' | '-' | '*' | ';'
                        );
                    }
                }
            }
        }
    }

    fn next(&mut self) -> Option<SourceToken> {
        loop {
            let start = self.position;
            let ch = self.advance()?;
            if source_space(ch) {
                continue;
            }
            if ch == '/' && self.comment() {
                continue;
            }
            let kind = if ch == '\'' || ch == '"' {
                self.raw_string(ch);
                let raw = self.source.get(start + ch.len_utf8()..self.position)?;
                let decoded = source_string(raw, ch);
                self.regex_allowed = false;
                self.last_word = None;
                self.last_symbol = None;
                // An unrelated malformed legacy string is opaque, not a new
                // config error. A malformed auth slice is rejected later.
                ConfigToken::Text(decoded.unwrap_or_default())
            } else if ch == '`' {
                self.template();
                self.regex_allowed = false;
                self.last_word = None;
                self.last_symbol = None;
                ConfigToken::Symbol('\0')
            } else if ch == '/' && self.regex_allowed {
                self.regex();
                self.regex_allowed = false;
                self.last_word = None;
                self.last_symbol = None;
                ConfigToken::Symbol('\0')
            } else if let Some(word) = self.source_word(ch) {
                self.regex_allowed = matches!(
                    word.as_str(),
                    "return"
                        | "throw"
                        | "case"
                        | "delete"
                        | "typeof"
                        | "void"
                        | "new"
                        | "yield"
                        | "await"
                        | "in"
                        | "instanceof"
                        | "else"
                        | "do"
                        | "default"
                );
                self.last_word = Some(word.clone());
                self.last_symbol = None;
                ConfigToken::Word(word)
            } else {
                self.regex_allowed = match ch {
                    '(' => {
                        self.control_parens
                            .push(self.last_word.as_deref().is_some_and(|word| {
                                matches!(word, "if" | "while" | "for" | "with" | "switch" | "catch")
                            }));
                        true
                    }
                    ')' => self.control_parens.pop().unwrap_or(false),
                    '{' => {
                        let block = self.last_symbol == Some(')')
                            || self.last_symbol == Some(';')
                            || (self.last_word.is_none() && self.last_symbol.is_none())
                            || self.last_word.as_deref().is_some_and(|word| {
                                !matches!(word, "default" | "return" | "yield")
                            });
                        self.block_braces.push(block);
                        true
                    }
                    '}' => self.block_braces.pop().unwrap_or(false),
                    ']' => false,
                    '+' | '-' if self.last_symbol == Some(ch) => false,
                    ch => !ch.is_ascii_digit() && ch != '.',
                };
                self.last_word = None;
                self.last_symbol = Some(ch);
                ConfigToken::Symbol(ch)
            };
            return Some(SourceToken {
                kind,
                start,
                end: self.position,
            });
        }
    }
}

fn recognized_root_key(name: &str) -> Option<&'static str> {
    match name {
        "auth" => Some("auth"),
        "profile" => Some("profile"),
        "initialWindow" => Some("initialWindow"),
        _ => None,
    }
}

fn static_root_key(token: &SourceToken, scan: &SourceScan<'_>) -> Option<&'static str> {
    match &token.kind {
        ConfigToken::Word(name) | ConfigToken::Text(name) => recognized_root_key(name),
        ConfigToken::Symbol('[') => {
            let mut preview = scan.clone();
            let key = preview.next()?;
            let close = preview.next()?;
            if close.kind == ConfigToken::Symbol(']')
                && let ConfigToken::Text(name) = &key.kind
            {
                return recognized_root_key(name);
            }
            None
        }
        ConfigToken::Symbol(_) => None,
    }
}

fn prefixed_method_key(token: &SourceToken, scan: &SourceScan<'_>) -> Option<&'static str> {
    let async_method = is_word(token, "async");
    if !async_method
        && !is_word(token, "get")
        && !is_word(token, "set")
        && token.kind != ConfigToken::Symbol('*')
    {
        return None;
    }
    let mut preview = scan.clone();
    let mut head = preview.next()?;
    if async_method && head.kind == ConfigToken::Symbol('*') {
        head = preview.next()?;
    }
    let name = if head.kind == ConfigToken::Symbol('[') {
        let key = preview.next()?;
        if preview.next()?.kind != ConfigToken::Symbol(']') {
            return None;
        }
        if let ConfigToken::Text(name) = key.kind {
            recognized_root_key(&name)
        } else {
            None
        }
    } else {
        match &head.kind {
            ConfigToken::Word(name) | ConfigToken::Text(name) => recognized_root_key(name),
            ConfigToken::Symbol(_) => None,
        }
    }?;
    let next = preview.next()?;
    if matches!(next.kind, ConfigToken::Symbol('(' | '<')) {
        Some(name)
    } else {
        None
    }
}

// Bounded TS type-argument lookahead, only committed for an angle span followed
// by a call/parameter list. Thus root commas in f<A, profile, B>(...) are not
// object-member separators, while a relational a < b, profile: ... still is.
// This is intentionally not a TypeScript expression evaluator.
fn skip_type_arguments(scan: &mut SourceScan<'_>) -> bool {
    let mut preview = scan.clone();
    let first = scan.position;
    let mut angle_depth = 1_usize;
    let mut bracket_closers = Vec::new();
    let mut saw_type = false;
    let mut after_comma = true;
    for _ in 0..256 {
        let Some(token) = preview.next() else {
            return false;
        };
        if token.end.saturating_sub(first) > 4096 {
            return false;
        }
        match token.kind {
            ConfigToken::Word(_) | ConfigToken::Text(_) => {
                if bracket_closers.is_empty() {
                    saw_type = true;
                    after_comma = false;
                }
            }
            ConfigToken::Symbol('<') if bracket_closers.is_empty() => {
                angle_depth += 1;
            }
            ConfigToken::Symbol('>') if bracket_closers.is_empty() => {
                if !saw_type || after_comma {
                    return false;
                }
                angle_depth -= 1;
                if angle_depth == 0 {
                    let mut suffix = preview.clone();
                    if suffix
                        .next()
                        .is_some_and(|next| next.kind == ConfigToken::Symbol('('))
                    {
                        *scan = preview;
                        return true;
                    }
                    return false;
                }
            }
            ConfigToken::Symbol(open @ ('{' | '[' | '(')) => {
                if bracket_closers.is_empty() {
                    saw_type = true;
                    after_comma = false;
                }
                bracket_closers.push(match open {
                    '{' => '}',
                    '[' => ']',
                    _ => ')',
                });
            }
            ConfigToken::Symbol(close @ ('}' | ']' | ')')) => {
                if bracket_closers.pop() != Some(close) {
                    return false;
                }
            }
            ConfigToken::Symbol(',') if bracket_closers.is_empty() => {
                if after_comma {
                    return false;
                }
                after_comma = true;
            }
            ConfigToken::Symbol(':' | ';' | '\0') if bracket_closers.is_empty() => {
                return false;
            }
            // Function type arrows are not generic-close delimiters.
            ConfigToken::Symbol('=') if preview.peek() == Some('>') => {
                preview.advance();
            }
            ConfigToken::Symbol(_) => {}
        }
    }
    false
}

fn is_word(token: &SourceToken, word: &str) -> bool {
    matches!(&token.kind, ConfigToken::Word(found) if found == word)
}

pub(super) fn root_auth_slice(source: &str) -> Result<Option<&str>, AuthConfigError> {
    let mut scan = SourceScan::new(source);
    let mut nesting = Vec::new();
    while let Some(token) = scan.next() {
        if nesting.is_empty() && is_word(&token, "export") {
            let Some(default) = scan.next() else {
                return Ok(None);
            };
            if !is_word(&default, "default") {
                continue;
            }
            let Some(open) = scan.next() else {
                return Ok(None);
            };
            if open.kind == ConfigToken::Symbol('{') {
                return exported_auth_slice(source, &mut scan);
            }
        }
        match token.kind {
            ConfigToken::Symbol(opener @ ('{' | '(' | '[')) => nesting.push(opener),
            ConfigToken::Symbol('}' | ')' | ']') => {
                nesting.pop();
            }
            _ => {}
        }
    }
    Ok(None)
}

fn exported_auth_slice<'a>(
    source: &'a str,
    scan: &mut SourceScan<'a>,
) -> Result<Option<&'a str>, AuthConfigError> {
    let mut nesting = Vec::new();
    let mut at_key = true;
    let mut found = None;
    let mut pending_start = None;
    while let Some(token) = scan.next() {
        if nesting.is_empty() {
            if matches!(token.kind, ConfigToken::Symbol(',' | '}')) {
                if let Some(start) = pending_start.take() {
                    found = source.get(start..token.start);
                }
                if token.kind == ConfigToken::Symbol('}') {
                    return Ok(found);
                }
                at_key = true;
                continue;
            }
            if at_key {
                at_key = false;
                let direct = static_root_key(&token, scan);
                let method = prefixed_method_key(&token, scan);
                match direct.or(method) {
                    Some("profile") => {
                        return Err(auth_config_error(
                            "root profile belongs in permissions manifest",
                        ));
                    }
                    Some("initialWindow") => {
                        return Err(auth_config_error(
                            "root initialWindow requires facade integration",
                        ));
                    }
                    _ => {}
                }
                if token.kind == ConfigToken::Symbol('[') && direct == Some("auth") {
                    return Err(auth_config_error("computed root auth keys are unsupported"));
                }
                if method == Some("auth") {
                    let detail = if found.is_some() || pending_start.is_some() {
                        "duplicate root auth property/accessor"
                    } else {
                        "root auth accessor is unsupported; use a literal property"
                    };
                    return Err(auth_config_error(detail));
                }
                if direct == Some("auth") {
                    if found.is_some() || pending_start.is_some() {
                        return Err(auth_config_error("duplicate root auth property"));
                    }
                    let colon = scan
                        .next()
                        .ok_or_else(|| auth_config_error("unfinished root auth property"))?;
                    if colon.kind != ConfigToken::Symbol(':') {
                        return Err(auth_config_error(
                            "root auth requires a literal value, not shorthand or a method",
                        ));
                    }
                    pending_start = Some(colon.end);
                    continue;
                }
            }
        }
        if nesting.is_empty() && token.kind == ConfigToken::Symbol('<') && skip_type_arguments(scan)
        {
            continue;
        }
        match token.kind {
            ConfigToken::Symbol(opener @ ('{' | '(' | '[')) => nesting.push(match opener {
                '{' => '}',
                '(' => ')',
                _ => ']',
            }),
            ConfigToken::Symbol(closer @ ('}' | ')' | ']'))
                if nesting.pop() != Some(closer)
                    && (found.is_some() || pending_start.is_some()) =>
            {
                return Err(auth_config_error("unbalanced exported auth object"));
            }
            _ => {}
        }
    }
    if found.is_some() || pending_start.is_some() {
        Err(auth_config_error("unclosed exported auth object"))
    } else {
        Ok(None)
    }
}
