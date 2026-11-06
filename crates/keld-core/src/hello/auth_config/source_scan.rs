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
                ch if ch.is_ascii_alphabetic() || ch == '_' || ch == '$' => {
                    let start = self.position - ch.len_utf8();
                    while self.peek().is_some_and(|next| {
                        next.is_ascii_alphanumeric() || next == '_' || next == '$'
                    }) {
                        self.advance();
                    }
                    let word = self.source.get(start..self.position).unwrap_or_default();
                    if let Some(mode) = modes.last_mut() {
                        mode.1 = matches!(
                            word,
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
                ch if ch.is_whitespace() => {}
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
            if ch.is_whitespace() {
                continue;
            }
            if ch == '/' && self.comment() {
                continue;
            }
            let mut undecodable_string = false;
            let kind = if ch == '\'' || ch == '"' {
                self.raw_string(ch);
                let raw = self.source.get(start + ch.len_utf8()..self.position)?;
                let decoded = config_string(&mut raw.chars().peekable(), ch);
                // Legacy parsing intentionally treats malformed unrelated strings
                // as opaque. The facade root scanner must not mistake a valid JS
                // \xNN key for a different, harmless property name.
                undecodable_string = decoded.is_err();
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
            } else if ch.is_ascii_alphabetic() || ch == '_' || ch == '$' {
                while self
                    .peek()
                    .is_some_and(|next| next.is_ascii_alphanumeric() || next == '_' || next == '$')
                {
                    self.advance();
                }
                let word = self.source.get(start..self.position)?.to_owned();
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
            && matches!(&token.kind, ConfigToken::Text(key) if !matches!(key.as_str(), "initialWindow" | "renderer" | "auth"))
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
    if token.undecodable_string || token.kind == ConfigToken::Symbol('\\') {
        return Err(auth_config_error(
            "unresolved escape in exported root property name",
        ));
    }
    if method_modifier
        && matches!(
            &token.kind,
            ConfigToken::Word(key) | ConfigToken::Text(key)
                if matches!(key.as_str(), "initialWindow" | "renderer" | "auth")
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

fn root_accessor_error(
    token: &SourceToken,
    source: &str,
    property: &str,
    duplicate: bool,
) -> Option<AuthConfigError> {
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
                if token.kind == ConfigToken::Symbol('[') {
                    let mut preview = SourceScan::new(source.get(token.end..).unwrap_or_default());
                    if let Some(key) = preview.next()
                        && matches!(&key.kind, ConfigToken::Text(name) if name == property)
                        && preview
                            .next()
                            .is_some_and(|end| end.kind == ConfigToken::Symbol(']'))
                    {
                        return Err(auth_config_error("computed root auth keys are unsupported"));
                    }
                }
                if let Some(error) = root_accessor_error(
                    &token,
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
