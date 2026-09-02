//! Semantic validator and deterministic renderer for Keld product status.

#[path = "repo_path_contract.rs"]
mod repo_path_contract;

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use repo_path_contract::escapes_public_repo;

const LEDGER_REL: &str = "docs/engineering/product-status.tsv";
const OUTPUT_REL: &str = "docs/engineering/product-status.md";
const SCHEMA_LINE: &str = "# keld.product-status/v1";
const COLUMNS_LINE: &str = "# id\towner\tplatform\tcurrent\tcurrent_note\tcurrent_evidence\ttarget\ttarget_source\tissues\tlast_verified_sha";
const FIELD_COUNT: usize = 10;
const JUST_STATUS_TEST_COMMANDS: &[&str] = &[
    "mkdir -p target/product-status",
    "rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test",
    "target/product-status/product-status-test",
];
const JUST_STATUS_CHECK_COMMANDS: &[&str] = &[
    "mkdir -p target/product-status",
    "rustc --edition=2024 -D warnings tools/product_status.rs -o target/product-status/product-status",
    "target/product-status/product-status check .",
];
const REQUIRED_NON_CRATE_IDS: &[&str] = &[
    "package.keld-electron",
    "package.keld-api",
    "package.keld-web",
    "package.keld-cli",
    "package.keld-schema",
    "package.create-keld",
    "phase.0",
    "phase.1",
    "phase.2",
    "phase.3",
    "phase.4",
    "surface.windows-app-link",
    "surface.no-flag-macos",
    "surface.no-flag-windows",
    "surface.no-flag-linux",
];

const REQUIRED_CONSUMERS: &[&str] = &[
    "README.md",
    "docs/architecture/01-overview.md",
    "docs/onboarding/01-project-summary.md",
    "docs/onboarding/02-architecture-guide.md",
    "docs/onboarding/03-api-and-cli-surface.md",
    "docs/engineering/linear-roadmap-mapping.md",
];

const ROADMAP_AUTHORITY_CONSUMERS: &[&str] = &[
    "docs/engineering/linear-roadmap-mapping.md",
    "docs/onboarding/01-project-summary.md",
    "docs/onboarding/02-architecture-guide.md",
    "docs/onboarding/03-api-and-cli-surface.md",
    "docs/onboarding/05-development-guide.md",
    "docs/onboarding/06-documentation-map.md",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Platform {
    All,
    Macos,
    Windows,
    Linux,
}

impl Platform {
    fn parse(value: &str, line: usize) -> Result<Self, String> {
        match value {
            "all" => Ok(Self::All),
            "macos" => Ok(Self::Macos),
            "windows" => Ok(Self::Windows),
            "linux" => Ok(Self::Linux),
            _ => Err(invalid(
                line,
                format!("unknown platform `{value}`; use all, macos, windows, or linux"),
            )),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Macos => "macOS",
            Self::Windows => "Windows",
            Self::Linux => "Linux",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Current {
    Live,
    Partial,
    Skeleton,
    Specified,
}

impl Current {
    fn parse(value: &str, line: usize) -> Result<Self, String> {
        match value {
            "live" => Ok(Self::Live),
            "partial" => Ok(Self::Partial),
            "skeleton" => Ok(Self::Skeleton),
            "specified" => Ok(Self::Specified),
            _ => Err(invalid(
                line,
                format!(
                    "unsupported current classification `{value}`; completion claims are not allowed"
                ),
            )),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Live => "Live",
            Self::Partial => "Partial",
            Self::Skeleton => "Skeleton",
            Self::Specified => "Specified only",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Specified,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecordKind {
    Crate,
    Package,
    Phase,
    Surface,
}

fn expected_package_owner(id: &str) -> Option<&'static str> {
    match id {
        "package.keld-electron" => Some("package:@keld/electron"),
        "package.keld-api" => Some("package:@keld/api"),
        "package.keld-web" => Some("package:@keld/web"),
        "package.keld-cli" => Some("package:@keld/cli"),
        "package.keld-schema" => Some("package:@keld/schema"),
        "package.create-keld" => Some("package:create-keld"),
        _ => None,
    }
}

impl RecordKind {
    fn parse(id: &str, owner: &str, line: usize) -> Result<Self, String> {
        let (kind, suffix) = id
            .split_once('.')
            .ok_or_else(|| invalid(line, "id must use a closed `kind.name` namespace"))?;
        if suffix.is_empty() {
            return Err(invalid(line, "id namespace has an empty name"));
        }
        let parsed = match kind {
            "crate" => Self::Crate,
            "package" => Self::Package,
            "phase" => Self::Phase,
            "surface" => Self::Surface,
            _ => {
                return Err(invalid(
                    line,
                    format!("unknown id namespace `{kind}`; use crate, package, phase, or surface"),
                ));
            }
        };
        let expected_owner_prefix = format!("{kind}:");
        if !owner.starts_with(&expected_owner_prefix) || owner == expected_owner_prefix {
            return Err(invalid(
                line,
                format!("owner `{owner}` does not match id namespace `{kind}`"),
            ));
        }
        if matches!(parsed, Self::Package) {
            if expected_package_owner(id) != Some(owner) {
                return Err(invalid(
                    line,
                    format!("owner `{owner}` is not the canonical package owner for id `{id}`"),
                ));
            }
        } else if owner != format!("{kind}:{suffix}") {
            return Err(invalid(
                line,
                format!("owner `{owner}` must equal `{kind}:{suffix}` for id `{id}`"),
            ));
        }
        Ok(parsed)
    }

    const fn heading(self) -> &'static str {
        match self {
            Self::Crate => "Crates",
            Self::Package => "Packages",
            Self::Phase => "Phases",
            Self::Surface => "Platform surfaces",
        }
    }
}

impl Target {
    fn parse(value: &str, line: usize) -> Result<Self, String> {
        match value {
            "specified" => Ok(Self::Specified),
            "none" => Ok(Self::None),
            _ => Err(invalid(
                line,
                format!("unknown target classification `{value}`; use specified or none"),
            )),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Specified => "Specified",
            Self::None => "None",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EvidenceKind {
    Code,
    Test,
    Ci,
}

impl EvidenceKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Test => "test",
            Self::Ci => "CI",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Evidence {
    kind: EvidenceKind,
    source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Record {
    line: usize,
    id: String,
    owner: String,
    kind: RecordKind,
    platform: Platform,
    current: Current,
    current_note: String,
    evidence: Vec<Evidence>,
    target: Target,
    target_source: Option<String>,
    issues: Vec<String>,
    last_verified_sha: String,
}

fn invalid(line: usize, detail: impl AsRef<str>) -> String {
    format!(
        "KELD-DOCS005: invalid product-status ledger at line {line}: {}. \
         Fix `{LEDGER_REL}`, run `just product-status`, then rerun `just product-status-check`.",
        detail.as_ref()
    )
}

fn valid_key(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-:@/".contains(&byte)
        })
}

fn validate_sha(value: &str, line: usize) -> Result<(), String> {
    if value.len() != 40
        || value.bytes().all(|byte| byte == b'0')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            line,
            "last_verified_sha must be a nonzero 40-character lowercase hexadecimal commit",
        ));
    }
    Ok(())
}

fn parse_issues(value: &str, line: usize) -> Result<Vec<String>, String> {
    if value == "-" {
        return Ok(Vec::new());
    }
    let mut issues = Vec::new();
    let mut numbers = BTreeSet::new();
    let mut previous = None;
    for issue in value.split(',') {
        let Some(number) = issue.strip_prefix("KEL-") else {
            return Err(invalid(line, format!("malformed issue `{issue}`")));
        };
        let number = number
            .parse::<u32>()
            .map_err(|_| invalid(line, format!("malformed issue `{issue}`")))?;
        if number == 0 {
            return Err(invalid(line, format!("malformed issue `{issue}`")));
        }
        if !numbers.insert(number) {
            return Err(invalid(line, format!("duplicate issue `{issue}`")));
        }
        if previous.is_some_and(|prior| number <= prior) {
            return Err(invalid(line, "issues must be sorted by numeric KEL id"));
        }
        previous = Some(number);
        issues.push(issue.to_owned());
    }
    Ok(issues)
}

fn parse_evidence(value: &str, line: usize) -> Result<Vec<Evidence>, String> {
    if value == "-" {
        return Ok(Vec::new());
    }
    if value.is_empty() {
        return Err(invalid(
            line,
            "current_evidence is empty; use `-` only for specified-only rows",
        ));
    }
    let mut evidence = Vec::new();
    for token in value.split(';') {
        let Some((kind, source)) = token.split_once(':') else {
            return Err(invalid(line, format!("malformed evidence token `{token}`")));
        };
        if source.is_empty() {
            return Err(invalid(
                line,
                format!("evidence token `{token}` has an empty source"),
            ));
        }
        let kind = match kind {
            "code" => EvidenceKind::Code,
            "test" => EvidenceKind::Test,
            "ci" => EvidenceKind::Ci,
            _ => return Err(invalid(line, format!("unknown evidence kind `{kind}`"))),
        };
        evidence.push(Evidence {
            kind,
            source: source.to_owned(),
        });
    }
    Ok(evidence)
}

fn parse_record(line_number: usize, line: &str) -> Result<Record, String> {
    let fields = line.split('\t').collect::<Vec<_>>();
    if fields.len() != FIELD_COUNT {
        return Err(invalid(
            line_number,
            format!(
                "expected {FIELD_COUNT} tab-separated fields, found {}",
                fields.len()
            ),
        ));
    }
    let [
        id,
        owner,
        platform,
        current,
        current_note,
        evidence,
        target,
        target_source,
        issues,
        sha,
    ] = fields.as_slice()
    else {
        return Err(invalid(line_number, "internal field-shape mismatch"));
    };
    if !valid_key(id) {
        return Err(invalid(
            line_number,
            "id is missing or contains unsupported characters",
        ));
    }
    if !valid_key(owner) || !owner.contains(':') {
        return Err(invalid(
            line_number,
            "owner is missing or is not a logical `kind:key`",
        ));
    }
    let kind = RecordKind::parse(id, owner, line_number)?;
    if current_note.is_empty() || current_note.contains('|') {
        return Err(invalid(
            line_number,
            "current_note is empty or contains a Markdown table delimiter",
        ));
    }
    validate_sha(sha, line_number)?;
    let target = Target::parse(target, line_number)?;
    let target_source = match (target, *target_source) {
        (Target::Specified, "-") => {
            return Err(invalid(
                line_number,
                "specified target is missing target_source",
            ));
        }
        (Target::Specified, source) => Some(source.to_owned()),
        (Target::None, "-") => None,
        (Target::None, _) => {
            return Err(invalid(
                line_number,
                "target=none must use `-` for target_source",
            ));
        }
    };
    Ok(Record {
        line: line_number,
        id: (*id).to_owned(),
        owner: (*owner).to_owned(),
        kind,
        platform: Platform::parse(platform, line_number)?,
        current: Current::parse(current, line_number)?,
        current_note: (*current_note).to_owned(),
        evidence: parse_evidence(evidence, line_number)?,
        target,
        target_source,
        issues: parse_issues(issues, line_number)?,
        last_verified_sha: (*sha).to_owned(),
    })
}

fn parse_ledger(contents: &str) -> Result<Vec<Record>, String> {
    let normalized = contents.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = normalized.lines();
    if lines.next() != Some(SCHEMA_LINE) {
        return Err(invalid(1, format!("first line must be `{SCHEMA_LINE}`")));
    }
    if lines.next() != Some(COLUMNS_LINE) {
        return Err(invalid(2, "column declaration is missing or reordered"));
    }
    let mut records = Vec::new();
    let mut ids = BTreeMap::new();
    let mut owners = BTreeMap::new();
    for (offset, line) in lines.enumerate() {
        let line_number = offset + 3;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let record = parse_record(line_number, line)?;
        let duplicate_id = ids.get(&record.id).copied();
        let duplicate_owner = owners.get(&record.owner).copied();
        if duplicate_id.is_some() || duplicate_owner.is_some() {
            let mut details = Vec::new();
            if let Some(first) = duplicate_id {
                details.push(format!(
                    "duplicate id `{}`; first declared at line {first}",
                    record.id
                ));
            }
            if let Some(first) = duplicate_owner {
                details.push(format!(
                    "duplicate owner `{}`; first declared at line {first}",
                    record.owner
                ));
            }
            return Err(invalid(line_number, details.join("; ")));
        }
        ids.insert(record.id.clone(), line_number);
        owners.insert(record.owner.clone(), line_number);
        records.push(record);
    }
    if records.is_empty() {
        return Err(invalid(3, "ledger contains no records"));
    }
    for required in REQUIRED_NON_CRATE_IDS {
        if !ids.contains_key(*required) {
            return Err(invalid(
                3,
                format!("required stable status id `{required}` is missing"),
            ));
        }
    }
    Ok(records)
}

fn canonical_root(root: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(root).map_err(|error| {
        format!(
            "KELD-DOCS005: failed to resolve workspace root `{}`: {error}. Pass an existing checkout root.",
            root.display()
        )
    })
}

fn resolve_public_file(root: &Path, relative: &str, line: usize) -> Result<PathBuf, String> {
    let candidate = Path::new(relative);
    if escapes_public_repo(candidate) {
        return Err(invalid(
            line,
            format!("source path `{relative}` is outside the public tracked contract"),
        ));
    }
    let joined = root.join(candidate);
    let canonical = match fs::canonicalize(&joined) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(invalid(
                line,
                format!("source path `{relative}` does not exist"),
            ));
        }
        Err(error) => {
            return Err(invalid(
                line,
                format!("cannot resolve source path `{relative}`: {error}"),
            ));
        }
    };
    let stripped = canonical.strip_prefix(root).map_err(|_| {
        invalid(
            line,
            format!("source path `{relative}` resolves outside the checkout"),
        )
    })?;
    if escapes_public_repo(stripped) || !canonical.is_file() {
        return Err(invalid(
            line,
            format!("source path `{relative}` is not a public regular file"),
        ));
    }
    Ok(canonical)
}

fn run_git(root: &Path, args: &[&str], line: usize) -> Result<std::process::Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| invalid(line, format!("failed to run git: {error}")))
}

