//! Literal-only static authentication configuration parsing, not config evaluation.
//! Provider validation remains with keld-native.

mod source_scan;

use self::source_scan::{root_auth_slice, root_facade_property_slice};

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

/// A facade boot declaration rejected by the shared literal root-property reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitialWindowConfigError {
    detail: &'static str,
}

impl std::fmt::Display for InitialWindowConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "KELD-CORE-035: invalid initialWindow declaration: {}. Use only initialWindow: \"none\" on the exported config root; remove renderer and auth from facade boots.",
            self.detail
        )
    }
}

impl std::error::Error for InitialWindowConfigError {}

/// Invalid explicit version metadata; no application configuration is executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppVersionConfigError {
    detail: &'static str,
}

impl std::fmt::Display for AppVersionConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "KELD-CORE-035: invalid application version: {}. Declare one nonempty literal root version string in keld.config.ts, without expressions, accessors, computed keys or spreads.",
            self.detail
        )
    }
}

impl std::error::Error for AppVersionConfigError {}

/// Extracts only explicit version metadata through the existing root scanner.
/// An absent value stays absent; this does not consult package files or the environment.
///
/// # Errors
/// Rejects ambiguous declarations and values outside the bounded string contract.
pub fn version_from_config_ts(source: &str) -> Result<Option<String>, AppVersionConfigError> {
    let invalid = |_| AppVersionConfigError {
        detail: "the exported root version must be unique and literal",
    };
    let Some(value) = root_facade_property_slice(source, "version").map_err(invalid)? else {
        return Ok(None);
    };
    if value.len() > 64 * 1024 {
        return Err(AppVersionConfigError {
            detail: "the literal source exceeds 64 KiB",
        });
    }
    let tokens = config_tokens(value).map_err(invalid)?;
    let [ConfigToken::Text(version)] = tokens.as_slice() else {
        return Err(AppVersionConfigError {
            detail: "only a literal string is supported",
        });
    };
    validate_app_version(version)?;
    Ok(Some(version.clone()))
}

pub(crate) fn validate_app_version(value: &str) -> Result<(), AppVersionConfigError> {
    if value.is_empty() || value.len() > 4096 || value.contains('\0') {
        return Err(AppVersionConfigError {
            detail: "the value must be nonempty, NUL-free and at most 4096 UTF-8 bytes",
        });
    }
    Ok(())
}

pub(crate) fn deserialize_optional_app_version<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // `default` handles an omitted field. A present null must not become absence.
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;
    validate_app_version(&value).map_err(serde::de::Error::custom)?;
    Ok(Some(value))
}

/// Invalid literal user-agent application metadata; no config code was executed.
#[derive(Debug, PartialEq, Eq)]
pub struct ApplicationNameForUserAgentConfigError {
    detail: String,
}

impl std::fmt::Display for ApplicationNameForUserAgentConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "KELD-CORE-035: invalid applicationNameForUserAgent: {}. Declare one literal root string with 1..=256 visible ASCII bytes and internal spaces only; remove expressions, duplicates, computed keys, accessors and spreads.",
            self.detail
        )
    }
}

impl std::error::Error for ApplicationNameForUserAgentConfigError {}

/// Reads optional cold application metadata through the shared literal scanner.
/// No package, environment, runtime fact or permission supplies a fallback.
///
/// # Errors
/// Rejects ambiguous/nonliteral syntax and values refused by the WV value owner.
pub fn application_name_for_user_agent_from_config_ts(
    source: &str,
) -> Result<Option<keld_wv::ApplicationNameForUserAgent>, ApplicationNameForUserAgentConfigError> {
    let invalid = |_| ApplicationNameForUserAgentConfigError {
        detail: String::from("the exported root property must be unique and literal"),
    };
    let Some(value) =
        root_facade_property_slice(source, "applicationNameForUserAgent").map_err(invalid)?
    else {
        return Ok(None);
    };
    if value.len() > 64 * 1024 {
        return Err(ApplicationNameForUserAgentConfigError {
            detail: String::from("the literal source exceeds 64 KiB"),
        });
    }
    let tokens = config_tokens(value).map_err(invalid)?;
    let [ConfigToken::Text(text)] = tokens.as_slice() else {
        return Err(ApplicationNameForUserAgentConfigError {
            detail: String::from("only a literal string is supported"),
        });
    };
    keld_wv::ApplicationNameForUserAgent::new(text.clone())
        .map(Some)
        .map_err(|error| ApplicationNameForUserAgentConfigError {
            detail: error.to_string(),
        })
}

