//! Phase 1 hello-world window (delegates to `keld-wv`).
//!
//! Callers go through this module so `AppKit`/`NSApplication` and the UI thread
//! stay inside the host stack (`keld-core` → `keld-wv` `WKWebView` backend).
//! Invoke on the process main thread; do not start a second event loop.

use std::fs;
use std::path::Path;

use keld_wv::{HELLO_HTML, WvError, run_hello_window as wv_run_hello};

/// Prepares the current process for its platform webview backend.
///
/// On Linux NVIDIA+Wayland this may replace the process through exact
/// `/proc/self/exe` with the DMA-BUF safe-mode environment applied. Shipping
/// binaries call this from their process-entry dispatcher before creating
/// threads or non-repeatable state. Other platforms return immediately.
///
/// # Errors
///
/// Returns [`WvError`] if Linux exact-self re-exec fails.
pub fn prepare_webview_process() -> Result<(), WvError> {
    #[cfg(target_os = "linux")]
    {
        let _ = keld_wv::webkitgtk::prepare_gpu_safe_mode_process()?;
    }
    Ok(())
}

/// Window title when config/args do not supply one.
pub const DEFAULT_HELLO_TITLE: &str = "Keld";

/// Renderer path when `keld.config.ts` has no `renderer` field.
pub const DEFAULT_RENDERER: &str = "index.html";

/// Bun entry path used by the hello template and boot compiler.
pub const DEFAULT_ENTRY: &str = "src/main.ts";

/// Opens the default Keld hello window until the user closes it.
/// On Linux, process entry must call [`prepare_webview_process`] first.
///
/// # Errors
///
/// Forwards [`keld_wv::WvError`] from the webview layer.
pub fn run_hello_window() -> Result<(), WvError> {
    run_hello_window_titled(DEFAULT_HELLO_TITLE)
}

/// Opens a hello window with `title` until the user closes it.
/// On Linux, process entry must call [`prepare_webview_process`] first.
///
/// # Errors
///
/// Forwards [`keld_wv::WvError`] from the webview layer.
pub fn run_hello_window_titled(title: &str) -> Result<(), WvError> {
    run_hello_window_html(title, HELLO_HTML)
}

/// Opens a window with `title` rendering inline `html` until the user closes it.
///
/// Used by `keld dev` with the project's renderer file contents. `keld hello`
/// and `keld-host --hello` keep the compiled `HELLO_HTML` constant.
/// On Linux, process entry must call [`prepare_webview_process`] first.
///
/// # Errors
///
/// Forwards [`keld_wv::WvError`] from the webview layer.
pub fn run_hello_window_html(title: &str, html: &str) -> Result<(), WvError> {
    wv_run_hello(title, html)
}

/// `--title <name>` or `--title=<name>` from argv (binary name included is fine).
#[must_use]
pub fn hello_title_from_args<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let arg = arg.as_ref();
        if let Some(value) = arg.strip_prefix("--title=") {
            let value = value.trim();
            if value.is_empty() {
                return None;
            }
            return Some(value.to_owned());
        }
        if arg == "--title" {
            let value = iter.next()?.as_ref().trim().to_owned();
            if value.is_empty() || value.starts_with('-') {
                return None;
            }
            return Some(value);
        }
    }
    None
}

fn quoted_config_field(source: &str, prefix: &str) -> Option<String> {
    for line in source.lines() {
        let Some(rest) = line.trim().strip_prefix(prefix) else {
            continue;
        };
        let rest = rest.trim().trim_end_matches(',').trim();
        let Some(inner) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
            continue;
        };
        if !inner.is_empty() {
            return Some(inner.to_owned());
        }
    }
    None
}

/// Reads `name: "..."` from a hello-template `keld.config.ts`.
#[must_use]
pub fn title_from_config_ts(source: &str) -> Option<String> {
    quoted_config_field(source, "name:")
}