fn validate_git_commit(root: &Path, sha: &str, line: usize) -> Result<(), String> {
    let commit = format!("{sha}^{{commit}}");
    let object = run_git(root, &["cat-file", "-e", &commit], line)?;
    if !object.status.success() {
        return Err(invalid(
            line,
            format!("last_verified_sha `{sha}` is not a commit in this checkout"),
        ));
    }
    let ancestor = run_git(root, &["merge-base", "--is-ancestor", sha, "HEAD"], line)?;
    if !ancestor.status.success() {
        return Err(invalid(
            line,
            format!("last_verified_sha `{sha}` is not an ancestor of HEAD"),
        ));
    }
    Ok(())
}

fn validate_git_file_at(root: &Path, sha: &str, relative: &str, line: usize) -> Result<(), String> {
    let tree = run_git(root, &["ls-tree", "--full-tree", sha, "--", relative], line)?;
    if !tree.status.success() {
        return Err(invalid(
            line,
            format!("git cannot inspect `{relative}` at `{sha}`"),
        ));
    }
    let output = String::from_utf8(tree.stdout)
        .map_err(|error| invalid(line, format!("git tree output is not UTF-8: {error}")))?;
    let mode = output.split_whitespace().next().unwrap_or_default();
    if !matches!(mode, "100644" | "100755") {
        return Err(invalid(
            line,
            format!(
                "source path `{relative}` is not a tracked regular file at `{sha}` (mode `{mode}`)"
            ),
        ));
    }
    Ok(())
}

fn read_git_file_at(root: &Path, sha: &str, relative: &str, line: usize) -> Result<String, String> {
    let object = format!("{sha}:{relative}");
    let output = run_git(root, &["show", &object], line)?;
    if !output.status.success() {
        return Err(invalid(
            line,
            format!("git cannot read `{relative}` at `{sha}`"),
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| invalid(line, format!("evidence blob is not UTF-8: {error}")))
}

fn simple_rust_char_literal_len(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'\'') {
        return None;
    }
    let body_start = start + 1;
    let body_end = if bytes.get(body_start) == Some(&b'\\') {
        body_start + 2
    } else {
        let rest = std::str::from_utf8(bytes.get(body_start..)?).ok()?;
        body_start + rest.chars().next()?.len_utf8()
    };
    (bytes.get(body_end) == Some(&b'\'')).then_some(body_end + 1 - start)
}

fn without_c_style_comments(contents: &str) -> String {
    #[derive(Clone, Copy)]
    enum Scan {
        Code,
        LineComment,
        BlockComment(u32),
        Quoted(u8),
        Raw(usize),
    }

    let bytes = contents.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let mut scan = Scan::Code;
    while index < bytes.len() {
        match scan {
            Scan::Code => {
                let pair = bytes.get(index..index.saturating_add(2));
                if pair == Some(b"//") {
                    scan = Scan::LineComment;
                    index += 2;
                } else if pair == Some(b"/*") {
                    scan = Scan::BlockComment(1);
                    index += 2;
                } else if bytes[index] == b'r' {
                    let mut cursor = index + 1;
                    while bytes.get(cursor) == Some(&b'#') {
                        cursor += 1;
                    }
                    if bytes.get(cursor) == Some(&b'"') {
                        scan = Scan::Raw(cursor - index - 1);
                        index = cursor + 1;
                    } else {
                        output.push(bytes[index]);
                        index += 1;
                    }
                } else if let Some(length) = simple_rust_char_literal_len(bytes, index) {
                    output.extend_from_slice(&bytes[index..index + length]);
                    index += length;
                } else if matches!(bytes[index], b'"' | b'`') {
                    scan = Scan::Quoted(bytes[index]);
                    index += 1;
                } else {
                    output.push(bytes[index]);
                    index += 1;
                }
            }
            Scan::LineComment => {
                if bytes[index] == b'\n' {
                    output.push(b'\n');
                    scan = Scan::Code;
                }
                index += 1;
            }
            Scan::BlockComment(depth) => {
                let pair = bytes.get(index..index.saturating_add(2));
                if pair == Some(b"/*") {
                    scan = Scan::BlockComment(depth.saturating_add(1));
                    index += 2;
                } else if pair == Some(b"*/") {
                    scan = if depth == 1 {
                        Scan::Code
                    } else {
                        Scan::BlockComment(depth - 1)
                    };
                    index += 2;
                } else {
                    if bytes[index] == b'\n' {
                        output.push(b'\n');
                    }
                    index += 1;
                }
            }
            Scan::Quoted(delimiter) => {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else {
                    if bytes[index] == b'\n' {
                        output.push(b'\n');
                    }
                    if bytes[index] == delimiter {
                        scan = Scan::Code;
                    }
                    index += 1;
                }
            }
            Scan::Raw(hashes) => {
                if bytes[index] == b'"'
                    && (0..hashes).all(|offset| bytes.get(index + 1 + offset) == Some(&b'#'))
                {
                    scan = Scan::Code;
                    index += 1 + hashes;
                } else {
                    if bytes[index] == b'\n' {
                        output.push(b'\n');
                    }
                    index += 1;
                }
            }
        }
    }
    String::from_utf8(output).unwrap_or_default()
}

fn has_declared_test(path: &str, contents: &str) -> bool {
    let contents = without_c_style_comments(contents);
    if path.ends_with(".rs") {
        return contents.lines().any(|line| {
            line.trim_start()
                .strip_prefix("#[test]")
                .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(char::is_whitespace))
        });
    }
    contents.lines().any(|line| {
        let line = line.trim_start();
        [
            "test(",
            "it(",
            "test.only(",
            "it.only(",
            "test.each(",
            "it.each(",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix))
    })
}