pub(crate) fn deserialize_optional_application_name_for_user_agent<'de, D>(
    deserializer: D,
) -> Result<Option<keld_wv::ApplicationNameForUserAgent>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Only omission selects default; a present null must not erase the request.
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;
    keld_wv::ApplicationNameForUserAgent::new(value)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

/// Whether the exported root selects an app-owned first window.
///
/// Missing config retains the renderer-declared schema-1 boot. A present root
/// declaration must be the one literal value "none" with no root renderer or
/// auth member. The shared auth source scanner ignores comments, strings,
/// nested properties, and unsupported dynamic exports without running TS.
///
/// # Errors
///
/// Rejects ambiguous, computed, duplicated or conflicting root declarations.
pub fn initial_window_from_config_ts(source: &str) -> Result<bool, InitialWindowConfigError> {
    let scan_error = |_| InitialWindowConfigError {
        detail: "the root property must be unique and literal",
    };
    let Some(value) = root_facade_property_slice(source, "initialWindow").map_err(scan_error)?
    else {
        return Ok(false);
    };
    let tokens = config_tokens(value).map_err(scan_error)?;
    if !matches!(tokens.as_slice(), [ConfigToken::Text(text)] if text == "none") {
        return Err(InitialWindowConfigError {
            detail: "the only supported value is the literal string \"none\"",
        });
    }
    for field in ["renderer", "auth"] {
        if root_facade_property_slice(source, field)
            .map_err(scan_error)?
            .is_some()
        {
            return Err(InitialWindowConfigError {
                detail: "renderer and auth fields are forbidden with initialWindow",
            });
        }
    }
    Ok(true)
}

fn auth_config_error(detail: &'static str) -> AuthConfigError {
    AuthConfigError { detail }
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

/// Literal cooperative app-id declaration; not an application principal or path.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct SingleInstanceDeclaration {
    #[serde(deserialize_with = "deserialize_single_instance_id")]
    id: String,
}

impl SingleInstanceDeclaration {
    /// Returns the admitted local coordination id, never a grant or principal.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// The single-instance declaration was not one bounded literal config object.
#[derive(Debug, Clone, Copy)]
pub struct SingleInstanceConfigError;
impl std::fmt::Display for SingleInstanceConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "KELD-CORE-035: invalid singleInstance declaration. Use one literal singleInstance: {{ id: \"org.keld.firstproof.drawio\" }} object with a 1..128-byte lowercase dotted id; remove expressions, accessors, computed keys, extra fields and spreads."
        )
    }
}
impl std::error::Error for SingleInstanceConfigError {}

fn validate_single_instance_id(id: &str) -> Result<(), SingleInstanceConfigError> {
    if id.is_empty()
        || id.len() > 128
        || !id.split('.').all(|part| {
            !part.is_empty()
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
    {
        return Err(SingleInstanceConfigError);
    }
    Ok(())
}
fn deserialize_single_instance_id<'de, D>(d: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let id = <String as serde::Deserialize>::deserialize(d)?;
    validate_single_instance_id(&id).map_err(serde::de::Error::custom)?;
    Ok(id)
}
/// Extracts the literal-only single-instance field using the one root scanner.
/// # Errors
/// Refuses unsupported dynamic or ambiguous values without evaluating config code.
pub fn single_instance_from_config_ts(
    source: &str,
) -> Result<Option<SingleInstanceDeclaration>, SingleInstanceConfigError> {
    let Some(value) = root_facade_property_slice(source, "singleInstance")
        .map_err(|_| SingleInstanceConfigError)?
    else {
        return Ok(None);
    };
    if value.len() > 1024 {
        return Err(SingleInstanceConfigError);
    }
    let tokens = config_tokens(value).map_err(|_| SingleInstanceConfigError)?;
    let trimmed = match tokens.as_slice() {
        [
            head @ ..,
            ConfigToken::Symbol(','),
            ConfigToken::Symbol('}'),
        ] => {
            let mut copy = head.to_vec();
            copy.push(ConfigToken::Symbol('}'));
            copy
        }
        _ => tokens,
    };
    let [
        ConfigToken::Symbol('{'),
        key,
        ConfigToken::Symbol(':'),
        ConfigToken::Text(id),
        ConfigToken::Symbol('}'),
    ] = trimmed.as_slice()
    else {
        return Err(SingleInstanceConfigError);
    };
    if !matches!(key, ConfigToken::Word(key) | ConfigToken::Text(key) if key == "id") {
        return Err(SingleInstanceConfigError);
    }
    validate_single_instance_id(id)?;
    Ok(Some(SingleInstanceDeclaration { id: id.clone() }))
}