/// Reads `renderer: "..."` from a hello-template `keld.config.ts`.
#[must_use]
pub fn renderer_from_config_ts(source: &str) -> Option<String> {
    quoted_config_field(source, "renderer:")
}

/// Reads `entry: "..."` from a hello-template `keld.config.ts`.
#[must_use]
pub fn entry_from_config_ts(source: &str) -> Option<String> {
    quoted_config_field(source, "entry:")
}

/// `name` from `project_root/keld.config.ts`, if the file exists and parses.
#[must_use]
pub fn read_config_title(project_root: &Path) -> Option<String> {
    let source = fs::read_to_string(project_root.join("keld.config.ts")).ok()?;
    title_from_config_ts(&source)
}

/// `renderer` from `project_root/keld.config.ts`, if the file exists and parses.
#[must_use]
pub fn read_config_renderer(project_root: &Path) -> Option<String> {
    let source = fs::read_to_string(project_root.join("keld.config.ts")).ok()?;
    renderer_from_config_ts(&source)
}

/// `entry` from `project_root/keld.config.ts`, if the file exists and parses.
#[must_use]
pub fn read_config_entry(project_root: &Path) -> Option<String> {
    let source = fs::read_to_string(project_root.join("keld.config.ts")).ok()?;
    entry_from_config_ts(&source)
}

/// `--title` wins; else config `name`; else [`DEFAULT_HELLO_TITLE`].
#[must_use]
pub fn resolve_hello_title(args: &[String], project_root: Option<&Path>) -> String {
    hello_title_from_args(args)
        .or_else(|| project_root.and_then(read_config_title))
        .unwrap_or_else(|| DEFAULT_HELLO_TITLE.to_owned())
}

/// Whether `keld-host` argv selects the hello diagnostic window.
///
/// This is the one hello position rule, identical on every OS: `--hello` may
/// appear anywhere after argv0, so `--hello --title <name>` and
/// `--title <name> --hello` are the same command. argv0 is the program path
/// and never selects hello. Selection is not admission: the caller must then
/// refuse every token [`host_hello_unknown_arg`] reports, which includes
/// `--hello` in `--title`'s value position (`--title --hello`).
#[must_use]
pub fn host_hello_requested<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .skip(1)
        .any(|arg| arg.as_ref() == "--hello")
}

