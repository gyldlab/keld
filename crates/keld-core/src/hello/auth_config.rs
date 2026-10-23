//! Literal-only static authentication configuration parsing, not config evaluation.
//! Provider validation remains with keld-native.

mod source_scan;

use self::source_scan::root_auth_slice;

/// Static-auth configuration syntax error; no configuration code was executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthConfigError {
    detail: &'static str,
}

impl std::fmt::Display for AuthConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.detail {
            "root profile belongs in permissions manifest" => write!(
                f,
                "KELD-CORE-035: root profile in keld.config.ts is unsupported. Declare the exact root profile in keld.permissions.jsonc instead."
            ),
            "root initialWindow requires facade integration" => write!(
                f,
                "KELD-CORE-035: root initialWindow in keld.config.ts is unsupported until GH #446 facade Ready integration; no renderer fallback is allowed."
            ),
            _ => write!(
                f,
                "KELD-CORE-035: invalid static auth configuration: {}. Use export default {{ auth: {{ \"entra-common\": {{ clientId: \"...\" }} }} }} as const with literal strings; remove expressions, computed keys and spreads.",
                self.detail
            ),
        }
    }
}

impl std::error::Error for AuthConfigError {}

fn auth_config_error(detail: &'static str) -> AuthConfigError {
    AuthConfigError { detail }
}

#[derive(Debug, PartialEq, Eq)]
enum ConfigToken {
    Word(String),
    Text(String),
    Symbol(char),
}

/// Extracts a literal root `auth` property without evaluating configuration code.
///
/// An absent root property returns an empty map without restricting legacy
/// imports, other values, duplicates, source size or nesting. Comments, strings,
/// templates, regular expressions and nested properties are not root properties.
/// Once found, `auth` must be an object of literal profile objects and strings,
/// with quoted/identifier keys, comments and trailing commas. Provider policy
/// and missing-client/redirect validation remain with the Native owner.
/// Existing name/entry/renderer readers retain their previous behavior.
///
/// # Errors
/// Returns [`AuthConfigError`] for unsupported/malformed or duplicate root auth,
/// or an auth slice over 64 KiB. Only an inline `export default { ... }` root is
/// a source of auth configuration; computed exports are not evaluated.
pub fn auth_configuration_from_config_ts(
    source: &str,
) -> Result<keld_native::auth::AuthConfiguration, AuthConfigError> {
    let Some(auth) = root_auth_slice(source)? else {
        return Ok(keld_native::auth::AuthConfiguration::new());
    };
    if auth.len() > 64 * 1024 {
        return Err(auth_config_error("auth value exceeds 64 KiB"));
    }
    let tokens = config_tokens(auth)?;
    let mut parser = ConfigReader {
        tokens: &tokens,
        position: 0,
    };
    let configuration = parser.auth()?;
    if parser.position != tokens.len() {
        return Err(auth_config_error(
            "auth must be a literal object without expressions",
        ));
    }
    Ok(configuration)
}

fn config_line_terminator(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn config_tokens(source: &str) -> Result<Vec<ConfigToken>, AuthConfigError> {
    let mut chars = source.chars().peekable();
    let mut tokens = Vec::new();
    while let Some(ch) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for next in chars.by_ref() {
                if config_line_terminator(next) {
                    break;
                }
            }
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut closed = false;
            while let Some(next) = chars.next() {
                if next == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    closed = true;
                    break;
                }
            }
            if !closed {
                return Err(auth_config_error("unterminated block comment"));
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            tokens.push(ConfigToken::Text(config_string(&mut chars, ch)?));
        } else if ch.is_ascii_alphabetic() || ch == '_' || ch == '$' {
            let mut word = String::from(ch);
            while chars
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
            {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            tokens.push(ConfigToken::Word(word));
        } else {
            // Backtick expressions cannot be skipped safely as quoted literals.
            if ch == '`' {
                return Err(auth_config_error("template literals are unsupported"));
            }
            tokens.push(ConfigToken::Symbol(ch));
        }
    }
    Ok(tokens)
}