fn validate_evidence_kind(
    root: &Path,
    record: &Record,
    evidence: &Evidence,
    git_checkout: bool,
) -> Result<(), String> {
    let path = evidence.source.replace('\\', "/");
    let owned = match record.kind {
        RecordKind::Crate => record
            .id
            .strip_prefix("crate.")
            .is_some_and(|name| path.starts_with(&format!("crates/{name}/"))),
        RecordKind::Package => record
            .owner
            .strip_prefix("package:")
            .is_some_and(|name| path.starts_with(&format!("packages/{name}/"))),
        RecordKind::Phase | RecordKind::Surface => true,
    };
    if !owned {
        return Err(invalid(
            record.line,
            format!(
                "{} evidence `{}` is outside owner `{}`",
                evidence.kind.label(),
                evidence.source,
                record.owner
            ),
        ));
    }
    let valid = match evidence.kind {
        EvidenceKind::Code => {
            (path.starts_with("crates/") && path.contains("/src/") && path.ends_with(".rs"))
                || (path.starts_with("packages/")
                    && path.contains("/src/")
                    && matches!(
                        Path::new(&path)
                            .extension()
                            .and_then(|value| value.to_str()),
                        Some("ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs")
                    ))
        }
        EvidenceKind::Test => {
            let named_test = path.contains("/tests/")
                || [".test.", "_test.", ".spec.", "_spec."]
                    .iter()
                    .any(|marker| path.contains(marker));
            let contents = if git_checkout {
                read_git_file_at(root, &record.last_verified_sha, &path, record.line)
            } else {
                fs::read_to_string(root.join(&path)).map_err(|error| {
                    invalid(
                        record.line,
                        format!("cannot read test evidence `{path}`: {error}"),
                    )
                })
            }?;
            let declared = has_declared_test(&path, &contents);
            let supported = matches!(
                Path::new(&path)
                    .extension()
                    .and_then(|value| value.to_str()),
                Some("rs" | "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs")
            );
            (named_test || path.contains("/src/")) && supported && declared
        }
        EvidenceKind::Ci => {
            (path.starts_with(".github/workflows/")
                && matches!(
                    Path::new(&path)
                        .extension()
                        .and_then(|value| value.to_str()),
                    Some("yml" | "yaml")
                ))
                || (path.starts_with("tools/ci_")
                    && matches!(
                        Path::new(&path)
                            .extension()
                            .and_then(|value| value.to_str()),
                        Some("rs" | "sh")
                    ))
        }
    };
    if valid {
        Ok(())
    } else {
        Err(invalid(
            record.line,
            format!(
                "{} evidence `{}` does not match its owned source class",
                evidence.kind.label(),
                evidence.source
            ),
        ))
    }
}

fn validate_records(root: &Path, records: &[Record]) -> Result<(), String> {
    let git_checkout = root.join(".git").exists();
    let mut validated_commits = BTreeSet::new();
    for record in records {
        if git_checkout && validated_commits.insert(record.last_verified_sha.as_str()) {
            validate_git_commit(root, &record.last_verified_sha, record.line)?;
        }
        let has_code = record
            .evidence
            .iter()
            .any(|item| item.kind == EvidenceKind::Code);
        let has_test = record
            .evidence
            .iter()
            .any(|item| item.kind == EvidenceKind::Test);
        match record.current {
            Current::Live if !has_code || !has_test => {
                return Err(invalid(
                    record.line,
                    "live requires distinct code and declared test source evidence",
                ));
            }
            Current::Partial | Current::Skeleton if !has_code => {
                return Err(invalid(
                    record.line,
                    "partial and skeleton rows require code evidence",
                ));
            }
            Current::Specified if !record.evidence.is_empty() => {
                return Err(invalid(
                    record.line,
                    "specified-only rows must use `-` for current_evidence",
                ));
            }
            _ => {}
        }
        let mut canonical_evidence = BTreeSet::new();
        for item in &record.evidence {
            let canonical = resolve_public_file(root, &item.source, record.line)?;
            if !canonical_evidence.insert(canonical) {
                return Err(invalid(
                    record.line,
                    format!("evidence source `{}` is listed more than once", item.source),
                ));
            }
            if git_checkout {
                validate_git_file_at(root, &record.last_verified_sha, &item.source, record.line)?;
            }
            validate_evidence_kind(root, record, item, git_checkout)?;
        }
        if let Some(source) = &record.target_source {
            if !source.starts_with("docs/architecture/") && !source.starts_with("docs/specs/") {
                return Err(invalid(
                    record.line,
                    format!(
                        "target_source `{source}` must be owned by docs/architecture or docs/specs"
                    ),
                ));
            }
            resolve_public_file(root, source, record.line)?;
            if git_checkout {
                validate_git_file_at(root, &record.last_verified_sha, source, record.line)?;
            }
        }
    }
    Ok(())
}

fn load(root: &Path) -> Result<(PathBuf, Vec<Record>), String> {
    let root = canonical_root(root)?;
    let ledger = fs::read_to_string(root.join(LEDGER_REL)).map_err(|error| {
        format!(
            "KELD-DOCS005: cannot read `{LEDGER_REL}`: {error}. Restore the ledger and rerun `just product-status-check`."
        )
    })?;
    let records = parse_ledger(&ledger)?;
    validate_records(&root, &records)?;
    Ok((root, records))
}

fn markdown_link(path: &str, label: &str) -> String {
    format!("[{label}](../../{path})")
}

fn render_evidence(record: &Record) -> String {
    if record.evidence.is_empty() {
        return "—".to_owned();
    }
    record
        .evidence
        .iter()
        .map(|item| markdown_link(&item.source, item.kind.label()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_issues(record: &Record) -> String {
    if record.issues.is_empty() {
        "—".to_owned()
    } else {
        record.issues.join(", ")
    }
}

fn render_table(output: &mut String, records: &[&Record]) {
    output
        .push_str("| Surface | Platform | Current | Evidence | Target | Issues | Verified at |\n");
    output.push_str("|---|---|---|---|---|---|---|\n");
    for record in records {
        let target = record.target_source.as_ref().map_or_else(
            || record.target.label().to_owned(),
            |source| markdown_link(source, record.target.label()),
        );
        output.push_str(&format!(
            "| `{}` | {} | **{}** — {} | {} | {} | {} | `{}` |\n",
            record.id,
            record.platform.label(),
            record.current.label(),
            record.current_note,
            render_evidence(record),
            target,
            render_issues(record),
            record.last_verified_sha
        ));
    }
}

fn render(records: &[Record]) -> String {
    let mut output = String::from(concat!(
        "# Keld product status\n\n",
        "> Generated from [`product-status.tsv`](product-status.tsv) by\n",
        "> `tools/product_status.rs`; do not edit this file by hand.\n\n",
        "This ledger owns repository **Current/Target/Evidence** facts. Architecture owns ",
        "normative design, while Linear owns live issue status, assignees, dependencies, ",
        "and claims. A row records the immutable commit at which its evidence was last ",
        "verified; it does not claim that commit is the current branch head. A `test` link ",
        "records recognized test declaration syntax in source; it is not a runner receipt or ",
        "a claim ",
        "that the test was enabled or passed. Documentation consistency is not product or ",
        "real-OS completion.\n",
    ));
    for kind in [
        RecordKind::Crate,
        RecordKind::Package,
        RecordKind::Phase,
        RecordKind::Surface,
    ] {
        let section = records
            .iter()
            .filter(|record| record.kind == kind)
            .collect::<Vec<_>>();
        if section.is_empty() {
            continue;
        }
        output.push_str("\n## ");
        output.push_str(kind.heading());
        output.push_str("\n\n");
        render_table(&mut output, &section);
    }
    output
}

fn write_output(path: &Path, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|error| {
        format!(
            "KELD-DOCS005: failed to write generated output `{}`: {error}. Check checkout permissions.",
            path.display()
        )
    })
}

fn generate(root: &Path) -> Result<(), String> {
    let (root, records) = load(root)?;
    write_output(&root.join(OUTPUT_REL), &render(&records))
}

fn check_generated(root: &Path, expected: &str) -> Result<(), String> {
    let actual = fs::read(root.join(OUTPUT_REL)).map_err(|error| {
        format!(
            "KELD-DOCS004: generated output `{OUTPUT_REL}` is missing or unreadable: {error}. Run `just product-status` and commit it."
        )
    })?;
    if actual == expected.as_bytes() {
        Ok(())
    } else {
        Err(format!(
            "KELD-DOCS004: generated output `{OUTPUT_REL}` is stale. Run `just product-status` and commit the result."
        ))
    }
}

fn read_required(root: &Path, relative: &str) -> Result<String, String> {
    fs::read_to_string(root.join(relative)).map_err(|error| {
        format!(
            "KELD-DOCS005: required status consumer `{relative}` is unreadable: {error}. Restore it and rerun the check."
        )
    })
}

fn visible_markdown(contents: &str) -> String {
    let mut without_comments = String::with_capacity(contents.len());
    let mut rest = contents;
    while let Some(start) = rest.find("<!--") {
        without_comments.push_str(&rest[..start]);
        let Some(end) = rest[start + 4..].find("-->") else {
            rest = "";
            break;
        };
        rest = &rest[start + 4 + end + 3..];
    }
    without_comments.push_str(rest);

    let mut visible = String::new();
    let mut fence: Option<(char, usize)> = None;
    for line in without_comments.lines() {
        let mut code_probe = line;
        loop {
            let trimmed = code_probe.trim_start_matches(' ');
            let Some(after_marker) = trimmed.strip_prefix('>') else {
                break;
            };
            code_probe = after_marker.strip_prefix(' ').unwrap_or(after_marker);
        }
        let indented_code = code_probe.starts_with('\t')
            || code_probe.chars().take_while(|c| *c == ' ').count() >= 4;
        if fence.is_none() && indented_code {
            continue;
        }
        let trimmed = line.trim_start();
        let marker = trimmed
            .chars()
            .next()
            .filter(|value| matches!(value, '`' | '~'))
            .map(|value| {
                (
                    value,
                    trimmed.chars().take_while(|item| *item == value).count(),
                )
            })
            .filter(|(_, width)| *width >= 3);
        if let Some((marker, width)) = marker {
            match fence {
                Some((active, opening_width))
                    if active == marker
                        && width >= opening_width
                        && trimmed[width..].trim().is_empty() =>
                {
                    fence = None;
                }
                None => fence = Some((marker, width)),
                _ => {}
            }
            continue;
        }
        if fence.is_none() {
            visible.push_str(&strip_inline_code(line));
            visible.push('\n');
        }
    }
    visible
}

fn strip_inline_code(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        output.push_str(&rest[..start]);
        let run = rest[start..]
            .chars()
            .take_while(|value| *value == '`')
            .count();
        let after_open = &rest[start + run..];
        let mut search = after_open;
        let mut consumed = 0;
        let mut closing = None;
        while let Some(candidate) = search.find('`') {
            let width = search[candidate..]
                .chars()
                .take_while(|value| *value == '`')
                .count();
            if width == run {
                closing = Some(consumed + candidate + width);
                break;
            }
            let advance = candidate + width;
            consumed += advance;
            search = &search[advance..];
        }
        if let Some(end) = closing {
            rest = &after_open[end..];
        } else {
            output.push_str(&rest[start..]);
            return output;
        }
    }
    output.push_str(rest);
    output
}

#[derive(Debug, PartialEq, Eq)]
struct VisibleLinkTarget {
    target: String,
    slash_normalized: String,
    image: bool,
}

fn visible_link_targets(contents: &str) -> Option<Vec<VisibleLinkTarget>> {
    let visible = visible_markdown(contents);
    let mut rest = visible.as_str();
    let mut targets = Vec::new();
    while let Some(start) = rest.find("](") {
        let after = &rest[start + 2..];
        let end = after.find(')')?;
        let raw_target = after[..end].trim();
        let target = if let Some(angled) = raw_target.strip_prefix('<') {
            let (target, tail) = angled.split_once('>')?;
            if tail
                .chars()
                .next()
                .is_some_and(|character| !character.is_whitespace())
            {
                return None;
            }
            target
        } else {
            raw_target.split_whitespace().next().unwrap_or_default()
        };
        let target = target.split('#').next().unwrap_or_default();
        let image = rest[..start]
            .rfind('[')
            .is_some_and(|open| open > 0 && rest.as_bytes()[open - 1] == b'!');
        targets.push(VisibleLinkTarget {
            target: target.to_owned(),
            slash_normalized: target.replace('\\', "/"),
            image,
        });
        rest = &after[end + 1..];
    }
    Some(targets)
}

fn links_to_repository_path(
    root: &Path,
    consumer: &str,
    contents: &str,
    expected_relative: &str,
) -> bool {
    let consumer_parent = Path::new(consumer)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let Ok(expected) = fs::canonicalize(root.join(expected_relative)) else {
        return false;
    };
    visible_link_targets(contents).is_some_and(|targets| {
        targets.iter().any(|link| {
            !link.image
                && !link.target.contains("://")
                && fs::canonicalize(root.join(consumer_parent).join(&link.target))
                    .is_ok_and(|candidate| candidate == expected)
        })
    })
}

fn links_to_product_status(root: &Path, consumer: &str, contents: &str) -> bool {
    links_to_repository_path(root, consumer, contents, OUTPUT_REL)
}

fn links_to_status_ledger(root: &Path, consumer: &str, contents: &str) -> bool {
    links_to_repository_path(root, consumer, contents, LEDGER_REL)
}

fn links_to_roadmap(contents: &str) -> Result<bool, ()> {
    visible_link_targets(contents)
        .map(|targets| {
            targets.iter().any(|link| {
                link.slash_normalized
                    .rsplit('/')
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case("ROADMAP.md"))
            })
        })
        .ok_or(())
}

