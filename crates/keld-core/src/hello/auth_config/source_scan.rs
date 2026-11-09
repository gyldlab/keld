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
            let kind = if ch == '\'' || ch == '"' {
                self.raw_string(ch);
                let raw = self.source.get(start + ch.len_utf8()..self.position)?;
                let decoded = config_string(&mut raw.chars().peekable(), ch);
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
            });
        }
    }
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

fn auth_accessor_prefix(source: &str) -> bool {
    let mut preview = SourceScan::new(source);
    let Some(name) = preview.next() else {
        return false;
    };
    let auth = if name.kind == ConfigToken::Symbol('[') {
        preview
            .next()
            .is_some_and(|key| matches!(&key.kind, ConfigToken::Text(name) if name == "auth"))
            && preview
                .next()
                .is_some_and(|end| end.kind == ConfigToken::Symbol(']'))
    } else {
        matches!(&name.kind, ConfigToken::Word(key) | ConfigToken::Text(key) if key == "auth")
    };
    auth && preview
        .next()
        .is_some_and(|open| open.kind == ConfigToken::Symbol('('))
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
                let auth = matches!(&token.kind, ConfigToken::Word(key) | ConfigToken::Text(key) if key == "auth");
                if token.kind == ConfigToken::Symbol('[') {
                    let mut preview = SourceScan::new(source.get(token.end..).unwrap_or_default());
                    if let Some(key) = preview.next()
                        && matches!(&key.kind, ConfigToken::Text(name) if name == "auth")
                        && preview
                            .next()
                            .is_some_and(|end| end.kind == ConfigToken::Symbol(']'))
                    {
                        return Err(auth_config_error("computed root auth keys are unsupported"));
                    }
                }
                if matches!(&token.kind, ConfigToken::Word(word) if word == "get" || word == "set")
                    && auth_accessor_prefix(source.get(token.end..).unwrap_or_default())
                {
                    let detail = if found.is_some() || pending_start.is_some() {
                        "duplicate root auth property/accessor"
                    } else {
                        "root auth accessor is unsupported; use a literal property"
                    };
                    return Err(auth_config_error(detail));
                }
                if auth {
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
