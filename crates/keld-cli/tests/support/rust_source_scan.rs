//! Rust source scanner for the GH-508 channel-table contracts.
//!
//! `docs/specs/gh508-kipc-channel-table.md` §3 criterion 6 owns the scan input,
//! the test-only exclusions and the two patterns implemented here; criterion 15
//! reuses the same input and exclusions for capability-name string literals.
//!
//! This is a deliberately small lexer (comments, string and char literals,
//! brackets), not a Rust parser. Any construct it does not recognise as one of
//! the three excluded test-only forms is scanned, so the scan fails closed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// One scan input: repository-relative path to source text.
pub type Sources = BTreeMap<PathBuf, String>;

/// One pattern match in the remaining (non-test) text.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hit {
    /// Repository-relative path with `/` separators.
    pub file: String,
    /// 1-based line of the match start.
    pub line: usize,
}

/// A `"..."` or raw string literal token found in code (never in a comment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrLiteral {
    /// 1-based line of the opening quote.
    pub line: usize,
    /// Literal text between the quotes, escapes left as written.
    pub value: String,
}

/// Per-line code view and string literals of one file.
#[derive(Debug)]
pub struct Lexed {
    /// Each source line with comment text and literal contents replaced by
    /// spaces, so bracket counting never sees a brace inside a string.
    pub code: Vec<String>,
    /// Every plain and raw string literal; byte and C strings are not `&str`.
    pub strings: Vec<StrLiteral>,
}

/// Lexes `text` into its code view and string literals.
pub fn lex(text: &str) -> Lexed {
    Lexer {
        chars: text.chars().collect(),
        pos: 0,
        line: 1,
        code: vec![String::new()],
        strings: Vec::new(),
    }
    .run()
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    code: Vec<String>,
    strings: Vec<StrLiteral>,
}