fn skip_json_whitespace(bytes: &[u8], index: &mut usize) {
    while bytes.get(*index).is_some_and(u8::is_ascii_whitespace) {
        *index += 1;
    }
}

fn expect_json_byte(bytes: &[u8], index: &mut usize, expected: u8) -> Result<(), String> {
    skip_json_whitespace(bytes, index);
    if bytes.get(*index) != Some(&expected) {
        return Err(format!(
            "KELD-DOCS004: malformed Cargo metadata near byte {}; expected `{}`",
            *index,
            char::from(expected)
        ));
    }
    *index += 1;
    Ok(())
}

fn parse_plain_json_string(bytes: &[u8], index: &mut usize) -> Result<String, String> {
    expect_json_byte(bytes, index, b'"')?;
    let start = *index;
    while let Some(byte) = bytes.get(*index) {
        match *byte {
            b'"' => {
                let value = std::str::from_utf8(&bytes[start..*index]).map_err(|error| {
                    format!("KELD-DOCS004: Cargo metadata string is not UTF-8: {error}")
                })?;
                *index += 1;
                return Ok(value.to_owned());
            }
            b'\\' => {
                return Err(format!(
                    "KELD-DOCS004: unexpected escaped Cargo metadata key/name near byte {}",
                    *index
                ));
            }
            0..=0x1f => {
                return Err(format!(
                    "KELD-DOCS004: control byte in Cargo metadata string near byte {}",
                    *index
                ));
            }
            _ => *index += 1,
        }
    }
    Err("KELD-DOCS004: unterminated Cargo metadata string".to_owned())
}

fn skip_json_string(bytes: &[u8], index: &mut usize) -> Result<(), String> {
    expect_json_byte(bytes, index, b'"')?;
    while let Some(byte) = bytes.get(*index) {
        match *byte {
            b'"' => {
                *index += 1;
                return Ok(());
            }
            b'\\' => {
                *index += 2;
                if *index > bytes.len() {
                    return Err("KELD-DOCS004: truncated Cargo metadata escape".to_owned());
                }
            }
            0..=0x1f => {
                return Err(format!(
                    "KELD-DOCS004: control byte in Cargo metadata string near byte {}",
                    *index
                ));
            }
            _ => *index += 1,
        }
    }
    Err("KELD-DOCS004: unterminated Cargo metadata string".to_owned())
}

fn skip_json_value(bytes: &[u8], index: &mut usize) -> Result<(), String> {
    skip_json_whitespace(bytes, index);
    match bytes.get(*index).copied() {
        Some(b'"') => skip_json_string(bytes, index),
        Some(b'{') => {
            *index += 1;
            skip_json_whitespace(bytes, index);
            if bytes.get(*index) == Some(&b'}') {
                *index += 1;
                return Ok(());
            }
            loop {
                skip_json_string(bytes, index)?;
                expect_json_byte(bytes, index, b':')?;
                skip_json_value(bytes, index)?;
                skip_json_whitespace(bytes, index);
                match bytes.get(*index) {
                    Some(b',') => *index += 1,
                    Some(b'}') => {
                        *index += 1;
                        return Ok(());
                    }
                    _ => {
                        return Err(format!(
                            "KELD-DOCS004: malformed Cargo metadata object near byte {}",
                            *index
                        ));
                    }
                }
            }
        }
        Some(b'[') => {
            *index += 1;
            skip_json_whitespace(bytes, index);
            if bytes.get(*index) == Some(&b']') {
                *index += 1;
                return Ok(());
            }
            loop {
                skip_json_value(bytes, index)?;
                skip_json_whitespace(bytes, index);
                match bytes.get(*index) {
                    Some(b',') => *index += 1,
                    Some(b']') => {
                        *index += 1;
                        return Ok(());
                    }
                    _ => {
                        return Err(format!(
                            "KELD-DOCS004: malformed Cargo metadata array near byte {}",
                            *index
                        ));
                    }
                }
            }
        }
        Some(_) => {
            let start = *index;
            while bytes.get(*index).is_some_and(|byte| {
                !byte.is_ascii_whitespace() && !matches!(byte, b',' | b']' | b'}')
            }) {
                *index += 1;
            }
            if *index == start {
                return Err(format!(
                    "KELD-DOCS004: missing Cargo metadata value near byte {start}"
                ));
            }
            Ok(())
        }
        None => Err("KELD-DOCS004: truncated Cargo metadata value".to_owned()),
    }
}

fn parse_metadata_package(bytes: &[u8], index: &mut usize) -> Result<String, String> {
    expect_json_byte(bytes, index, b'{')?;
    let mut name = None;
    loop {
        skip_json_whitespace(bytes, index);
        if bytes.get(*index) == Some(&b'}') {
            *index += 1;
            break;
        }
        let key = parse_plain_json_string(bytes, index)?;
        expect_json_byte(bytes, index, b':')?;
        if key == "name" {
            name = Some(parse_plain_json_string(bytes, index)?);
        } else {
            skip_json_value(bytes, index)?;
        }
        skip_json_whitespace(bytes, index);
        match bytes.get(*index) {
            Some(b',') => *index += 1,
            Some(b'}') => {
                *index += 1;
                break;
            }
            _ => {
                return Err(format!(
                    "KELD-DOCS004: malformed Cargo package object near byte {}",
                    *index
                ));
            }
        }
    }
    let name = name.ok_or_else(|| "KELD-DOCS004: Cargo package has no name".to_owned())?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(format!(
            "KELD-DOCS004: Cargo emitted unsupported package name `{name}`"
        ));
    }
    Ok(name)
}

fn parse_metadata_packages(bytes: &[u8], index: &mut usize) -> Result<BTreeSet<String>, String> {
    expect_json_byte(bytes, index, b'[')?;
    let mut names = BTreeSet::new();
    loop {
        skip_json_whitespace(bytes, index);
        if bytes.get(*index) == Some(&b']') {
            *index += 1;
            return Ok(names);
        }
        let name = parse_metadata_package(bytes, index)?;
        if !names.insert(name.clone()) {
            return Err(format!(
                "KELD-DOCS004: Cargo workspace oracle repeated package `{name}`"
            ));
        }
        skip_json_whitespace(bytes, index);
        match bytes.get(*index) {
            Some(b',') => *index += 1,
            Some(b']') => {
                *index += 1;
                return Ok(names);
            }
            _ => {
                return Err(format!(
                    "KELD-DOCS004: malformed Cargo packages array near byte {}",
                    *index
                ));
            }
        }
    }
}

