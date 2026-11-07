//! Identify the exported root auth literal while ignoring unrelated TypeScript.
use super::{
    AuthConfigError, ConfigToken, auth_config_error, config_line_terminator, config_string,
};

// This scanner locates a static exported object's root property; it does not
// validate/evaluate unrelated TypeScript or derive provider policy. Strict
// syntax/size checks apply only after a literal root auth property is found.
#[derive(Debug)]
struct SourceToken {
    kind: ConfigToken,
    start: usize,
    end: usize,
    undecodable_string: bool,
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
            let mut undecodable_string = false;
            let kind = if ch == '\'' || ch == '"' {
                self.raw_string(ch);
                let raw = self.source.get(start + ch.len_utf8()..self.position)?;
                let decoded = source_string(raw, ch);
                // Legacy parsing intentionally treats malformed unrelated strings
                // as opaque. The facade root scanner must not mistake a valid JS
                // \xNN key for a different, harmless property name.
                undecodable_string = config_string(&mut raw.chars().peekable(), ch).is_err();
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
                undecodable_string,
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
    root_named_property_slice(source, "auth")
}

/// Use the same exported-root ownership scanner for facade and auth declarations.
pub(super) fn root_named_property_slice<'a>(
    source: &'a str,
    property: &str,
) -> Result<Option<&'a str>, AuthConfigError> {
    root_property_slice(source, property, false)
}

/// Facade selection must not silently ignore root spreads or unresolved keys:
/// either could supply `initialWindow`, `renderer`, or `auth` at runtime.
/// Keep the existing permissive auth-absent reader unchanged for schema one.
pub(super) fn root_facade_property_slice<'a>(
    source: &'a str,
    property: &str,
) -> Result<Option<&'a str>, AuthConfigError> {
    root_property_slice(source, property, true)
}

