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
    /// Every file an ungated `mod <name>;` may resolve to, over-approximated
    /// (see [`declare`]). A file a production declaration reaches is never
    /// excluded.
    pub declared: BTreeSet<PathBuf>,
    /// 0-based lines of ungated `mod <name>;` declarations this scanner cannot
    /// resolve (an unreadable `#[path`, nesting inside a `#[path]` inline
    /// module, or a path above the repository root): reported as hits.
    pub unresolved: BTreeSet<usize>,
    /// `(0-based line, file)` for each plain `include!("...")` outside every
    /// test-only item, resolved relative to the including file.
    pub includes: BTreeSet<(usize, PathBuf)>,
    /// 0-based lines of production `include!` calls this scanner cannot read.
    pub unreadable_includes: BTreeSet<usize>,
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

/// A `#[path ...]` attribute: a plain `#[path = "value"]`, or any other form,
/// which the caller must treat as unresolvable.
enum PathAttribute {
    Plain(String),
    Unreadable,
}

fn path_attribute(attribute: &str) -> Option<PathAttribute> {
    let compact: String = attribute.chars().filter(|c| !c.is_whitespace()).collect();
    if !compact.starts_with("#[path") {
        return None;
    }
    let value = compact
        .strip_prefix("#[path=\"")
        .and_then(|rest| rest.strip_suffix("\"]"))
        .filter(|value| !value.contains(['"', '\\']));
    Some(value.map_or(PathAttribute::Unreadable, |value| {
        PathAttribute::Plain(value.to_owned())
    }))
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
    normalize_within(path).unwrap_or_else(|| path.to_path_buf())
}

/// [`normalize`], or `None` when a `..` climbs above the first component.
fn normalize_within(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Some(out)
}

/// Files a `mod <name>;` in `declaring`, nested in the inline modules `scope`,
/// resolves to under Rust's module rules (the Reference, "Modules"). The module
/// directory is the declaring file's directory for a mod-rs file (`lib.rs`,
/// `main.rs`, `mod.rs`) and `<dir>/<stem>` otherwise; each enclosing inline
/// module adds its name. Without `#[path]` the file is `<name>.rs` or
/// `<name>/mod.rs` there. A `#[path]` outside inline modules is relative to the
/// declaring file's directory; inside them, to that nested module directory.
pub fn resolve_module_file(
    declaring: &Path,
    name: &str,
    path: Option<&str>,
    scope: &[String],
) -> Vec<PathBuf> {
    let directory = declaring.parent().unwrap_or_else(|| Path::new(""));
    let file_name = declaring.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let module_directory = if matches!(file_name, "lib.rs" | "main.rs" | "mod.rs") {
        directory.to_path_buf()
    } else {
        let stem = declaring.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        directory.join(stem)
    };
    let nested = scope
        .iter()
        .fold(module_directory, |dir, inline| dir.join(inline));
    if let Some(path) = path {
        let base = if scope.is_empty() {
            directory.to_path_buf()
        } else {
            nested
        };
        return vec![normalize(&base.join(path))];
    }
    vec![
        normalize(&nested.join(format!("{name}.rs"))),
        normalize(&nested.join(name).join("mod.rs")),
    ]
}

/// The inline modules enclosing one line, and whether any carries `#[path]`.
#[derive(Debug, Default, Clone)]
struct Scope {
    names: Vec<String>,
    pathed: bool,
}

fn preceded_by_path_attribute(lines: &[&str], code: &[String], index: usize) -> bool {
    let mut at = index;
    while at > 0 {
        at -= 1;
        if let Some(attribute) = single_line_attribute(lines[at], &code[at]) {
            if path_attribute(attribute).is_some() {
                return true;
            }
            continue;
        }
        let trimmed = lines[at].trim();
        if !(trimmed.is_empty() || trimmed.starts_with("//")) {
            return false;
        }
    }
    false
}

/// Per line, the inline `mod <name> {` blocks enclosing it (brace-matched on
/// the code view, so braces in strings and comments never count).
fn inline_scopes(lines: &[&str], code: &[String]) -> Vec<Scope> {
    let mut scopes = Vec::with_capacity(code.len());
    let mut stack: Vec<(String, bool, i64)> = Vec::new();
    let mut depth = 0_i64;
    for (index, line) in code.iter().enumerate() {
        scopes.push(Scope {
            names: stack.iter().map(|(name, _, _)| name.clone()).collect(),
            pathed: stack.iter().any(|(_, pathed, _)| *pathed),
        });
        let mut opening = module_declaration(line.trim())
            .filter(|(_, rest)| rest.starts_with('{'))
            .map(|(name, _)| {
                (
                    name.to_owned(),
                    preceded_by_path_attribute(lines, code, index),
                )
            });
        for c in line.chars() {
            match c {
                '{' => {
                    if let Some((name, pathed)) = opening.take() {
                        stack.push((name, pathed, depth));
                    }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                    while stack.last().is_some_and(|(_, _, open)| *open >= depth) {
                        stack.pop();
                    }
                }
                _ => {}
            }
        }
    }
    scopes
}