/// First argv token `keld-host --hello` does not accept.
///
/// Allowed after argv0, in any order (see [`host_hello_requested`]):
/// `--hello`, `--title <name>`, `--title=<name>`.
/// `--title` with no following value is still consumed (title resolution
/// treats it as absent). `--title` followed by a token that starts with `-`
/// does not consume that token as a title; the token is the unknown arg.
#[must_use]
pub fn host_hello_unknown_arg<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    let _argv0 = iter.next();
    while let Some(arg) = iter.next() {
        let arg = arg.as_ref();
        if arg == "--hello" || arg.starts_with("--title=") {
            continue;
        }
        if arg == "--title" {
            let Some(value) = iter.next() else {
                continue;
            };
            let value = value.as_ref();
            if value.starts_with('-') {
                return Some(value.to_owned());
            }
            continue;
        }
        return Some(arg.to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_ENTRY, DEFAULT_HELLO_TITLE, DEFAULT_RENDERER, entry_from_config_ts,
        hello_title_from_args, host_hello_requested, host_hello_unknown_arg, read_config_entry,
        renderer_from_config_ts, resolve_hello_title, title_from_config_ts,
    };

    #[test]
    fn default_title_is_keld() {
        assert_eq!(DEFAULT_HELLO_TITLE, "Keld");
    }

    #[test]
    fn config_file_entry_reads_value_and_missing_is_none() {
        let temp = tempfile::tempdir().expect("config entry root");
        assert_eq!(read_config_entry(temp.path()), None);
        std::fs::write(
            temp.path().join("keld.config.ts"),
            "export default {\n  entry: \"src/primary.ts\",\n};\n",
        )
        .expect("write config entry");
        assert_eq!(
            read_config_entry(temp.path()),
            Some(String::from("src/primary.ts"))
        );
    }

    #[test]
    fn title_flag_space_and_equals() {
        assert_eq!(
            hello_title_from_args(["keld-host", "--hello", "--title", "Acme"]),
            Some(String::from("Acme"))
        );
        assert_eq!(
            hello_title_from_args(["keld-host", "--hello", "--title=Acme"]),
            Some(String::from("Acme"))
        );
        assert_eq!(hello_title_from_args(["keld-host", "--hello"]), None);
    }

    #[test]
    fn title_flag_rejects_empty_and_dangling() {
        assert_eq!(hello_title_from_args(["--title="]), None);
        assert_eq!(hello_title_from_args(["--title"]), None);
        assert_eq!(hello_title_from_args(["--title", "--hello"]), None);
    }

    #[test]
    fn host_hello_accepts_title_flags() {
        assert_eq!(host_hello_unknown_arg(["keld-host", "--hello"]), None);
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "--title", "Acme"]),
            None
        );
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "--title=Acme"]),
            None
        );
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "--title"]),
            None
        );
    }

    #[test]
    fn host_hello_position_is_order_free_after_argv0() {
        for args in [
            &["keld-host", "--hello"][..],
            &["keld-host", "--hello", "--title", "Acme"],
            &["keld-host", "--title", "Acme", "--hello"],
            &["keld-host", "--title=Acme", "--hello"],
        ] {
            assert!(host_hello_requested(args), "{args:?}");
            assert_eq!(host_hello_unknown_arg(args), None, "{args:?}");
        }
        assert_eq!(
            resolve_hello_title(
                &["keld-host", "--title", "Acme", "--hello"].map(String::from),
                None
            ),
            "Acme"
        );
    }

    #[test]
    fn host_hello_is_not_selected_by_argv0_or_a_title_value() {
        assert!(!host_hello_requested(["keld-host"]));
        assert!(!host_hello_requested(["--hello"]));
        assert!(!host_hello_requested(["keld-host", "--title=--hello"]));
        // A value-position `--hello` selects the hello parser, which refuses it.
        assert!(host_hello_requested(["keld-host", "--title", "--hello"]));
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--title", "--hello"]),
            Some(String::from("--hello"))
        );
    }

    #[test]
    fn host_hello_rejects_unknown_flags() {
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "--title", "--devtools"]),
            Some(String::from("--devtools"))
        );
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "--devtools"]),
            Some(String::from("--devtools"))
        );
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "--permissions"]),
            Some(String::from("--permissions"))
        );
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "extra"]),
            Some(String::from("extra"))
        );
    }

    #[test]
    fn config_name_from_hello_template_shape() {
        let source = r#"export default {
  name: "demo-app",
  entry: "src/main.ts",
  renderer: "index.html",
} as const;
"#;
        assert_eq!(title_from_config_ts(source).as_deref(), Some("demo-app"));
        assert_eq!(
            renderer_from_config_ts(source).as_deref(),
            Some("index.html")
        );
        assert_eq!(entry_from_config_ts(source).as_deref(), Some("src/main.ts"));
    }

    #[test]
    fn config_name_ignores_missing_or_empty() {
        assert_eq!(title_from_config_ts("export default {}"), None);
        assert_eq!(title_from_config_ts("name: \"\""), None);
        assert_eq!(renderer_from_config_ts("export default {}"), None);
        assert_eq!(renderer_from_config_ts("renderer: \"\""), None);
        assert_eq!(entry_from_config_ts("entry: \"\""), None);
    }

    #[test]
    fn default_renderer_is_index_html() {
        assert_eq!(DEFAULT_RENDERER, "index.html");
        assert_eq!(DEFAULT_ENTRY, "src/main.ts");
    }

    #[test]
    fn args_override_config_and_default() {
        let args = vec![String::from("--title"), String::from("from-args")];
        assert_eq!(resolve_hello_title(&args, None), "from-args");
        assert_eq!(resolve_hello_title(&[], None), "Keld");
    }

    #[test]
    fn config_file_title_when_no_args() {
        let dir = std::env::temp_dir().join(format!(
            "keld-core-hello-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("keld.config.ts"),
            "export default {\n  name: \"from-config\",\n} as const;\n",
        )
        .expect("write");
        assert_eq!(resolve_hello_title(&[], Some(&dir)), "from-config");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Static-auth configuration syntax error; no configuration code was executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthConfigError {
    detail: &'static str,
}

impl std::fmt::Display for AuthConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "KELD-CORE-035: invalid static auth configuration: {}. Use export default {{ auth: {{ \"entra-common\": {{ clientId: \"...\" }} }} }} as const with literal strings; remove expressions, computed keys and spreads.",
            self.detail
        )
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

fn root_auth_slice(source: &str) -> Result<Option<&str>, AuthConfigError> {
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

#[cfg(test)]
mod auth_config_tests {
    use super::auth_configuration_from_config_ts;

    #[test]
    fn static_auth_literal_preserves_credentials_and_redirect() {
        let config = auth_configuration_from_config_ts(
            r#"
            export default {
                name: "Legacy Title",
                /* not the key: auth: { injected: {} } */
                auth: { 'entra-common': {
                    clientId: 'operator-\u0063lient',
                    fixedRedirect: "http://localhost:4321/callback?note={auth}",
                }, },
            } as const;
        "#,
        )
        .expect("static configuration");
        let profile = config.get("entra-common").expect("profile carried");
        assert_eq!(profile.client_id.as_deref(), Some("operator-client"));
        assert_eq!(
            profile.fixed_redirect.as_deref(),
            Some("http://localhost:4321/callback?note={auth}")
        );
    }

    #[test]
    fn static_auth_carries_as_return_without_registration_validation() {
        let config = auth_configuration_from_config_ts(
            "export default { auth: { 'entra-common': { asRedirect: 'not-a-valid-registration' } } };",
        )
        .expect("AS registration semantics remain post-Allow");
        let profile = config.get("entra-common").expect("closed profile");
        assert!(profile.client_id.is_none());
        assert!(profile.fixed_redirect.is_none());
        assert_eq!(
            profile.as_redirect.as_deref(),
            Some("not-a-valid-registration")
        );
    }

    #[test]
    fn static_auth_ignores_comment_string_and_nested_mentions() {
        let config = auth_configuration_from_config_ts(
            r#"
            // auth: { "entra-common": { clientId: "not-config" } }
            export default {
                name: "auth: { pretend: {} }",
                other: { auth: { "entra-common": { clientId: "nested" } } },
            };
        "#,
        )
        .expect("no root auth");
        assert!(config.is_empty());
    }

    #[test]
    fn static_auth_rejects_unknown_profile_fields_at_owning_dto() {
        for source in [
            "export default { auth: { 'entra-common': { clientID: 'operator-client' } } };",
            "export default { auth: { 'future-provider': { clientID: 'unused' } } };",
            "export default { auth: { 'entra-common': { clientId: 'operator-client', fixedRedirct: 'http://localhost:53161/callback' } } };",
        ] {
            let error = auth_configuration_from_config_ts(source)
                .expect_err("unknown inner auth field must refuse");
            assert!(error.to_string().starts_with("KELD-CORE-035"));
            assert!(
                error
                    .to_string()
                    .contains("invalid auth configuration fields")
            );
        }
    }

    #[test]
    fn static_auth_keeps_unknown_profile_and_missing_client_for_native_owner() {
        let config = auth_configuration_from_config_ts(
            "export default { auth: { 'unknown-provider': { clientId: 'operator' }, 'entra-common': {} } };"
        ).expect("carrier does not filter provider policy");
        assert_eq!(config.len(), 2);
        assert_eq!(
            config
                .get("unknown-provider")
                .expect("unknown carried")
                .client_id
                .as_deref(),
            Some("operator")
        );
        assert!(
            config
                .get("entra-common")
                .expect("missing client preserved")
                .client_id
                .is_none()
        );
    }

    #[test]
    fn static_auth_decodes_quotes_unicode_and_surrogate_pair() {
        let config = auth_configuration_from_config_ts(
            r#"
            export default { "auth": { "entra-common": {
                "clientId": 'a\'b"c\\d\uD83D\uDE00',
            } } }
        "#,
        )
        .expect("actual string decoding");
        assert_eq!(
            config
                .get("entra-common")
                .expect("profile")
                .client_id
                .as_deref(),
            Some("a'b\"c\\d😀")
        );
    }

    #[test]
    fn static_auth_rejects_computed_spread_dynamic_and_malformed_input() {
        for source in [
            "export default { auth: buildAuth() };",
            "export default { auth: { ...settings } };",
            "export default { auth: { ['entra-common']: {} } };",
            "export default { ['auth']: {} };",
            "export default { auth: { 'entra-common': { clientId } } };",
            "export default { auth: { 'entra-common': { clientId: computed } } };",
            "export default { auth: { 'entra-common': { clientId: `template` } } };",
            "export default { auth: {} satisfies AuthConfiguration };",
            "export default { auth: { 'entra-common': { clientId: 'broken } } };",
            "export default { auth: { 'entra-common': { clientId: '\\u12XX' } } };",
            "export default { auth: {}",
            "export default { auth: {} ] };",
        ] {
            let error = auth_configuration_from_config_ts(source)
                .expect_err("unsupported input must refuse");
            let rendered = error.to_string();
            assert!(rendered.starts_with("KELD-CORE-035:"), "{rendered}");
            assert!(rendered.contains("literal strings"), "{rendered}");
        }
    }

    #[test]
    fn static_auth_rejects_duplicate_keys_before_canonicalization() {
        for source in [
            "export default { auth: {}, auth: {} };",
            "export default { auth: { 'entra-common': {}, 'entra-common': {} } };",
            "export default { auth: { 'entra-common': { clientId: 'a', clientId: 'b' } } };",
            "export default { auth: { 'entra-common': { clientId: 'a', 'clientId': 'b' } } };",
        ] {
            let error =
                auth_configuration_from_config_ts(source).expect_err("duplicate must refuse");
            assert!(error.to_string().contains("duplicate"));
        }
    }

    #[test]
    fn static_auth_legacy_templates_and_structural_limits() {
        for source in [
            "export default {}",
            "export default {\nname: \"hello\",\nentry: \"src/main.ts\",\nrenderer: \"index.html\",\n} as const;",
        ] {
            assert!(
                auth_configuration_from_config_ts(source)
                    .expect("existing template")
                    .is_empty()
            );
        }
        let oversized = format!(
            "export default {{ auth: {{ 'entra-common': {{ clientId: '{}' }} }} }};",
            "x".repeat(64 * 1024)
        );
        assert!(auth_configuration_from_config_ts(&oversized).is_err());
        let deep = format!(
            "export default {{ other: {}0{} }};",
            "[".repeat(200),
            "]".repeat(200)
        );
        assert!(
            auth_configuration_from_config_ts(&deep)
                .expect("unrelated depth remains compatible")
                .is_empty()
        );
    }
    #[test]
    fn absent_auth_preserves_legacy_nonliteral_configurations() {
        for source in [
            "export default {\n name: \"legacy\",\n other: `template ${1 + 2}`,\n};",
            "import settings from './settings';\nconst x = 3;\nexport default {\n name: \"legacy\",\n other: settings,\n};",
            "const x = 6 / 2;\nexport default {\n name: \"legacy\",\n other: /auth: { pretend: {} }/,\n};",
            "export default {\n name: \"legacy\",\n other: 1, other: 2,\n};",
            "if (true) /export default { auth: {} }/.test('x');\nexport default {\n name: \"legacy\",\n};",
            "function helper() { return { auth: {} }; }\nexport default {\n name: \"legacy\",\n};",
        ] {
            assert_eq!(
                super::title_from_config_ts(source).as_deref(),
                Some("legacy")
            );
            assert!(
                auth_configuration_from_config_ts(source)
                    .expect("no auth cannot restrict old config")
                    .is_empty()
            );
        }
        let large = format!("/*{}*/ export default {{}}", "x".repeat(64 * 1024));
        assert!(
            auth_configuration_from_config_ts(&large)
                .expect("unrelated size")
                .is_empty()
        );
    }

    #[test]
    fn auth_location_skips_templates_regex_division_and_unrelated_duplicate_keys() {
        let source = r"
            import settings from './settings';
            const ratio = {} / 2;
            const pattern = /export default { auth: { fake: {} } }/;
            export default {
                name: `auth: ${ { nested: `auth: ${1}` } }`,
                regex: /auth: { \/ }/,
                ratio: 8 / 2,
                other: 1, other: 2,
                nested: { auth: { fake: {} } },
                auth: { 'entra-common': { clientId: 'real-client' } },
            } as const;
        ";
        let config = auth_configuration_from_config_ts(source).expect("strict slice only");
        assert_eq!(config.len(), 1);
        assert_eq!(
            config
                .get("entra-common")
                .expect("actual root")
                .client_id
                .as_deref(),
            Some("real-client")
        );
    }

    #[test]
    fn computed_auth_prefix_does_not_create_auth_property() {
        let source = "export default {\n name: \"legacy\",\n ['auth' + 'orship']: 'value',\n};";
        assert_eq!(
            super::title_from_config_ts(source).as_deref(),
            Some("legacy")
        );
        assert!(
            auth_configuration_from_config_ts(source)
                .expect("not literal root auth")
                .is_empty()
        );
        let actual = "export default { ['auth' /* key end */]: {} };";
        assert!(auth_configuration_from_config_ts(actual).is_err());
    }

    #[test]
    fn named_auth_accessors_are_unsupported_not_silently_absent() {
        for source in [
            "export default { get auth() { return {}; } };",
            "export default { set auth(value) {} };",
            "export default { get 'auth'() { return {}; } };",
            "export default { get ['auth']() { return {}; } };",
            "export default { auth: {}, get auth() { return {}; } };",
            "export default { get auth() { return {}; }, auth: {} };",
        ] {
            let error = auth_configuration_from_config_ts(source)
                .expect_err("actual root accessor must refuse");
            assert!(error.to_string().starts_with("KELD-CORE-035:"));
        }
        for source in [
            "export default { get: { auth: {} } };",
            "export default { get authorship() { return { auth: {} }; } };",
            "export default { get ['auth' + 'orship']() { return {}; } };",
        ] {
            assert!(
                auth_configuration_from_config_ts(source)
                    .expect("unrelated getter/key")
                    .is_empty()
            );
        }
    }

    #[test]
    fn config_line_comments_end_at_cr_or_unicode_line_terminator() {
        for end in ["\r", "\r\n", "\u{2028}", "\u{2029}"] {
            let source = format!(
                "export default {{ // root comment{end}auth: {{ // auth comment{end}'entra-common': {{ clientId: 'visible' }} }} }};"
            );
            let config =
                auth_configuration_from_config_ts(&source).expect("line comment cannot hide auth");
            assert_eq!(
                config
                    .get("entra-common")
                    .expect("profile")
                    .client_id
                    .as_deref(),
                Some("visible")
            );
        }
    }

    #[test]
    fn trailing_statements_do_not_restrict_static_auth_extraction() {
        let source = "export default { auth: { 'entra-common': { clientId: 'kept' } } }; doSomething(); throw new Error('not executed');";
        let config = auth_configuration_from_config_ts(source)
            .expect("same literal extraction with opaque tail");
        assert_eq!(
            config
                .get("entra-common")
                .expect("profile")
                .client_id
                .as_deref(),
            Some("kept")
        );
    }
}