fn is_ident(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

impl Lexer {
    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn emit(&mut self, c: char, keep: bool) {
        if c == '\n' {
            self.line += 1;
            self.code.push(String::new());
            return;
        }
        let out = if keep { c } else { ' ' };
        if let Some(current) = self.code.last_mut() {
            current.push(out);
        }
    }

    fn advance(&mut self, keep: bool) -> Option<char> {
        let c = self.peek(0)?;
        self.pos += 1;
        self.emit(c, keep);
        Some(c)
    }

    fn prev_is_ident(&self) -> bool {
        self.pos > 0 && is_ident(self.chars[self.pos - 1])
    }

    fn run(mut self) -> Lexed {
        while let Some(c) = self.peek(0) {
            match c {
                '/' if self.peek(1) == Some('/') => self.line_comment(),
                '/' if self.peek(1) == Some('*') => self.block_comment(),
                '"' => self.quoted(true),
                '\'' => self.char_or_lifetime(),
                'r' | 'b' | 'c' if !self.prev_is_ident() && self.literal_prefix() => {}
                _ => {
                    self.advance(true);
                }
            }
        }
        Lexed {
            code: self.code,
            strings: self.strings,
        }
    }

    fn line_comment(&mut self) {
        while let Some(c) = self.peek(0) {
            if c == '\n' {
                return;
            }
            self.advance(false);
        }
    }

    fn block_comment(&mut self) {
        let mut depth = 0_usize;
        while let Some(c) = self.peek(0) {
            if c == '/' && self.peek(1) == Some('*') {
                depth += 1;
                self.advance(false);
                self.advance(false);
            } else if c == '*' && self.peek(1) == Some('/') {
                depth -= 1;
                self.advance(false);
                self.advance(false);
                if depth == 0 {
                    return;
                }
            } else {
                self.advance(false);
            }
        }
    }

    /// Handles `b"`, `b'`, `c"`, `br#"`, `cr#"` and `r#"`; returns whether a
    /// literal was consumed. `r#ident` (a raw identifier) is not a literal.
    fn literal_prefix(&mut self) -> bool {
        match (self.peek(0), self.peek(1)) {
            (Some('b' | 'c'), Some('"')) => {
                self.advance(true);
                self.quoted(false);
                true
            }
            (Some('b'), Some('\'')) => {
                self.advance(true);
                self.char_literal();
                true
            }
            (Some('b' | 'c'), Some('r')) if self.raw_starts_at(2) => {
                self.advance(true);
                self.advance(true);
                self.raw(false);
                true
            }
            (Some('r'), _) if self.raw_starts_at(1) => {
                self.advance(true);
                self.raw(true);
                true
            }
            _ => false,
        }
    }

    fn raw_starts_at(&self, offset: usize) -> bool {
        let mut k = offset;
        while self.peek(k) == Some('#') {
            k += 1;
        }
        self.peek(k) == Some('"')
    }

    fn quoted(&mut self, record: bool) {
        let line = self.line;
        self.advance(true);
        let mut value = String::new();
        while let Some(c) = self.peek(0) {
            if c == '\\' {
                self.advance(false);
                value.push(c);
                if let Some(escaped) = self.advance(false) {
                    value.push(escaped);
                }
                continue;
            }
            if c == '"' {
                self.advance(true);
                break;
            }
            self.advance(false);
            value.push(c);
        }
        if record {
            self.strings.push(StrLiteral { line, value });
        }
    }

    fn raw(&mut self, record: bool) {
        let line = self.line;
        let mut hashes = 0_usize;
        while self.peek(0) == Some('#') {
            self.advance(true);
            hashes += 1;
        }
        self.advance(true);
        let mut value = String::new();
        while let Some(c) = self.peek(0) {
            if c == '"' && (1..=hashes).all(|k| self.peek(k) == Some('#')) {
                for _ in 0..=hashes {
                    self.advance(true);
                }
                break;
            }
            self.advance(false);
            value.push(c);
        }
        if record {
            self.strings.push(StrLiteral { line, value });
        }
    }

    /// `'\…'` and `'x'` are char literals; anything else is a lifetime or label.
    fn char_or_lifetime(&mut self) {
        if self.peek(1) == Some('\\') || (self.peek(1).is_some() && self.peek(2) == Some('\'')) {
            self.char_literal();
        } else {
            self.advance(true);
        }
    }

    fn char_literal(&mut self) {
        self.advance(true);
        while let Some(c) = self.peek(0) {
            match c {
                '\\' => {
                    self.advance(false);
                    self.advance(false);
                }
                '\'' => {
                    self.advance(true);
                    return;
                }
                '\n' => return,
                _ => {
                    self.advance(false);
                }
            }
        }
    }
}

/// Lines and whole files that a test-only attribute directly gates.
#[derive(Debug, Default)]
pub struct Exclusions {
    /// 0-based lines of this file.
    pub lines: BTreeSet<usize>,
    /// Files a test-gated `mod <name>;` resolves to.
    pub files: BTreeSet<PathBuf>,
}

/// Whether a single-line attribute is test-only: `#[cfg(test)]` or
/// `#[cfg(all(...))]` with `test` as a top-level conjunct. `cfg(any(test, …))`,
/// `cfg(not(test))` and every other attribute are not.
pub fn is_test_only_cfg(attribute: &str) -> bool {
    let compact: String = attribute.chars().filter(|c| !c.is_whitespace()).collect();
    if compact == "#[cfg(test)]" {
        return true;
    }
    let Some(inner) = compact
        .strip_prefix("#[cfg(all(")
        .and_then(|rest| rest.strip_suffix("))]"))
    else {
        return false;
    };
    top_level_args(inner).iter().any(|arg| arg == "test")
}

fn top_level_args(list: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0_i32;
    let mut current = String::new();
    for c in list.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                args.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    args.push(current);
    args
}

fn balanced(code: &str) -> bool {
    let mut stack = Vec::new();
    for c in code.chars() {
        match c {
            '(' | '[' | '{' => stack.push(c),
            ')' | ']' | '}' => {
                let open = match c {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                if stack.pop() != Some(open) {
                    return false;
                }
            }
            _ => {}
        }
    }
    stack.is_empty()
}

/// An outer attribute that opens and closes on one line, as rustfmt writes it.
fn single_line_attribute<'a>(original: &'a str, code: &str) -> Option<&'a str> {
    let code = code.trim();
    (code.starts_with("#[") && code.ends_with(']') && balanced(code)).then(|| original.trim())
}

fn path_attribute(attribute: &str) -> Option<String> {
    let compact: String = attribute.chars().filter(|c| !c.is_whitespace()).collect();
    let value = compact.strip_prefix("#[path=\"")?.strip_suffix("\"]")?;
    Some(value.to_owned())
}

fn strip_visibility(item: &str) -> &str {
    let Some(rest) = item.strip_prefix("pub") else {
        return item;
    };
    let rest = if let Some(scoped) = rest.strip_prefix('(') {
        match scoped.find(')') {
            Some(end) => &scoped[end + 1..],
            None => return item,
        }
    } else {
        rest
    };
    if rest.starts_with(char::is_whitespace) {
        rest.trim_start()
    } else {
        item
    }
}

/// `mod <name>` followed by the rest of the item line, if `item` declares a module.
fn module_declaration(item: &str) -> Option<(&str, &str)> {
    let rest = strip_visibility(item).strip_prefix("mod")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let end = rest.find(|c: char| !is_ident(c)).unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    Some((&rest[..end], rest[end..].trim()))
}