fn config_string(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    quote: char,
) -> Result<String, AuthConfigError> {
    let mut json = String::from("\"");
    while let Some(ch) = chars.next() {
        if ch == quote {
            json.push('"');
            return serde_json::from_str(&json)
                .map_err(|_| auth_config_error("invalid string escape"));
        }
        if ch == '\\' {
            let escaped = chars
                .next()
                .ok_or_else(|| auth_config_error("unfinished string escape"))?;
            if escaped == '\'' && quote == '\'' {
                json.push('\'');
            } else if matches!(
                escaped,
                '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' | 'u'
            ) {
                json.push('\\');
                json.push(escaped);
                if escaped == 'u' {
                    for _ in 0..4 {
                        let hex =
                            chars
                                .next()
                                .filter(char::is_ascii_hexdigit)
                                .ok_or_else(|| {
                                    auth_config_error("Unicode escape requires four hex digits")
                                })?;
                        json.push(hex);
                    }
                }
            } else {
                return Err(auth_config_error("unsupported string escape"));
            }
        } else if ch == '"' {
            json.push_str("\\\"");
        } else if ch.is_control() {
            return Err(auth_config_error("literal control character in string"));
        } else {
            json.push(ch);
        }
    }
    Err(auth_config_error("unterminated quoted string"))
}

struct ConfigReader<'a> {
    tokens: &'a [ConfigToken],
    position: usize,
}

impl ConfigReader<'_> {
    fn take_symbol(&mut self, symbol: char) -> bool {
        if self.tokens.get(self.position) == Some(&ConfigToken::Symbol(symbol)) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn symbol(&mut self, symbol: char) -> Result<(), AuthConfigError> {
        if self.take_symbol(symbol) {
            Ok(())
        } else {
            Err(auth_config_error("expected literal object punctuation"))
        }
    }

    fn key(&mut self) -> Result<String, AuthConfigError> {
        let key = match self.tokens.get(self.position) {
            Some(ConfigToken::Text(key) | ConfigToken::Word(key)) => key.clone(),
            _ => {
                return Err(auth_config_error(
                    "property keys must be literal; no computed key or spread",
                ));
            }
        };
        self.position += 1;
        Ok(key)
    }

    fn text(&mut self) -> Result<String, AuthConfigError> {
        let Some(ConfigToken::Text(value)) = self.tokens.get(self.position) else {
            return Err(auth_config_error(
                "auth values must be quoted strings or objects",
            ));
        };
        self.position += 1;
        Ok(value.clone())
    }

    fn object_separator(&mut self) -> Result<(), AuthConfigError> {
        if self.take_symbol(',')
            || self.tokens.get(self.position) == Some(&ConfigToken::Symbol('}'))
        {
            Ok(())
        } else {
            Err(auth_config_error("expected comma or closing object brace"))
        }
    }

    fn auth(&mut self) -> Result<keld_native::auth::AuthConfiguration, AuthConfigError> {
        self.symbol('{')?;
        let mut auth = keld_native::auth::AuthConfiguration::new();
        while !self.take_symbol('}') {
            let key = self.key()?;
            self.symbol(':')?;
            let profile = self.profile()?;
            if auth.insert(key, profile).is_some() {
                return Err(auth_config_error("duplicate auth profile"));
            }
            self.object_separator()?;
        }
        Ok(auth)
    }

    fn profile(&mut self) -> Result<keld_native::auth::AuthProfileConfiguration, AuthConfigError> {
        self.symbol('{')?;
        let mut fields = serde_json::Map::new();
        while !self.take_symbol('}') {
            let key = self.key()?;
            self.symbol(':')?;
            let value = self.text()?;
            if fields
                .insert(key, serde_json::Value::String(value))
                .is_some()
            {
                return Err(auth_config_error("duplicate auth configuration field"));
            }
            self.object_separator()?;
        }
        serde_json::from_value(serde_json::Value::Object(fields))
            .map_err(|_| auth_config_error("invalid auth configuration fields"))
    }
}