fn root_property_slice<'a>(
    source: &'a str,
    property: &str,
    strict_root: bool,
) -> Result<Option<&'a str>, AuthConfigError> {
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
            let Some(mut open) = scan.next() else {
                return Ok(None);
            };
            // Facade boot mode follows an exported object literal even when
            // the literal is wrapped in redundant grouping parentheses.
            // The old permissive auth reader deliberately retains its
            // direct-literal-only behavior for legacy renderer configurations.
            let mut grouping = 0_usize;
            while strict_root && open.kind == ConfigToken::Symbol('(') {
                grouping += 1;
                open = scan
                    .next()
                    .ok_or_else(|| auth_config_error("unfinished parenthesized config root"))?;
            }
            if open.kind == ConfigToken::Symbol('{') {
                let selected = exported_property_slice(source, &mut scan, property, strict_root)?;
                if grouping != 0 {
                    for _ in 0..grouping {
                        let mut close = scan.next();
                        if close.as_ref().is_some_and(|token| is_word(token, "as")) {
                            if !scan.next().is_some_and(|token| is_word(&token, "const")) {
                                return Err(auth_config_error(
                                    "only as const is supported inside grouped config roots",
                                ));
                            }
                            close = scan.next();
                        }
                        if close
                            .as_ref()
                            .is_some_and(|token| is_word(token, "satisfies"))
                        {
                            close = validate_satisfies_type_suffix(&mut scan)?.0;
                        }
                        if !close.is_some_and(|token| token.kind == ConfigToken::Symbol(')')) {
                            return Err(auth_config_error(
                                "parenthesized config root must close after its literal object",
                            ));
                        }
                    }
                }
                // Both direct and parenthesized object roots must export the
                // literal itself. A subsequent `&&`, member access or call
                // could change the selected initial-window mode at runtime.
                if strict_root {
                    validate_literal_root_suffix(&mut scan)?;
                }
                return Ok(selected);
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

fn validate_literal_root_suffix(scan: &mut SourceScan<'_>) -> Result<(), AuthConfigError> {
    let mut previous_end = scan.position;
    let suffix = scan.next();
    if suffix
        .as_ref()
        .is_some_and(|token| starts_following_statement(scan, previous_end, token))
    {
        return Ok(());
    }
    let end = match suffix.as_ref().map(|token| &token.kind) {
        None | Some(ConfigToken::Symbol(';')) => return Ok(()),
        Some(ConfigToken::Word(word)) if word == "as" => {
            if !scan.next().is_some_and(|token| is_word(&token, "const")) {
                return Err(auth_config_error(
                    "literal config supports only an optional as const assertion",
                ));
            }
            previous_end = scan.position;
            let end = scan.next();
            if end
                .as_ref()
                .is_some_and(|token| is_word(token, "satisfies"))
            {
                let (end, type_end) = validate_satisfies_type_suffix(scan)?;
                previous_end = type_end;
                end
            } else {
                end
            }
        }
        Some(ConfigToken::Word(word)) if word == "satisfies" => {
            let (end, type_end) = validate_satisfies_type_suffix(scan)?;
            previous_end = type_end;
            end
        }
        _ => {
            return Err(auth_config_error(
                "config root must directly export the literal",
            ));
        }
    };
    if end.is_none_or(|token| {
        token.kind == ConfigToken::Symbol(';')
            || starts_following_statement(scan, previous_end, &token)
    }) {
        Ok(())
    } else {
        Err(auth_config_error(
            "config root has an unsupported expression suffix",
        ))
    }
}

fn starts_following_statement(
    scan: &SourceScan<'_>,
    previous_end: usize,
    token: &SourceToken,
) -> bool {
    // A new statement after a line terminator is separate from the literal
    // export, even with a comment between the two tokens. Do not mistake
    // operators, member access or calls across a newline for a new statement.
    let starts_statement = matches!(
        &token.kind,
        ConfigToken::Word(word)
            if !matches!(word.as_str(), "in" | "instanceof" | "as" | "satisfies")
    );
    starts_statement
        && scan
            .source
            .get(previous_end..token.start)
            .is_some_and(|between| between.chars().any(config_line_terminator))
}

fn validate_satisfies_type_suffix(
    scan: &mut SourceScan<'_>,
) -> Result<(Option<SourceToken>, usize), AuthConfigError> {
    let Some(first) = scan.next() else {
        return Err(auth_config_error("satisfies requires a static type"));
    };
    let mut previous_end = first.end;
    match first.kind {
        ConfigToken::Word(_) => {}
        ConfigToken::Symbol('{') => {
            previous_end = consume_static_type_group(scan, '}')?;
        }
        _ => return Err(auth_config_error("satisfies requires a static type")),
    }
    loop {
        match scan.next() {
            Some(token) if token.kind == ConfigToken::Symbol('.') => {
                let Some(segment) = scan.next() else {
                    return Err(auth_config_error("invalid qualified type name"));
                };
                if !matches!(segment.kind, ConfigToken::Word(_)) {
                    return Err(auth_config_error("invalid qualified type name"));
                }
                previous_end = segment.end;
            }
            Some(token) if token.kind == ConfigToken::Symbol('<') => {
                previous_end = consume_static_type_group(scan, '>')?;
            }
            Some(token) if token.kind == ConfigToken::Symbol('[') => {
                previous_end = consume_static_type_group(scan, ']')?;
            }
            end => return Ok((end, previous_end)),
        }
    }
}

/// Consume inert type syntax, leaving the exported expression's delimiters
/// outside the group untouched. This scanner never evaluates a type or value.
fn consume_static_type_group(
    scan: &mut SourceScan<'_>,
    closing: char,
) -> Result<usize, AuthConfigError> {
    let mut closings = vec![closing];
    while let Some(token) = scan.next() {
        let ConfigToken::Symbol(symbol) = token.kind else {
            continue;
        };
        match symbol {
            '{' => closings.push('}'),
            '[' => closings.push(']'),
            '(' => closings.push(')'),
            '<' => closings.push('>'),
            // The arrow in a function type is not a generic closer.
            '>' if scan.source[..token.start].ends_with('=') => {}
            '}' | ']' | ')' | '>' => {
                if closings.pop() != Some(symbol) {
                    return Err(auth_config_error("mismatched static type delimiter"));
                }
                if closings.is_empty() {
                    return Ok(token.end);
                }
            }
            _ => {}
        }
    }
    Err(auth_config_error("unterminated static type"))
}

/// Only a computed, quoted, unrelated root key is unambiguous. Even a quoted
/// reserved key must use normal property syntax, so it cannot escape facade
/// selection or the forbidden-field check through a method/accessor prefix.
fn harmless_computed_root_key(source_after_open: &str) -> bool {
    let mut preview = SourceScan::new(source_after_open);
    preview.next().is_some_and(|token| {
        !token.undecodable_string
            && matches!(&token.kind, ConfigToken::Text(key) if !matches!(key.as_str(), "initialWindow" | "renderer" | "auth" | "profile"))
    }) && preview
        .next()
        .is_some_and(|token| token.kind == ConfigToken::Symbol(']'))
}

fn property_accessor_prefix(source: &str, property: &str) -> bool {
    let mut preview = SourceScan::new(source);
    let Some(name) = preview.next() else {
        return false;
    };
    let named = if name.kind == ConfigToken::Symbol('[') {
        preview
            .next()
            .is_some_and(|key| matches!(&key.kind, ConfigToken::Text(name) if name == property))
            && preview
                .next()
                .is_some_and(|end| end.kind == ConfigToken::Symbol(']'))
    } else {
        matches!(&name.kind, ConfigToken::Word(key) | ConfigToken::Text(key) if key == property)
    };
    named
        && preview
            .next()
            .is_some_and(|open| open.kind == ConfigToken::Symbol('('))
}

/// Inspect only the root member's key/header, not its nested value. A JS
/// Unicode escape in an identifier or a quoted key escape we cannot decode
/// could spell a forbidden boot field, so it must not become an absent key.
fn validate_facade_root_member_token(
    token: &SourceToken,
    source: &str,
    method_modifier: bool,
    at_key: bool,
) -> Result<(), AuthConfigError> {
    if token.undecodable_string
        || token.kind == ConfigToken::Symbol('\\')
        || (matches!(token.kind, ConfigToken::Word(_))
            && source
                .get(token.start..token.end)
                .is_some_and(|raw| raw.contains('\\')))
    {
        return Err(auth_config_error(
            "unresolved escape in exported root property name",
        ));
    }
    if method_modifier
        && matches!(
            &token.kind,
            ConfigToken::Word(key) | ConfigToken::Text(key)
                if matches!(key.as_str(), "initialWindow" | "renderer" | "auth" | "profile")
        )
    {
        return Err(auth_config_error(
            "reserved root method is unsupported for facade boot",
        ));
    }
    if (at_key
        && token.kind == ConfigToken::Symbol('.')
        && source
            .get(token.start..)
            .is_some_and(|remaining| remaining.starts_with("...")))
        || ((at_key || method_modifier)
            && token.kind == ConfigToken::Symbol('[')
            && !harmless_computed_root_key(source.get(token.end..).unwrap_or_default()))
    {
        return Err(auth_config_error(
            "unresolved computed root key or object spread",
        ));
    }
    Ok(())
}

/// Type parameters on a root method do not create new object members.
/// The closing `>` remains a parser boundary, including nested generics.
fn track_method_type_bracket(
    token: &SourceToken,
    source: &str,
    nesting: &mut Vec<char>,
    may_open_generic: bool,
) {
    match token.kind {
        ConfigToken::Symbol('<')
            if nesting.last() == Some(&'>') || (nesting.is_empty() && may_open_generic) =>
        {
            nesting.push('>');
        }
        ConfigToken::Symbol('>')
            if nesting.last() == Some(&'>')
                && !source
                    .get(..token.start)
                    .is_some_and(|before| before.ends_with('=')) =>
        {
            nesting.pop();
        }
        _ => {}
    }
}

fn update_member_signature(
    token: &SourceToken,
    at_key: bool,
    value_started: bool,
    method_signature: bool,
) -> (bool, bool) {
    if token.kind == ConfigToken::Symbol('(') && !at_key && !value_started {
        (value_started, true)
    } else if token.kind == ConfigToken::Symbol(':') && !method_signature {
        (true, method_signature)
    } else {
        (value_started, method_signature)
    }
}

fn root_member_error(
    token: &SourceToken,
    scan: &SourceScan<'_>,
    source: &str,
    property: &str,
    duplicate: bool,
) -> Option<AuthConfigError> {
    if static_root_key(token, scan).or_else(|| prefixed_method_key(token, scan)) == Some("profile")
    {
        return Some(auth_config_error(
            "root profile belongs in permissions manifest",
        ));
    }
    if token.kind == ConfigToken::Symbol('[') {
        let mut preview = SourceScan::new(source.get(token.end..).unwrap_or_default());
        if let Some(key) = preview.next()
            && matches!(&key.kind, ConfigToken::Text(name) if name == property)
            && preview
                .next()
                .is_some_and(|end| end.kind == ConfigToken::Symbol(']'))
        {
            return Some(auth_config_error("computed root auth keys are unsupported"));
        }
    }
    if !matches!(&token.kind, ConfigToken::Word(word) if matches!(word.as_str(), "get" | "set"))
        || !property_accessor_prefix(source.get(token.end..).unwrap_or_default(), property)
    {
        return None;
    }
    Some(auth_config_error(if duplicate {
        "duplicate root auth property/accessor"
    } else {
        "root auth accessor is unsupported; use a literal property"
    }))
}

fn advance_exported_root_nesting(
    token: &SourceToken,
    source: &str,
    nesting: &mut Vec<char>,
    strict_root: bool,
    may_open_generic: bool,
    require_balanced: bool,
) -> Result<(), AuthConfigError> {
    if strict_root {
        track_method_type_bracket(token, source, nesting, may_open_generic);
    }
    match token.kind {
        ConfigToken::Symbol(opener @ ('{' | '(' | '[')) => nesting.push(match opener {
            '{' => '}',
            '(' => ')',
            _ => ']',
        }),
        ConfigToken::Symbol(closer @ ('}' | ')' | ']'))
            if nesting.pop() != Some(closer) && require_balanced =>
        {
            return Err(auth_config_error("unbalanced exported auth object"));
        }
        _ => {}
    }
    Ok(())
}

fn next_value_generic_prefix(token: &SourceToken, current: bool, root_value_prefix: bool) -> bool {
    if root_value_prefix && token.kind == ConfigToken::Symbol(':') {
        true
    } else {
        current
            && (token.kind == ConfigToken::Symbol(':')
                || matches!(&token.kind, ConfigToken::Word(word) if word == "async"))
    }
}

fn exported_property_slice<'a>(
    source: &'a str,
    scan: &mut SourceScan<'a>,
    property: &str,
    strict_root: bool,
) -> Result<Option<&'a str>, AuthConfigError> {
    let mut nesting = Vec::new();
    let mut at_key = true;
    let mut value_started = false;
    // An unrelated value can begin with a generic arrow expression:
    // helper: <T, renderer>(value: T) => value. Its type-parameter commas
    // are not exported root-property separators.
    let mut value_generic_head = false;
    let mut method_signature = false;
    let mut method_modifier = false;
    let mut found = None;
    let mut pending_start = None;
    while let Some(token) = scan.next() {
        let generic_at_token = value_generic_head;
        if nesting.is_empty() {
            if matches!(token.kind, ConfigToken::Symbol(',' | '}')) {
                if let Some(start) = pending_start.take() {
                    found = source.get(start..token.start);
                }
                if token.kind == ConfigToken::Symbol('}') {
                    return Ok(found);
                }
                at_key = true;
                value_started = false;
                value_generic_head = false;
                method_modifier = false;
                method_signature = false;
                continue;
            }
            // Before the value colon, method modifiers and ambiguous key
            // escapes must be examined even after the first root token.
            if strict_root && !value_started && !method_signature {
                validate_facade_root_member_token(&token, source, method_modifier, at_key)?;
            }
            value_generic_head = next_value_generic_prefix(
                &token,
                value_generic_head,
                !at_key && !value_started && !method_signature,
            );
            (value_started, method_signature) =
                update_member_signature(&token, at_key, value_started, method_signature);
            if at_key {
                at_key = false;
                method_modifier = strict_root
                    && (token.kind == ConfigToken::Symbol('*')
                        || matches!(&token.kind, ConfigToken::Word(word) if matches!(word.as_str(), "async" | "get" | "set")));
                let named = matches!(&token.kind, ConfigToken::Word(key) | ConfigToken::Text(key) if key == property);
                if let Some(error) = root_member_error(
                    &token,
                    scan,
                    source,
                    property,
                    found.is_some() || pending_start.is_some(),
                ) {
                    return Err(error);
                }
                if named {
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
                    value_started = true;
                    continue;
                }
            } else if method_modifier {
                // Only `async *` extends the modifier prefix. Once a key is
                // read, further words belong to signatures or type metadata.
                method_modifier = token.kind == ConfigToken::Symbol('*');
            }
        }
        if nesting.is_empty() && token.kind == ConfigToken::Symbol('<') && skip_type_arguments(scan)
        {
            continue;
        }
        let may_open_generic = !at_key
            && (!value_started || method_signature || generic_at_token || value_generic_head);
        advance_exported_root_nesting(
            &token,
            source,
            &mut nesting,
            strict_root,
            may_open_generic,
            found.is_some() || pending_start.is_some(),
        )?;
    }
    if found.is_some() || pending_start.is_some() {
        Err(auth_config_error("unclosed exported auth object"))
    } else {
        Ok(None)
    }
}