fn inline_module_end(code: &[String], start: usize) -> Option<usize> {
    let mut depth = 0_i64;
    let mut opened = false;
    for (index, line) in code.iter().enumerate().skip(start) {
        for c in line.chars() {
            match c {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
            if opened && depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn single_line_item(code: &str) -> bool {
    !code.is_empty() && (code.ends_with(';') || code.ends_with('}')) && balanced(code)
}

/// Lexical `.`/`..` normalization for repository-relative paths.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Files a `mod <name>;` in `declaring` resolves to under Rust's module rules:
/// a `#[path]` value relative to the declaring file's directory, else
/// `<name>.rs` or `<name>/mod.rs` beside `lib.rs`/`main.rs`/`mod.rs`, else
/// under `<parent>/` for any other `<parent>.rs`.
pub fn resolve_module_file(declaring: &Path, name: &str, path: Option<&str>) -> Vec<PathBuf> {
    let directory = declaring.parent().unwrap_or_else(|| Path::new(""));
    if let Some(path) = path {
        return vec![normalize(&directory.join(path))];
    }
    let file_name = declaring.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let base = if matches!(file_name, "lib.rs" | "main.rs" | "mod.rs") {
        directory.to_path_buf()
    } else {
        let stem = declaring.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        directory.join(stem)
    };
    vec![
        normalize(&base.join(format!("{name}.rs"))),
        normalize(&base.join(name).join("mod.rs")),
    ]
}

/// Computes the criterion-6 test-only exclusions of one file.
///
/// Only an item that a test-only attribute directly gates is excluded: an
/// inline `mod <name> {` up to its brace-matched close, a single-line item, or
/// the file a `mod <name>;` resolves to. Every other form stays scanned.
pub fn exclusions(declaring: &Path, text: &str, lexed: &Lexed) -> Exclusions {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut found = Exclusions::default();
    let mut index = 0;
    while index < lines.len() {
        if single_line_attribute(lines[index], &lexed.code[index]).is_none() {
            index += 1;
            continue;
        }
        let group_start = index;
        let mut test_only = false;
        let mut path_value = None;
        let mut item = index;
        while item < lines.len() {
            if let Some(attribute) = single_line_attribute(lines[item], &lexed.code[item]) {
                test_only |= is_test_only_cfg(attribute);
                if let Some(value) = path_attribute(attribute) {
                    path_value = Some(value);
                }
            } else {
                let trimmed = lines[item].trim();
                if !(trimmed.is_empty() || trimmed.starts_with("//")) {
                    break;
                }
            }
            item += 1;
        }
        if item >= lines.len() {
            break;
        }
        if !test_only {
            index = item;
            continue;
        }
        let code = lexed.code[item].trim();
        if let Some((name, rest)) = module_declaration(code) {
            if rest.starts_with('{') {
                if let Some(end) = inline_module_end(&lexed.code, item) {
                    found.lines.extend(group_start..=end);
                    index = end + 1;
                    continue;
                }
            } else if rest == ";" {
                found
                    .files
                    .extend(resolve_module_file(declaring, name, path_value.as_deref()));
                found.lines.extend(group_start..=item);
                index = item + 1;
                continue;
            }
        } else if single_line_item(code) {
            found.lines.extend(group_start..=item);
            index = item + 1;
            continue;
        }
        index = item;
    }
    found
}

/// Loads the criterion-6 scan input: every `.rs` file under `crates/*/src` and
/// `crates/keld-ipc/fuzz/fuzz_targets`. `tests/` directories are never scanned.
pub fn load_scan_inputs(root: &Path) -> Sources {
    let mut sources = Sources::new();
    let mut crates: Vec<PathBuf> = fs::read_dir(root.join("crates"))
        .expect("read crates/")
        .map(|entry| entry.expect("crates/ entry").path())
        .collect();
    crates.sort();
    for crate_dir in crates {
        let src = crate_dir.join("src");
        if src.is_dir() {
            walk(root, &src, &mut sources);
        }
    }
    let fuzz = root.join("crates/keld-ipc/fuzz/fuzz_targets");
    assert!(fuzz.is_dir(), "scan input {} is missing", fuzz.display());
    walk(root, &fuzz, &mut sources);
    sources
}

fn walk(root: &Path, directory: &Path, sources: &mut Sources) {
    let mut entries: Vec<_> = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("directory entry"))
        .collect();
    entries.sort_by_key(fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type().expect("file type");
        if kind.is_dir() {
            if entry.file_name() != "tests" {
                walk(root, &path, sources);
            }
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            let relative = path
                .strip_prefix(root)
                .expect("input under root")
                .to_path_buf();
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            sources.insert(relative, text);
        }
    }
}

/// The remaining (non-test) view of every scanned file, after applying every
/// file's exclusions. Files resolved from a test-gated `mod <name>;` vanish.
pub fn remaining(sources: &Sources) -> BTreeMap<PathBuf, (String, Lexed, BTreeSet<usize>)> {
    let mut lexed: BTreeMap<PathBuf, (Lexed, Exclusions)> = BTreeMap::new();
    for (path, text) in sources {
        let view = lex(text);
        let found = exclusions(path, text, &view);
        lexed.insert(path.clone(), (view, found));
    }
    let excluded_files: BTreeSet<PathBuf> = lexed
        .values()
        .flat_map(|(_, found)| found.files.iter().cloned())
        .collect();
    let mut out = BTreeMap::new();
    for (path, (view, found)) in lexed {
        if excluded_files.contains(&path) {
            continue;
        }
        let text = sources[&path]
            .split('\n')
            .enumerate()
            .map(|(index, line)| {
                if found.lines.contains(&index) {
                    ""
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        out.insert(path, (text, view, found.lines));
    }
    out
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].bytes().filter(|&b| b == b'\n').count() + 1
}

fn skip_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() && bytes[at].is_ascii_whitespace() {
        at += 1;
    }
    at
}

/// 1-based lines where `ChannelId\(\s*[0-9]` or
/// `const\s+[A-Z0-9_]*CHANNEL[A-Z0-9_]*\s*:\s*u16\s*=\s*[0-9]` starts.
pub fn channel_literal_lines(text: &str) -> BTreeSet<usize> {
    let bytes = text.as_bytes();
    let mut lines = BTreeSet::new();
    for (offset, _) in text.match_indices("ChannelId(") {
        let at = skip_whitespace(bytes, offset + "ChannelId(".len());
        if bytes.get(at).is_some_and(u8::is_ascii_digit) {
            lines.insert(line_of(text, offset));
        }
    }
    for (offset, _) in text.match_indices("const") {
        let mut at = offset + "const".len();
        let name_start = skip_whitespace(bytes, at);
        if name_start == at {
            continue;
        }
        at = name_start;
        while at < bytes.len()
            && (bytes[at].is_ascii_uppercase() || bytes[at].is_ascii_digit() || bytes[at] == b'_')
        {
            at += 1;
        }
        if !text[name_start..at].contains("CHANNEL") {
            continue;
        }
        at = skip_whitespace(bytes, at);
        if bytes.get(at) != Some(&b':') {
            continue;
        }
        at = skip_whitespace(bytes, at + 1);
        if !text[at..].starts_with("u16") {
            continue;
        }
        at = skip_whitespace(bytes, at + "u16".len());
        if bytes.get(at) != Some(&b'=') {
            continue;
        }
        at = skip_whitespace(bytes, at + 1);
        if bytes.get(at).is_some_and(u8::is_ascii_digit) {
            lines.insert(line_of(text, offset));
        }
    }
    lines
}

fn display(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Criterion 6: hand-written channel ids in the remaining text of every input
/// except `skip` (the table file itself).
pub fn channel_literal_hits(sources: &Sources, skip: &Path) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (path, (text, _, _)) in remaining(sources) {
        if path == skip {
            continue;
        }
        for line in channel_literal_lines(&text) {
            hits.push(Hit {
                file: display(&path),
                line,
            });
        }
    }
    hits
}

/// Criterion 15: string literals equal to a `names` entry in the remaining
/// code of every input outside `owner` (the `keld-guard` source tree).
pub fn capability_literal_hits(
    sources: &Sources,
    owner: &Path,
    names: &BTreeSet<String>,
) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (path, (_, view, excluded)) in remaining(sources) {
        if path.starts_with(owner) {
            continue;
        }
        for literal in &view.strings {
            if names.contains(&literal.value) && !excluded.contains(&(literal.line - 1)) {
                hits.push(Hit {
                    file: display(&path),
                    line: literal.line,
                });
            }
        }
    }
    hits
}

/// Every `pub const NAME: &str = "value";` inside `pub mod capability {` of
/// the `keld-guard` crate root: the exported capability names (criterion 15).
pub fn exported_capability_names(guard_lib: &str) -> BTreeSet<String> {
    let view = lex(guard_lib);
    let lines: Vec<&str> = guard_lib.split('\n').collect();
    let start = lines
        .iter()
        .position(|line| line.trim() == "pub mod capability {")
        .expect("keld-guard exports `pub mod capability {`");
    let end = inline_module_end(&view.code, start).expect("capability module closes");
    lines[start..=end]
        .iter()
        .filter_map(|line| {
            let value = line.trim().strip_prefix("pub const ")?;
            let (_, value) = value.split_once(": &str = \"")?;
            Some(value.strip_suffix("\";")?.to_owned())
        })
        .collect()
}