fn package_names_from_metadata(bytes: &[u8]) -> Result<BTreeSet<String>, String> {
    let mut index = 0;
    expect_json_byte(bytes, &mut index, b'{')?;
    loop {
        skip_json_whitespace(bytes, &mut index);
        if bytes.get(index) == Some(&b'}') {
            return Err("KELD-DOCS004: Cargo metadata has no packages array".to_owned());
        }
        let key = parse_plain_json_string(bytes, &mut index)?;
        expect_json_byte(bytes, &mut index, b':')?;
        if key == "packages" {
            return parse_metadata_packages(bytes, &mut index);
        }
        skip_json_value(bytes, &mut index)?;
        skip_json_whitespace(bytes, &mut index);
        match bytes.get(index) {
            Some(b',') => index += 1,
            Some(b'}') => {
                return Err("KELD-DOCS004: Cargo metadata has no packages array".to_owned());
            }
            _ => {
                return Err(format!(
                    "KELD-DOCS004: malformed Cargo metadata root near byte {index}"
                ));
            }
        }
    }
}

fn workspace_crate_names(root: &Path) -> Result<BTreeSet<String>, String> {
    workspace_crate_names_with_cargo_home(root, None)
}

fn workspace_crate_names_with_cargo_home(
    root: &Path,
    cargo_home: Option<&Path>,
) -> Result<BTreeSet<String>, String> {
    let mut metadata_command = Command::new("cargo");
    metadata_command
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root);
    if let Some(cargo_home) = cargo_home {
        metadata_command.env("CARGO_HOME", cargo_home);
    }
    let metadata = metadata_command
        .output()
        .map_err(|error| format!("KELD-DOCS004: cannot run Cargo workspace oracle: {error}"))?;
    if !metadata.status.success() {
        return Err(format!(
            "KELD-DOCS004: Cargo workspace oracle failed: {}",
            String::from_utf8_lossy(&metadata.stderr).trim()
        ));
    }
    let names = package_names_from_metadata(&metadata.stdout)?;
    if names.is_empty() {
        return Err(
            "KELD-DOCS004: Cargo.toml workspace has no explicit `crates/*` members.".to_owned(),
        );
    }
    Ok(names)
}

fn check_root_repo_map(root: &Path, records: &[Record]) -> Result<(), String> {
    let agents = read_required(root, "AGENTS.md")?;
    if !links_to_status_ledger(root, "AGENTS.md", &agents) {
        return Err("KELD-DOCS004: AGENTS.md must link directly to the canonical product-status source ledger. Reconcile that root consumer under its instruction key.".to_owned());
    }
    if !links_to_product_status(root, "AGENTS.md", &agents) {
        return Err("KELD-DOCS004: AGENTS.md must link directly to the generated view of product status while naming the TSV as its source ledger. Reconcile that root consumer under its instruction key.".to_owned());
    }
    let ledger = records
        .iter()
        .filter_map(|record| record.id.strip_prefix("crate.").map(ToOwned::to_owned))
        .collect::<BTreeSet<_>>();
    let workspace = workspace_crate_names(root)?;
    if ledger != workspace {
        return Err(format!(
            "KELD-DOCS004: product-status crate inventory is stale against Cargo workspace membership. Workspace={workspace:?}; Ledger={ledger:?}."
        ));
    }
    Ok(())
}

fn just_recipe_commands(contents: &str, name: &str) -> Option<Vec<String>> {
    let header = format!("{name}:");
    let mut lines = contents.lines();
    lines.find(|line| line.trim_end() == header)?;
    let mut commands = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        if !line.chars().next().is_some_and(char::is_whitespace) {
            break;
        }
        let command = line.trim();
        if !command.starts_with('#') {
            commands.push(command.to_owned());
        }
    }
    Some(commands)
}

fn check_consumers(root: &Path, records: &[Record]) -> Result<(), String> {
    for relative in REQUIRED_CONSUMERS {
        let contents = read_required(root, relative)?;
        if !links_to_product_status(root, relative, &contents) {
            return Err(format!(
                "KELD-DOCS004: status consumer `{relative}` does not contain a usable Markdown link to the canonical generated product status. Add the link or remove the duplicate status claim."
            ));
        }
    }
    for relative in ROADMAP_AUTHORITY_CONSUMERS {
        let contents = read_required(root, relative)?;
        match links_to_roadmap(&contents) {
            Ok(true) => {
                return Err(format!(
                    "KELD-DOCS005: `{relative}` still links gitignored ROADMAP.md from an authority-bearing status surface. Point it at `{OUTPUT_REL}`."
                ));
            }
            Ok(false) => {}
            Err(()) => {
                return Err(format!(
                    "KELD-DOCS005: `{relative}` contains visible Markdown links that the status checker cannot parse. Fix the malformed link before validating ROADMAP authority."
                ));
            }
        }
    }
    let doc_map = read_required(root, "docs/onboarding/06-documentation-map.md")?;
    if doc_map.contains(".claude/DEFINITION_OF_DONE.md") {
        return Err("KELD-DOCS005: documentation map names ignored .claude/DEFINITION_OF_DONE.md as authority. Use tracked AGENTS/workflow contracts only.".to_owned());
    }
    let justfile = read_required(root, "justfile")?;
    let ci_prerequisites = justfile
        .lines()
        .find_map(|line| line.strip_prefix("ci:"))
        .map(str::split_whitespace)
        .map(|values| values.collect::<Vec<_>>())
        .ok_or_else(|| "KELD-DOCS005: justfile is missing the root `ci:` recipe.".to_owned())?;
    let position = |name: &str| ci_prerequisites.iter().position(|value| *value == name);
    let (Some(status_test), Some(status_check), Some(llms_test), Some(llms_check)) = (
        position("product-status-test"),
        position("product-status-check"),
        position("llms-test"),
        position("llms-check"),
    ) else {
        return Err("KELD-DOCS004: justfile `ci` must include product-status-test, product-status-check, llms-test, and llms-check.".to_owned());
    };
    if status_test > status_check || status_check > llms_test || llms_test > llms_check {
        return Err("KELD-DOCS004: justfile `ci` must run product-status tests/check before llms tests/check.".to_owned());
    }
    for (recipe, expected) in [
        ("product-status-test", JUST_STATUS_TEST_COMMANDS),
        ("product-status-check", JUST_STATUS_CHECK_COMMANDS),
    ] {
        let actual = just_recipe_commands(&justfile, recipe)
            .ok_or_else(|| format!("KELD-DOCS004: justfile is missing `{recipe}` recipe body."))?;
        if actual
            .iter()
            .map(String::as_str)
            .ne(expected.iter().copied())
        {
            return Err(format!(
                "KELD-DOCS004: justfile `{recipe}` body must run the exact canonical status commands."
            ));
        }
    }
    check_root_repo_map(root, records)
}

fn check(root: &Path) -> Result<(), String> {
    let (root, records) = load(root)?;
    check_generated(&root, &render(&records))?;
    check_consumers(&root, &records)
}

fn run_cli() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(|| {
        "KELD-DOCS005: missing command. Run `product-status generate [workspace]` or `product-status check [workspace]`.".to_owned()
    })?;
    let root = args
        .next()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    if args.next().is_some() {
        return Err("KELD-DOCS005: too many arguments. Run `product-status generate [workspace]` or `product-status check [workspace]`.".to_owned());
    }
    match command.as_str() {
        "generate" => generate(&root),
        "check" => check(&root),
        _ => Err(format!(
            "KELD-DOCS005: unknown command `{command}`. Use generate or check."
        )),
    }
}