/// Records an ungated `mod <name>;` on 0-based `line` as production reach.
///
/// Whether a file is mod-rs is not decidable from its name alone: rustc
/// treats a `#[path]`-loaded file, a crate root other than `lib.rs`/`main.rs`
/// (`src/bin/*.rs`, fuzz targets) and an `include!`d file as mod-rs. Reach is
/// therefore recorded under both readings, which can only keep more files in
/// the scan.
fn declare(
    found: &mut Exclusions,
    declaring: &Path,
    line: usize,
    name: &str,
    path: Option<&PathAttribute>,
    scope: &Scope,
) {
    if scope.pathed || matches!(path, Some(PathAttribute::Unreadable)) {
        found.unresolved.insert(line);
        return;
    }
    let value = match path {
        Some(PathAttribute::Plain(value)) => Some(value.as_str()),
        _ => None,
    };
    let as_mod_rs = declaring.with_file_name("mod.rs");
    for reading in [declaring, as_mod_rs.as_path()] {
        for target in resolve_module_file(reading, name, value, &scope.names) {
            match normalize_within(&target) {
                Some(target) => {
                    found.declared.insert(target);
                }
                None => {
                    found.unresolved.insert(line);
                }
            }
        }
    }
}

/// `include!("...")` calls on production lines: the resolved files, and the
/// lines whose argument is not one plain string literal.
fn production_includes(declaring: &Path, lines: &[&str], code: &[String], found: &mut Exclusions) {
    let directory = declaring.parent().unwrap_or_else(|| Path::new(""));
    for (index, line) in lines.iter().enumerate() {
        if found.lines.contains(&index) || !code[index].contains("include!") {
            continue;
        }
        for (offset, _) in line.match_indices("include!") {
            let rest = line[offset + "include!".len()..].trim_start();
            let literal = rest
                .strip_prefix('(')
                .map(str::trim_start)
                .and_then(|rest| rest.strip_prefix('"'))
                .and_then(|rest| rest.split_once('"'))
                .filter(|(value, tail)| !value.contains('\\') && tail.trim_start().starts_with(')'))
                .map(|(value, _)| value);
            match literal {
                Some(value) => match normalize_within(&directory.join(value)) {
                    Some(target) => {
                        found.includes.insert((index, target));
                    }
                    None => {
                        found.unreadable_includes.insert(index);
                    }
                },
                None => {
                    found.unreadable_includes.insert(index);
                }
            }
        }
    }
}