fn main() {
    if let Err(error) = run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path =
                env::temp_dir().join(format!("keld-product-status-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create isolated fixture root");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.path.join(relative);
            fs::create_dir_all(path.parent().expect("fixture path has parent"))
                .expect("create fixture parent");
            fs::write(path, contents).expect("write fixture");
        }

        fn read(&self, relative: &str) -> String {
            fs::read_to_string(self.path.join(relative)).expect("read fixture")
        }

        fn replace(&self, relative: &str, old: &str, new: &str) {
            let contents = self.read(relative);
            assert_eq!(contents.matches(old).count(), 1, "mutation anchor `{old}`");
            self.write(relative, &contents.replacen(old, new, 1));
        }

        fn replace_all(&self, relative: &str, old: &str, new: &str) {
            let contents = self.read(relative);
            assert!(contents.contains(old), "mutation anchor `{old}`");
            self.write(relative, &contents.replace(old, new));
        }

        fn remove_line_containing(&self, relative: &str, needle: &str) {
            let contents = self.read(relative);
            assert_eq!(
                contents
                    .lines()
                    .filter(|line| line.contains(needle))
                    .count(),
                1,
                "line mutation anchor `{needle}`"
            );
            let filtered = contents
                .lines()
                .filter(|line| !line.contains(needle))
                .collect::<Vec<_>>()
                .join("\n");
            self.write(relative, &(filtered + "\n"));
        }

        fn git(&self, args: &[&str]) -> String {
            let output = Command::new("git")
                .arg("-C")
                .arg(&self.path)
                .args(args)
                .output()
                .expect("run fixture git");
            assert!(
                output.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout)
                .expect("git output UTF-8")
                .trim()
                .to_owned()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn fixture_ledger() -> String {
        let mut ledger = format!(
            "{SCHEMA_LINE}\n{COLUMNS_LINE}\n\
             crate.keld-core\tcrate:keld-core\tall\tpartial\tHello slice exists.\tcode:crates/keld-core/src/lib.rs;test:crates/keld-core/tests/live.rs\tspecified\tdocs/architecture/01-overview.md\tKEL-1\t{SHA}\n\
             crate.keld-pack\tcrate:keld-pack\tall\tskeleton\tFormat contract only.\tcode:crates/keld-pack/src/lib.rs\tspecified\tdocs/architecture/01-overview.md\t-\t{SHA}\n\
             package.keld-api\tpackage:@keld/api\tall\tspecified\tPackage is specified but absent.\t-\tspecified\tdocs/architecture/01-overview.md\tKEL-2\t{SHA}\n"
        );
        for id in REQUIRED_NON_CRATE_IDS {
            if *id == "package.keld-api" {
                continue;
            }
            let (kind, suffix) = id.split_once('.').expect("required id namespace");
            let owner = expected_package_owner(id)
                .map_or_else(|| format!("{kind}:{suffix}"), ToOwned::to_owned);
            ledger.push_str(&format!(
                "{id}\t{owner}\tall\tspecified\tFixture surface is specified.\t-\tspecified\tdocs/architecture/01-overview.md\t-\t{SHA}\n"
            ));
        }
        ledger
    }

    fn fixture() -> TempDir {
        let temp = TempDir::new();
        temp.write(LEDGER_REL, &fixture_ledger());
        temp.write("crates/keld-core/src/lib.rs", "pub fn live() {}\n");
        temp.write("crates/keld-core/tests/live.rs", "#[test] fn live() {}\n");
        temp.write("crates/keld-pack/src/lib.rs", "pub enum Format {}\n");
        temp.write(
            "Cargo.toml",
            "[workspace]\nresolver = \"3\"\nmembers = [\n    \"crates/keld-core\",\n    \"crates/keld-pack\",\n]\n",
        );
        temp.write(
            "crates/keld-core/Cargo.toml",
            "[package]\nname = \"keld-core\"\nversion = \"0.0.1\"\nedition = \"2024\"\n\n[dependencies]\nserde = \"1\"\n",
        );
        temp.write(
            "crates/keld-pack/Cargo.toml",
            "[package]\nname = \"keld-pack\"\nversion = \"0.0.1\"\nedition = \"2024\"\n",
        );
        temp.write(
            "AGENTS.md",
            "# Agents\n\n[Product status source ledger](docs/engineering/product-status.tsv); [generated status view](docs/engineering/product-status.md).\n",
        );
        for relative in REQUIRED_CONSUMERS {
            let link = if *relative == "README.md" {
                "[status](docs/engineering/product-status.md)"
            } else {
                "[status](../engineering/product-status.md)"
            };
            temp.write(relative, &format!("# Consumer\n\n{link}\n"));
        }
        temp.write(
            "docs/onboarding/05-development-guide.md",
            "# Development\n\n[status](../engineering/product-status.md)\n",
        );
        temp.write(
            "docs/onboarding/06-documentation-map.md",
            "# Documentation\n\nTracked workflow only.\n",
        );
        temp.write(
            "justfile",
            concat!(
                "ci: product-status-test product-status-check llms-test llms-check\n\n",
                "product-status-test:\n",
                "    mkdir -p target/product-status\n",
                "    rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test\n",
                "    target/product-status/product-status-test\n\n",
                "product-status-check:\n",
                "    mkdir -p target/product-status\n",
                "    rustc --edition=2024 -D warnings tools/product_status.rs -o target/product-status/product-status\n",
                "    target/product-status/product-status check .\n",
            ),
        );
        temp
    }

    fn git_fixture() -> (TempDir, String) {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture before snapshot");
        temp.git(&["init", "--quiet"]);
        temp.git(&["config", "user.email", "fixture@example.invalid"]);
        temp.git(&["config", "user.name", "Keld fixture"]);
        temp.git(&["add", "-A"]);
        temp.git(&["commit", "--quiet", "-m", "fixture evidence snapshot"]);
        let evidence_sha = temp.git(&["rev-parse", "HEAD"]);
        temp.replace_all(LEDGER_REL, SHA, &evidence_sha);
        generate(temp.path()).expect("regenerate with evidence snapshot");
        temp.git(&["add", "-A"]);
        temp.git(&["commit", "--quiet", "-m", "add status ledger"]);
        (temp, evidence_sha)
    }

    fn expect_check_error(temp: &TempDir, needle: &str) {
        let error = check(temp.path()).expect_err("mutation must fail");
        assert!(error.contains(needle), "expected `{needle}` in `{error}`");
    }

    #[test]
    fn baseline_generation_is_idempotent_and_check_passes() {
        let temp = fixture();
        generate(temp.path()).expect("first generation");
        let first = temp.read(OUTPUT_REL);
        generate(temp.path()).expect("second generation");
        let second = temp.read(OUTPUT_REL);
        assert_eq!(first, second);
        check(temp.path()).expect("baseline check");
        assert!(!second.contains('\r'));
    }

    #[test]
    fn missing_id_fails() {
        let temp = fixture();
        temp.replace(LEDGER_REL, "crate.keld-core\t", "\t");
        expect_check_error(&temp, "id is missing");
    }

    #[test]
    fn duplicate_id_fails() {
        let temp = fixture();
        temp.replace(
            LEDGER_REL,
            "crate.keld-pack\tcrate:keld-pack\t",
            "crate.keld-core\tcrate:keld-core\t",
        );
        expect_check_error(&temp, "duplicate id");
    }

    #[test]
    fn missing_owner_fails() {
        let temp = fixture();
        temp.replace(LEDGER_REL, "\tcrate:keld-core\t", "\t\t");
        expect_check_error(&temp, "owner is missing");
    }

    #[test]
    fn duplicate_owner_fails() {
        let temp = fixture();
        temp.replace(
            LEDGER_REL,
            "crate.keld-pack\tcrate:keld-pack\t",
            "crate.keld-core\tcrate:keld-core\t",
        );
        expect_check_error(&temp, "duplicate owner");
    }

    #[test]
    fn package_id_owner_swap_fails() {
        let temp = fixture();
        temp.replace(LEDGER_REL, "package:@keld/api", "package:@keld/web");
        expect_check_error(&temp, "canonical package owner");
    }

    #[test]
    fn unknown_id_namespace_fails() {
        let temp = fixture();
        temp.replace(LEDGER_REL, "crate.keld-core\t", "crates.keld-core\t");
        expect_check_error(&temp, "unknown id namespace");
    }

    #[test]
    fn missing_required_stable_id_fails() {
        let temp = fixture();
        temp.remove_line_containing(LEDGER_REL, "phase.4\t");
        expect_check_error(&temp, "required stable status id `phase.4` is missing");
    }

    #[test]
    fn missing_required_package_id_fails() {
        let temp = fixture();
        temp.remove_line_containing(LEDGER_REL, "package.keld-web\t");
        expect_check_error(
            &temp,
            "required stable status id `package.keld-web` is missing",
        );
    }

    #[test]
    fn missing_required_surface_id_fails() {
        let temp = fixture();
        temp.remove_line_containing(LEDGER_REL, "surface.no-flag-linux\t");
        expect_check_error(
            &temp,
            "required stable status id `surface.no-flag-linux` is missing",
        );
    }

    #[test]
    fn missing_crate_id_fails_against_cargo_workspace_inventory() {
        let temp = fixture();
        temp.remove_line_containing(LEDGER_REL, "crate.keld-core\t");
        generate(temp.path()).expect("generate missing-crate mutation");
        expect_check_error(&temp, "stale against Cargo workspace membership");
    }

    #[test]
    fn cargo_oracle_accepts_inline_and_glob_workspace_members() {
        for members in [
            "members = [\"crates/keld-core\", \"crates/keld-pack\"]",
            "members = [\"crates/*\"] # Cargo owns glob expansion",
        ] {
            let temp = fixture();
            temp.write(
                "Cargo.toml",
                &format!("[workspace]\nresolver = \"3\"\n{members}\n"),
            );
            generate(temp.path()).expect("generate Cargo syntax fixture");
            check(temp.path()).expect("Cargo, not this checker, parses workspace members");
        }
    }

    #[test]
    fn cargo_metadata_oracle_passes_with_empty_cargo_home() {
        let temp = fixture();
        let empty_cargo_home = temp.path.join("empty-cargo-home");
        fs::create_dir(&empty_cargo_home).expect("create empty Cargo home");
        assert_eq!(
            workspace_crate_names_with_cargo_home(temp.path(), Some(&empty_cargo_home))
                .expect("metadata --no-deps must not need registry state"),
            BTreeSet::from(["keld-core".to_owned(), "keld-pack".to_owned()])
        );
    }

    #[test]
    fn metadata_parser_reads_package_names_not_nested_target_names() {
        let metadata = br#"{"packages":[{"name":"crate-a","targets":[{"name":"binary-a","src_path":"C:\\work\\a.rs"}]},{"name":"crate_b","features":{}}],"workspace_members":[]}"#;
        assert_eq!(
            package_names_from_metadata(metadata).expect("valid Cargo metadata shape"),
            BTreeSet::from(["crate-a".to_owned(), "crate_b".to_owned()])
        );
        assert!(package_names_from_metadata(br#"{"packages":[{"targets":[]}]}"#).is_err());
    }

    #[test]
    fn non_crates_workspace_member_requires_a_ledger_record() {
        let temp = fixture();
        temp.write(
            "Cargo.toml",
            "[workspace]\nresolver = \"3\"\nmembers = [\"crates/keld-core\", \"crates/keld-pack\", \"tools/helper\"]\n",
        );
        temp.write(
            "tools/helper/Cargo.toml",
            "[package]\nname = \"helper\"\nversion = \"0.0.1\"\nedition = \"2024\"\n",
        );
        temp.write("tools/helper/src/lib.rs", "pub fn helper() {}\n");
        generate(temp.path()).expect("generate non-crates member fixture");
        expect_check_error(&temp, "stale against Cargo workspace membership");
    }

    #[test]
    fn cargo_package_name_not_directory_name_owns_the_crate_id() {
        let temp = fixture();
        temp.replace(
            "crates/keld-pack/Cargo.toml",
            "name = \"keld-pack\"",
            "name = \"renamed-pack\"",
        );
        generate(temp.path()).expect("generate renamed package fixture");
        expect_check_error(&temp, "stale against Cargo workspace membership");
    }

    #[test]
    fn owner_namespace_must_match_id() {
        let temp = fixture();
        temp.replace(LEDGER_REL, "\tcrate:keld-core\t", "\tsurface:keld-core\t");
        expect_check_error(&temp, "does not match id namespace");
    }

    #[test]
    fn unknown_platform_fails() {
        let temp = fixture();
        temp.replace(LEDGER_REL, "\tall\tpartial\t", "\tandroid\tpartial\t");
        expect_check_error(&temp, "unknown platform");
    }

    #[test]
    fn unsupported_completion_classification_fails() {
        let temp = fixture();
        temp.replace(LEDGER_REL, "\tpartial\tHello", "\tcomplete\tHello");
        expect_check_error(&temp, "unsupported current classification");
    }

    #[test]
    fn unknown_target_fails() {
        let temp = fixture();
        temp.replace(
            LEDGER_REL,
            "test:crates/keld-core/tests/live.rs\tspecified\tdocs/architecture/01-overview.md",
            "test:crates/keld-core/tests/live.rs\tplanned\tdocs/architecture/01-overview.md",
        );
        expect_check_error(&temp, "unknown target classification");
    }

    #[test]
    fn missing_evidence_fails() {
        let temp = fixture();
        temp.replace(
            LEDGER_REL,
            "code:crates/keld-core/src/lib.rs;test:crates/keld-core/tests/live.rs",
            "-",
        );
        expect_check_error(&temp, "partial and skeleton rows require code evidence");
    }

    #[test]
    fn live_without_test_fails() {
        let temp = fixture();
        temp.replace(LEDGER_REL, "\tpartial\tHello", "\tlive\tHello");
        temp.replace(
            LEDGER_REL,
            "code:crates/keld-core/src/lib.rs;test:crates/keld-core/tests/live.rs",
            "code:crates/keld-core/src/lib.rs",
        );
        expect_check_error(
            &temp,
            "live requires distinct code and declared test source",
        );
    }

    #[test]
    fn nonexistent_evidence_path_fails() {
        let temp = fixture();
        temp.replace(
            LEDGER_REL,
            "code:crates/keld-core/src/lib.rs",
            "code:crates/keld-core/src/missing.rs",
        );
        expect_check_error(&temp, "does not exist");
    }

    #[test]
    fn evidence_label_must_match_owned_source_class() {
        let temp = fixture();
        temp.replace(
            LEDGER_REL,
            "test:crates/keld-core/tests/live.rs",
            "ci:crates/keld-core/tests/live.rs",
        );
        expect_check_error(&temp, "does not match its owned source class");
    }

    #[test]
    fn crate_evidence_must_belong_to_its_owner() {
        let temp = fixture();
        temp.replace(
            LEDGER_REL,
            "code:crates/keld-pack/src/lib.rs",
            "code:crates/keld-core/src/lib.rs",
        );
        expect_check_error(&temp, "is outside owner `crate:keld-pack`");
    }

    #[test]
    fn one_file_cannot_satisfy_code_and_test() {
        let temp = fixture();
        temp.replace(
            LEDGER_REL,
            "test:crates/keld-core/tests/live.rs",
            "test:crates/keld-core/src/lib.rs",
        );
        expect_check_error(&temp, "is listed more than once");
    }

    #[test]
    fn empty_named_test_file_is_not_declared_test_evidence() {
        let temp = fixture();
        temp.write("crates/keld-core/tests/empty.rs", "");
        temp.replace(
            LEDGER_REL,
            "test:crates/keld-core/tests/live.rs",
            "test:crates/keld-core/tests/empty.rs",
        );
        expect_check_error(&temp, "does not match its owned source class");
    }

    #[test]
    fn commented_test_marker_is_not_declared_test_evidence() {
        let temp = fixture();
        temp.write(
            "crates/keld-core/tests/comment.rs",
            "// #[test]\n// fn decoy() {}\n",
        );
        temp.replace(
            LEDGER_REL,
            "test:crates/keld-core/tests/live.rs",
            "test:crates/keld-core/tests/comment.rs",
        );
        expect_check_error(&temp, "does not match its owned source class");
    }

    #[test]
    fn block_commented_test_marker_is_not_declared_test_evidence() {
        let temp = fixture();
        temp.write(
            "crates/keld-core/tests/comment.rs",
            "/*\n#[test]\nfn decoy() {}\n*/\n",
        );
        temp.replace(
            LEDGER_REL,
            "test:crates/keld-core/tests/live.rs",
            "test:crates/keld-core/tests/comment.rs",
        );
        expect_check_error(&temp, "does not match its owned source class");
    }

    #[test]
    fn string_test_marker_is_not_declared_test_evidence() {
        let temp = fixture();
        temp.write(
            "crates/keld-core/tests/string.rs",
            "const DECOY: &str = \"#[test] fn decoy() {}\";\n",
        );
        temp.replace(
            LEDGER_REL,
            "test:crates/keld-core/tests/live.rs",
            "test:crates/keld-core/tests/string.rs",
        );
        expect_check_error(&temp, "does not match its owned source class");
    }

    #[test]
    fn rust_lifetime_before_test_marker_remains_declared_test_evidence() {
        let temp = fixture();
        temp.write(
            "crates/keld-core/tests/live.rs",
            "fn borrow<'a>(value: &'a str) -> &'a str { value }\n#[test]\nfn live() {}\n",
        );
        generate(temp.path()).expect("generate lifetime fixture");
        check(&temp.path).expect("Rust lifetimes must not hide a real test marker");
    }

    #[test]
    fn rust_quote_char_literals_do_not_hide_following_test_markers() {
        for literal in [r#"'"'"#, r#"'\"'"#] {
            assert!(
                has_declared_test(
                    "tests/live.rs",
                    &format!("const QUOTE: char = {literal};\n#[test]\nfn live() {{}}\n"),
                ),
                "char literal {literal} hid the following test marker"
            );
        }
    }

    #[test]
    fn bun_test_modifiers_are_declared_test_evidence() {
        for declaration in [
            "test.only(\"focused\", () => {});",
            "it.only(\"focused\", () => {});",
            "test.each([])(\"table\", () => {});",
            "it.each([])(\"table\", () => {});",
        ] {
            assert!(has_declared_test("src/example.test.ts", declaration));
        }
    }

    #[test]
    fn declared_test_evidence_is_not_an_execution_receipt() {
        assert!(has_declared_test(
            "tests/ignored.rs",
            "#[ignore]\n#[test]\nfn ignored() {}\n",
        ));
        assert!(has_declared_test(
            "tests/disabled.rs",
            "#[cfg(any())]\n#[test]\nfn disabled() {}\n",
        ));
        assert!(!has_declared_test(
            "tests/decoy.rs",
            "#[testify]\nfn not_a_test() {}\n",
        ));
    }

    #[test]
    fn nonexistent_target_path_fails() {
        let temp = fixture();
        temp.replace(
            LEDGER_REL,
            "test:crates/keld-core/tests/live.rs\tspecified\tdocs/architecture/01-overview.md",
            "test:crates/keld-core/tests/live.rs\tspecified\tdocs/architecture/missing.md",
        );
        expect_check_error(&temp, "does not exist");
    }

    #[test]
    fn target_source_must_be_normative() {
        let temp = fixture();
        temp.write("README-target.md", "# Not normative\n");
        temp.replace(
            LEDGER_REL,
            "test:crates/keld-core/tests/live.rs\tspecified\tdocs/architecture/01-overview.md",
            "test:crates/keld-core/tests/live.rs\tspecified\tREADME-target.md",
        );
        expect_check_error(&temp, "must be owned by docs/architecture or docs/specs");
    }

    #[test]
    fn absolute_evidence_path_fails() {
        let temp = fixture();
        temp.replace(
            LEDGER_REL,
            "code:crates/keld-core/src/lib.rs",
            "code:/outside.rs",
        );
        expect_check_error(&temp, "outside the public tracked contract");
    }

    #[test]
    fn parent_evidence_path_fails() {
        let temp = fixture();
        temp.replace(
            LEDGER_REL,
            "code:crates/keld-core/src/lib.rs",
            "code:../outside.rs",
        );
        expect_check_error(&temp, "outside the public tracked contract");
    }

    #[test]
    fn roadmap_evidence_path_fails_even_when_present() {
        let temp = fixture();
        temp.write("ROADMAP.md", "private roadmap\n");
        temp.replace(
            LEDGER_REL,
            "code:crates/keld-core/src/lib.rs",
            "code:ROADMAP.md",
        );
        expect_check_error(&temp, "outside the public tracked contract");
    }

    #[test]
    fn research_evidence_path_fails_even_when_present() {
        let temp = fixture();
        temp.write("docs/research/private.md", "private research\n");
        temp.replace(
            LEDGER_REL,
            "code:crates/keld-core/src/lib.rs",
            "code:docs/research/private.md",
        );
        expect_check_error(&temp, "outside the public tracked contract");
    }

    #[test]
    fn malformed_issue_fails() {
        let temp = fixture();
        temp.replace(LEDGER_REL, "\tKEL-1\t", "\tLINEAR-1\t");
        expect_check_error(&temp, "malformed issue");
    }

    #[test]
    fn duplicate_issue_fails() {
        let temp = fixture();
        temp.replace(LEDGER_REL, "\tKEL-1\t", "\tKEL-1,KEL-1\t");
        expect_check_error(&temp, "duplicate issue");
    }

    #[test]
    fn bad_sha_fails() {
        let temp = fixture();
        temp.replace(LEDGER_REL, &format!("\tKEL-2\t{SHA}"), "\tKEL-2\tABC123");
        expect_check_error(&temp, "40-character lowercase hexadecimal");
    }

    #[test]
    fn nonexistent_well_formed_sha_fails_in_a_git_checkout() {
        let (temp, evidence_sha) = git_fixture();
        temp.replace_all(
            LEDGER_REL,
            &evidence_sha,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        expect_check_error(&temp, "is not a commit");
    }

    #[test]
    fn file_added_after_evidence_snapshot_fails() {
        let (temp, _) = git_fixture();
        temp.write("crates/keld-core/src/later.rs", "pub fn later() {}\n");
        temp.replace(
            LEDGER_REL,
            "code:crates/keld-core/src/lib.rs",
            "code:crates/keld-core/src/later.rs",
        );
        expect_check_error(&temp, "not a tracked regular file");
    }

    #[test]
    fn git_metadata_cannot_be_evidence() {
        let (temp, _) = git_fixture();
        temp.replace(
            LEDGER_REL,
            "code:crates/keld-core/src/lib.rs",
            "code:.git/config",
        );
        expect_check_error(&temp, "outside the public tracked contract");
    }

    #[test]
    fn git_metadata_rejection_is_case_insensitive() {
        assert!(escapes_public_repo(Path::new(".GIT/config")));
        assert!(escapes_public_repo(Path::new("RoadMap.md")));
        assert!(escapes_public_repo(Path::new(
            "crates/keld-core/tests/CON.test.md"
        )));
        assert!(escapes_public_repo(Path::new("evidence.txt:stream")));
        assert!(escapes_public_repo(Path::new("trailing-dot.")));
    }

    #[test]
    fn free_prose_does_not_override_structured_current() {
        let temp = fixture();
        temp.replace(
            LEDGER_REL,
            "Hello slice exists.",
            "Support is not complete.",
        );
        let records = parse_ledger(&temp.read(LEDGER_REL)).expect("structured current controls");
        let core = records
            .iter()
            .find(|record| record.id == "crate.keld-core")
            .expect("fixture core row");
        assert_eq!(core.current, Current::Partial);
    }

    #[test]
    fn stale_generated_current_fails() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.replace(OUTPUT_REL, "**Partial**", "**Skeleton**");
        expect_check_error(&temp, "generated output");
    }

    #[test]
    fn ledger_target_change_without_regeneration_fails() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write(
            "docs/architecture/alternate.md",
            "# Alternate\n\n[status](../engineering/product-status.md)\n",
        );
        temp.replace(
            LEDGER_REL,
            "test:crates/keld-core/tests/live.rs\tspecified\tdocs/architecture/01-overview.md",
            "test:crates/keld-core/tests/live.rs\tspecified\tdocs/architecture/alternate.md",
        );
        expect_check_error(&temp, "generated output");
    }

    #[test]
    fn root_repo_map_must_link_directly_to_ledger() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.replace(
            "AGENTS.md",
            "docs/engineering/product-status.tsv",
            "docs/architecture/01-overview.md",
        );
        expect_check_error(&temp, "source ledger");
    }

    #[test]
    fn root_repo_map_must_link_to_generated_view() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.replace(
            "AGENTS.md",
            "docs/engineering/product-status.md",
            "docs/architecture/01-overview.md",
        );
        expect_check_error(&temp, "generated view");
    }

    #[test]
    fn visible_link_targets_share_scanning_and_normalization() {
        let targets = visible_link_targets(concat!(
            "[status](<docs/engineering/product-status.md#current> \"view\") ",
            "![image](assets/status.svg) ",
            "[old](..\\..\\ROADMAP.md#phase)\n",
        ))
        .expect("well-formed visible links");

        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].target, "docs/engineering/product-status.md");
        assert_eq!(targets[0].slash_normalized, targets[0].target);
        assert!(!targets[0].image);
        assert_eq!(targets[1].target, "assets/status.svg");
        assert!(targets[1].image);
        assert_eq!(targets[2].target, "..\\..\\ROADMAP.md");
        assert_eq!(targets[2].slash_normalized, "../../ROADMAP.md");
        assert!(
            visible_link_targets("[bad](<docs/engineering/product-status.md>suffix)").is_none(),
            "non-whitespace text after an angled target is not a valid title"
        );
    }

    #[test]
    fn missing_canonical_consumer_link_fails() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write("README.md", "# Consumer without status link\n");
        expect_check_error(&temp, "usable Markdown link");
    }

    #[test]
    fn commented_consumer_link_is_a_decoy() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write(
            "README.md",
            "# Consumer\n\n<!-- [status](docs/engineering/product-status.md) -->\n",
        );
        expect_check_error(&temp, "usable Markdown link");
    }

    #[test]
    fn fenced_consumer_link_is_a_decoy() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write(
            "README.md",
            "# Consumer\n\n```md\n[status](docs/engineering/product-status.md)\n```\n",
        );
        expect_check_error(&temp, "usable Markdown link");
    }

    #[test]
    fn wider_fence_consumer_link_is_a_decoy() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write(
            "README.md",
            "# Consumer\n\n````md\n```\n[status](docs/engineering/product-status.md)\n```\n````\n",
        );
        expect_check_error(&temp, "usable Markdown link");
    }

    #[test]
    fn inline_code_consumer_link_is_a_decoy() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write(
            "README.md",
            "# Consumer\n\n`[status](docs/engineering/product-status.md)`\n",
        );
        expect_check_error(&temp, "usable Markdown link");
    }

    #[test]
    fn image_consumer_link_is_a_decoy() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write(
            "README.md",
            "# Consumer\n\n![status](docs/engineering/product-status.md)\n",
        );
        expect_check_error(&temp, "usable Markdown link");
    }

    #[test]
    fn indented_code_consumer_link_is_a_decoy() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write(
            "README.md",
            "# Consumer\n\n    [status](docs/engineering/product-status.md)\n",
        );
        expect_check_error(&temp, "usable Markdown link");
    }

    #[test]
    fn blockquote_indented_code_consumer_link_is_a_decoy() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write(
            "README.md",
            "# Consumer\n\n>     [status](docs/engineering/product-status.md)\n",
        );
        expect_check_error(&temp, "usable Markdown link");
    }

    #[test]
    fn broken_consumer_link_fails() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write(
            "README.md",
            "# Consumer\n\n[status](missing/product-status.md)\n",
        );
        expect_check_error(&temp, "usable Markdown link");
    }

    #[test]
    fn harmless_roadmap_prose_is_not_authority() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write(
            "docs/onboarding/05-development-guide.md",
            "# Development\n\n[status](../engineering/product-status.md)\n\nROADMAP.md is not authoritative.\n",
        );
        check(temp.path()).expect("plain roadmap prose is not a link");
    }

    #[test]
    fn roadmap_authority_link_fails() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write(
            "docs/onboarding/05-development-guide.md",
            "# Development\n\n[status](../engineering/product-status.md)\n\n[old status](../../ROADMAP.md)\n",
        );
        expect_check_error(&temp, "still links gitignored ROADMAP.md");
    }

    #[test]
    fn malformed_link_cannot_hide_later_roadmap_authority() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.write(
            "docs/onboarding/05-development-guide.md",
            "# Development\n\n[bad](<status.md>suffix)\n[old status](../../ROADMAP.md)\n",
        );
        expect_check_error(&temp, "cannot parse");
    }

    #[test]
    fn removing_status_from_just_ci_fails() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.replace("justfile", " product-status-test", "");
        expect_check_error(&temp, "justfile `ci` must include");
    }

    #[test]
    fn no_op_status_recipe_body_fails() {
        let temp = fixture();
        generate(temp.path()).expect("generate fixture");
        temp.replace(
            "justfile",
            "    target/product-status/product-status-test",
            "    true",
        );
        expect_check_error(&temp, "body must run the exact canonical status commands");
    }

    #[test]
    fn private_sentinels_do_not_leak_into_generated_status() {
        let temp = fixture();
        temp.write("docs/research/secret.md", "RESEARCH_SENTINEL\n");
        temp.write("competitors/secret.md", "COMPETITOR_SENTINEL\n");
        temp.write(".claude/secret.md", "CLAUDE_SENTINEL\n");
        generate(temp.path()).expect("generate public ledger");
        let rendered = temp.read(OUTPUT_REL);
        for sentinel in [
            "RESEARCH_SENTINEL",
            "COMPETITOR_SENTINEL",
            "CLAUDE_SENTINEL",
        ] {
            assert!(!rendered.contains(sentinel), "{sentinel} leaked");
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_evidence_escape_fails() {
        let temp = fixture();
        let outside = TempDir::new();
        outside.write("outside.rs", "secret\n");
        let link = temp.path().join("crates/keld-core/src/link.rs");
        std::os::unix::fs::symlink(outside.path().join("outside.rs"), &link)
            .expect("create escaping symlink");
        temp.replace(
            LEDGER_REL,
            "code:crates/keld-core/src/lib.rs",
            "code:crates/keld-core/src/link.rs",
        );
        expect_check_error(&temp, "resolves outside the checkout");
    }

    #[cfg(windows)]
    #[test]
    fn windows_drive_relative_and_unc_paths_escape() {
        assert!(escapes_public_repo(Path::new("C:relative.txt")));
        assert!(escapes_public_repo(Path::new(
            r"\\server\share\evidence.rs"
        )));
    }

    #[cfg(windows)]
    #[test]
    fn windows_junction_escape_fails() {
        let temp = fixture();
        let outside = TempDir::new();
        outside.write("outside.rs", "pub fn secret() {}\n");
        let link = temp.path().join("crates/keld-core/src/junction");
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "New-Item",
                "-ItemType",
                "Junction",
                "-Path",
            ])
            .arg(&link)
            .arg("-Target")
            .arg(outside.path())
            .output()
            .expect("create fixture junction");
        assert!(
            output.status.success(),
            "New-Item Junction: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        temp.replace(
            LEDGER_REL,
            "code:crates/keld-core/src/lib.rs",
            "code:crates/keld-core/src/junction/outside.rs",
        );
        expect_check_error(&temp, "resolves outside the checkout");
        fs::remove_dir(&link).expect("remove fixture junction without touching target");
    }
}