/// Computes the criterion-6 test-only exclusions of one file.
///
/// Only an item that a test-only attribute directly gates is excluded: an
/// inline `mod <name> {` up to its brace-matched close, a single-line item, or
/// the file a `mod <name>;` resolves to. Every other form stays scanned.
pub fn exclusions(declaring: &Path, text: &str, lexed: &Lexed) -> Exclusions {
    let lines: Vec<&str> = text.split('\n').collect();
    let scopes = inline_scopes(&lines, &lexed.code);
    let mut found = Exclusions::default();
    let mut index = 0;
    while index < lines.len() {
        if single_line_attribute(lines[index], &lexed.code[index]).is_none() {
            if let Some((name, ";")) = module_declaration(lexed.code[index].trim()) {
                declare(&mut found, declaring, index, name, None, &scopes[index]);
            }
            index += 1;
            continue;
        }
        let group_start = index;
        let mut test_only = false;
        let mut path_value: Option<PathAttribute> = None;
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
        let code = lexed.code[item].trim();
        if !test_only {
            if let Some((name, ";")) = module_declaration(code) {
                declare(
                    &mut found,
                    declaring,
                    item,
                    name,
                    path_value.as_ref(),
                    &scopes[item],
                );
            }
            index = item;
            continue;
        }
        if let Some((name, rest)) = module_declaration(code) {
            if rest.starts_with('{') {
                if let Some(end) = inline_module_end(&lexed.code, item) {
                    found.lines.extend(group_start..=end);
                    index = end + 1;
                    continue;
                }
            } else if rest == ";" {
                // A `#[path` this scanner cannot read, or nesting inside a
                // `#[path]` inline module, resolves elsewhere: stay scanned.
                let scope = &scopes[item];
                let readable = !matches!(path_value, Some(PathAttribute::Unreadable));
                if readable && !scope.pathed {
                    let path = match &path_value {
                        Some(PathAttribute::Plain(value)) => Some(value.as_str()),
                        _ => None,
                    };
                    found
                        .files
                        .extend(resolve_module_file(declaring, name, path, &scope.names));
                    found.lines.extend(group_start..=item);
                    index = item + 1;
                    continue;
                }
                // Not excluded, so its target stays scanned; a test-gated
                // declaration is never production reach.
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
    production_includes(declaring, &lines, &lexed.code, &mut found);
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
    // A file that production code declares or `include!`s is production,
    // wherever it lives (even under `tests/`); load such files until none is
    // missing.
    loop {
        let (_, production) = production_view(&sources);
        let missing: Vec<PathBuf> = production
            .reached
            .into_iter()
            .filter(|path| !sources.contains_key(path) && root.join(path).is_file())
            .collect();
        if missing.is_empty() {
            return sources;
        }
        for path in missing {
            let text = fs::read_to_string(root.join(&path))
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            sources.insert(path, text);
        }
    }
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

/// Which inputs are production after every file's exclusions.
struct Production {
    /// Excluded whole files.
    excluded: BTreeSet<PathBuf>,
    /// Files a production declaration or `include!` reaches.
    reached: BTreeSet<PathBuf>,
    /// Production `include!` sites whose plain target is not a scanned file:
    /// `(file, 0-based line)`.
    missing_includes: BTreeSet<(PathBuf, usize)>,
}

fn in_tests_directory(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "tests")
}

/// Lexes every input and decides which files are production. A file stays
/// excluded only when a test-gated `mod` reaches it, or it sits under
/// `tests/`, and no production declaration or production `include!` reaches
/// it. Reach counts only from production files, so this iterates to a
/// fixpoint: each round can only shrink the excluded set.
fn production_view(sources: &Sources) -> (BTreeMap<PathBuf, (Lexed, Exclusions)>, Production) {
    let mut lexed: BTreeMap<PathBuf, (Lexed, Exclusions)> = BTreeMap::new();
    for (path, text) in sources {
        let view = lex(text);
        let found = exclusions(path, text, &view);
        lexed.insert(path.clone(), (view, found));
    }
    let gated: BTreeSet<PathBuf> = lexed
        .values()
        .flat_map(|(_, found)| found.files.iter().cloned())
        .collect();
    let mut reached = BTreeSet::new();
    loop {
        let excluded: BTreeSet<PathBuf> = lexed
            .keys()
            .filter(|path| {
                (gated.contains(*path) || in_tests_directory(path)) && !reached.contains(*path)
            })
            .cloned()
            .collect();
        let next: BTreeSet<PathBuf> = lexed
            .iter()
            .filter(|(path, _)| !excluded.contains(*path))
            .flat_map(|(_, (_, found))| {
                found
                    .declared
                    .iter()
                    .cloned()
                    .chain(found.includes.iter().map(|(_, target)| target.clone()))
            })
            .collect();
        if next == reached {
            let missing_includes = lexed
                .iter()
                .filter(|(path, _)| !excluded.contains(*path))
                .flat_map(|(path, (_, found))| {
                    found
                        .includes
                        .iter()
                        .filter(|(_, target)| !lexed.contains_key(target))
                        .map(|(line, _)| (path.clone(), *line))
                })
                .collect();
            return (
                lexed,
                Production {
                    excluded,
                    reached,
                    missing_includes,
                },
            );
        }
        reached = next;
    }
}

/// The remaining (non-test) view of every scanned file, after applying every
/// file's exclusions. Excluded whole files vanish.
pub fn remaining(sources: &Sources) -> BTreeMap<PathBuf, (String, Lexed, BTreeSet<usize>)> {
    let (lexed, production) = production_view(sources);
    let excluded_files = production.excluded;
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

/// Skips Unicode whitespace, as the spec's `\s` matches.
fn skip_whitespace(text: &str, at: usize) -> usize {
    let skipped: usize = text[at..]
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(char::len_utf8)
        .sum();
    at + skipped
}

/// 1-based lines where `ChannelId\(\s*[0-9]` or
/// `const\s+[A-Z0-9_]*CHANNEL[A-Z0-9_]*\s*:\s*u16\s*=\s*[0-9]` starts.
pub fn channel_literal_lines(text: &str) -> BTreeSet<usize> {
    let bytes = text.as_bytes();
    let mut lines = BTreeSet::new();
    for (offset, _) in text.match_indices("ChannelId(") {
        let at = skip_whitespace(text, offset + "ChannelId(".len());
        if bytes.get(at).is_some_and(u8::is_ascii_digit) {
            lines.insert(line_of(text, offset));
        }
    }
    for (offset, _) in text.match_indices("const") {
        let mut at = offset + "const".len();
        let name_start = skip_whitespace(text, at);
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
        at = skip_whitespace(text, at);
        if bytes.get(at) != Some(&b':') {
            continue;
        }
        at = skip_whitespace(text, at + 1);
        if !text[at..].starts_with("u16") {
            continue;
        }
        at = skip_whitespace(text, at + "u16".len());
        if bytes.get(at) != Some(&b'=') {
            continue;
        }
        at = skip_whitespace(text, at + 1);
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
    hits.extend(unreadable_include_hits(sources));
    hits.sort();
    hits.dedup();
    hits
}

/// Production sites whose target this scanner cannot scan: an unresolvable
/// `mod <name>;`, an `include!` it cannot read, or a plain `include!` whose
/// file is absent from the input. Reported as hits, so the scan fails closed.
pub fn unreadable_include_hits(sources: &Sources) -> Vec<Hit> {
    let (lexed, production) = production_view(sources);
    let mut hits: Vec<Hit> = lexed
        .iter()
        .filter(|(path, _)| !production.excluded.contains(*path))
        .flat_map(|(path, (_, found))| {
            found
                .unreadable_includes
                .iter()
                .chain(found.unresolved.iter())
                .map(|line| Hit {
                    file: display(path),
                    line: line + 1,
                })
        })
        .collect();
    hits.extend(production.missing_includes.iter().map(|(path, line)| Hit {
        file: display(path),
        line: line + 1,
    }));
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
    lines[start + 1..end]
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .map(|line| {
            line.strip_prefix("pub const ")
                .and_then(|value| value.split_once(": &str = \""))
                .and_then(|(_, value)| value.strip_suffix("\";"))
                .unwrap_or_else(|| {
                    panic!("capability module line is not `pub const NAME: &str = \"…\";`: {line}")
                })
                .to_owned()
        })
        .collect()
}

/// The remaining code view of a file: comments and literal contents blanked,
/// test-only lines removed.
fn remaining_code(view: &Lexed, excluded: &BTreeSet<usize>) -> String {
    view.code
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if excluded.contains(&index) {
                ""
            } else {
                line.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Calls `call(` in remaining code whose `argument`-th (0-based) top-level
/// argument starts with a decimal digit: a hand-written id passed positionally.
pub fn positional_literal_hits(sources: &Sources, calls: &[(&str, usize)]) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (path, (_, view, excluded)) in remaining(sources) {
        let code = remaining_code(&view, &excluded);
        for &(call, argument) in calls {
            for (offset, _) in code.match_indices(&format!("{call}(")) {
                let preceded_by_ident = code[..offset].chars().next_back().is_some_and(is_ident);
                if preceded_by_ident {
                    continue;
                }
                let mut depth = 0_i32;
                let mut index = 0;
                let base = offset + call.len() + 1;
                let mut start = base;
                for (at, c) in code[base..].char_indices() {
                    let at = base + at;
                    match c {
                        '(' | '[' | '{' => depth += 1,
                        ')' | ']' | '}' if depth == 0 => break,
                        ')' | ']' | '}' => depth -= 1,
                        ',' if depth == 0 => {
                            if index == argument {
                                break;
                            }
                            index += 1;
                            start = at + 1;
                        }
                        _ => {}
                    }
                }
                if index == argument
                    && code[start..]
                        .trim_start()
                        .starts_with(|c: char| c.is_ascii_digit())
                {
                    hits.push(Hit {
                        file: display(&path),
                        line: line_of(&code, offset),
                    });
                }
            }
        }
    }
    hits
}

/// Criterion 1: `ChannelEntry::new(` calls in the remaining code of every
/// input except `table`.
pub fn entry_construction_hits(sources: &Sources, table: &Path) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (path, (_, view, excluded)) in remaining(sources) {
        if path == table {
            continue;
        }
        let code = remaining_code(&view, &excluded);
        for (offset, _) in code.match_indices("ChannelEntry::new(") {
            hits.push(Hit {
                file: display(&path),
                line: line_of(&code, offset),
            });
        }
    }
    hits
}

/// Criterion 1: the table's code declares exactly one `fn new(`, and it has no
/// visibility, so no other module can construct an entry.
pub fn entry_constructor_is_private(table: &str) -> bool {
    let view = lex(table);
    let declarations: Vec<&str> = view
        .code
        .iter()
        .map(|line| line.trim())
        .filter(|line| line.contains("fn new("))
        .collect();
    declarations.len() == 1 && declarations[0].starts_with("const fn new(")
}

/// Files a production declaration or `include!` reaches (they are scanned
/// even outside `crates/*/src`, for example under `tests/`).
pub fn included_files(sources: &Sources) -> BTreeSet<PathBuf> {
    production_view(sources).1.reached
}
