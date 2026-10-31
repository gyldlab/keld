//! Enforces the repository spec standard owned by `.agents/specs.md` (#1321).
//!
//! Review rounds on #1280, #1307 and #1311 kept finding dangling references and counts
//! that disagree, because no agent can hold a 2,000-line spec and nothing checked one.
//! This gate enforces only what a machine can decide without false positives: size,
//! layout, stable IDs, required fields, relative links and pin citations. Prose quality
//! (ASD-STE100 sentence length, FACT/INFERRED/UNKNOWN labels, quote accuracy) stays a
//! review duty; `.agents/specs.md` says why.
//!
//! Landed files that predate the standard pass only through a ratchet: the baseline
//! records each one's over-cap values, a row must equal the measured value and must not
//! exceed that file's measurement at `BASELINE_SEAL`, and a row leaves when its file
//! complies. A sealed spec that moves to the standard before its gaps are filled keeps a
//! `migrated:<n>` row, which is the only way an `UNKNOWN` required field passes; a new
//! spec cannot hold one. `BASELINE_SEAL` and `BASELINE_ROWS` stop a new file from
//! entering. A change can lower the count. Raising it, or moving the seal, is a visible
//! edit under code-owner review; no in-repository check can forbid a pull request that
//! rewrites both the rule and its data.

#[path = "markdown_contract.rs"]
#[allow(
    dead_code,
    reason = "shared contract module; this checker uses only its visibility primitives"
)]
mod markdown_contract;

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use markdown_contract::{fence_marker, without_inline_code, without_struck_text};

const SPEC_ROOT: &str = "docs/specs";
const BASELINE_FILE: &str = ".spec-baseline.tsv";
const BASELINE_HEADER: &str = "# path\tlines\tbytes\tline_overflow\tstructure";
const TEMPLATE: &str = "docs/agents/spec-template.md";
const INDEX_FILE: &str = "index.md";

/// Line cap the repository owner set for one spec file.
const MAX_LINES: usize = 500;
/// 500 lines at 82 bytes per line, the census p80; see `.agents/specs.md`.
const MAX_BYTES: usize = 40 * 1024;
/// About five wrapped lines, so one table row cannot hide a page.
const MAX_LINE_CHARS: usize = 400;
/// An agent always loads the index, so it stays a map.
const INDEX_MAX_LINES: usize = 120;
const INDEX_MAX_BYTES: usize = 8 * 1024;

/// Exact baseline row count. Lower it when a row drains; never raise it.
const BASELINE_ROWS: usize = 42;
/// Every baseline path must exist in this commit: the last `main` before the standard.
/// Moving it forward is legitimate only for a rebase over a newly landed spec.
const BASELINE_SEAL: &str = "8e6f544ff3b7719d305c27a461b3db84dd5fed5a";

/// The ten sections of `docs/agents/spec-template.md`, in order.
const SECTIONS: [&str; 10] = [
    "Goal & non-goals",
    "Spec refs",
    "Acceptance criteria",
    "Design",
    "Boundaries",
    "Tasks",
    "Test plan & evidence",
    "Review gates triggered",
    "Perf impact",
    "Open questions, blockers & unknowns",
];
/// Sections the index owns in a directory spec; every other section lives in a part.
const INDEX_SECTIONS: [usize; 2] = [1, 8];
const PARTS_HEADING: &str = "Parts";
const BLOCKERS_HEADING: &str = "Current blockers";
const PART_TABLE_HEADER: &str = "| Part | Owns | Load when |";

const STATUSES: [&str; 4] = ["draft", "approved", "implementing", "done"];
const AC_FIELDS: [&str; 7] = [
    "Given",
    "When",
    "Then",
    "Negative control",
    "Single fault",
    "At-risk precondition",
    "Expected RED",
];
const BLOCKER_FIELDS: [&str; 4] = ["Status", "Owner", "Impact", "Check"];
const EVIDENCE_FIELDS: [&str; 4] = ["Run", "n", "Environment", "Limits"];
const TASK_BLOCKERS_FIELD: &str = "Blocked by";
/// AC fields where a bare `none` states nothing. `Given: none` is a real statement.
const CONTROL_FIELDS: [&str; 4] = [
    "Then",
    "Single fault",
    "At-risk precondition",
    "Expected RED",
];
/// Whole values that fill a required field without saying anything.
const NON_VALUES: [&str; 11] = [
    "tbd", "tba", "todo", "n/a", "na", "-", "--", "\u{2014}", "?", "???", "pending",
];
/// A heading with this text marks a spec, wherever the file sits.
const SPEC_SIGNATURE: &str = "acceptance criteria";

/// Stable ID kinds and the one section that may define each.
const KINDS: [(&str, usize); 7] = [
    ("AC", 3),
    ("D", 4),
    ("T", 6),
    ("E", 7),
    ("B", 10),
    ("Q", 10),
    ("U", 10),
];

/// Git variables that would redirect a child `git -C` when a hook runs this gate.
const GIT_ENV: [&str; 5] = [
    "GIT_DIR",
    "GIT_COMMON_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rule {
    SizeLines,
    SizeBytes,
    SizeLineLength,
    IndexSize,
    Name,
    Entry,
    IndexMissing,
    PartUnlisted,
    PartMissing,
    PartHeader,
    PartTable,
    Fields,
    Sections,
    Link,
    LinkCode,
    Anchor,
    Outside,
    IdDuplicate,
    IdDangling,
    IdSection,
    IdLegacyForm,
    IdRequired,
    Fence,
    AcFields,
    TaskFields,
    BlockerFields,
    EvidenceFields,
    FieldUnknown,
    FieldPlaceholder,
    OpenBlockers,
    PinMissing,
    CitePath,
    CiteLine,
    CiteSha,
    BaselineFormat,
    BaselineNew,
    BaselineCount,
    BaselineGrew,
    BaselineRaised,
    BaselineStale,
}

impl Rule {
    fn code(self) -> &'static str {
        match self {
            Rule::SizeLines => "SIZE-LINES",
            Rule::SizeBytes => "SIZE-BYTES",
            Rule::SizeLineLength => "SIZE-LINE-LENGTH",
            Rule::IndexSize => "INDEX-SIZE",
            Rule::Name => "NAME",
            Rule::Entry => "ENTRY",
            Rule::IndexMissing => "INDEX-MISSING",
            Rule::PartUnlisted => "PART-UNLISTED",
            Rule::PartMissing => "PART-MISSING",
            Rule::PartHeader => "PART-HEADER",
            Rule::PartTable => "PART-TABLE",
            Rule::Fields => "FIELDS",
            Rule::Sections => "SECTIONS",
            Rule::Link => "LINK",
            Rule::LinkCode => "LINK-CODE",
            Rule::Anchor => "ANCHOR",
            Rule::Outside => "OUTSIDE",
            Rule::IdDuplicate => "ID-DUPLICATE",
            Rule::IdDangling => "ID-DANGLING",
            Rule::IdSection => "ID-SECTION",
            Rule::IdLegacyForm => "ID-LEGACY-FORM",
            Rule::IdRequired => "ID-REQUIRED",
            Rule::Fence => "FENCE",
            Rule::AcFields => "AC-FIELDS",
            Rule::TaskFields => "TASK-FIELDS",
            Rule::BlockerFields => "BLOCKER-FIELDS",
            Rule::EvidenceFields => "EVIDENCE-FIELDS",
            Rule::FieldUnknown => "FIELD-UNKNOWN",
            Rule::FieldPlaceholder => "FIELD-PLACEHOLDER",
            Rule::OpenBlockers => "OPEN-BLOCKERS",
            Rule::PinMissing => "PIN-MISSING",
            Rule::CitePath => "CITE-PATH",
            Rule::CiteLine => "CITE-LINE",
            Rule::CiteSha => "CITE-SHA",
            Rule::BaselineFormat => "BASELINE-FORMAT",
            Rule::BaselineNew => "BASELINE-NEW",
            Rule::BaselineCount => "BASELINE-COUNT",
            Rule::BaselineGrew => "BASELINE-GREW",
            Rule::BaselineRaised => "BASELINE-RAISED",
            Rule::BaselineStale => "BASELINE-STALE",
        }
    }
}

#[derive(Debug)]
struct Finding {
    rule: Rule,
    file: String,
    line: usize,
    detail: String,
}

/// Counts a clean run must not hide: a skipped check is reported, never implied.
#[derive(Debug, Default)]
struct Report {
    findings: Vec<Finding>,
    specs: usize,
    exempt: usize,
    citations_verified: usize,
    citations_unverifiable: usize,
    private_links: usize,
}

impl Report {
    fn add(&mut self, rule: Rule, file: &str, line: usize, detail: String) {
        self.findings.push(Finding {
            rule,
            file: file.to_owned(),
            line,
            detail,
        });
    }

    /// Takes over one spec's separately collected result.
    fn absorb(&mut self, mut other: Report) {
        self.findings.append(&mut other.findings);
        self.citations_verified += other.citations_verified;
        self.citations_unverifiable += other.citations_unverifiable;
    }
}

/// Git facts the checker needs. Tests substitute a table for the real repository.
trait History {
    fn paths_at(&self, commit: &str, dir: &str) -> Result<BTreeSet<String>, String>;
    fn has_commit(&self, sha: &str) -> bool;
    /// True when `sha` stays in a fresh clone of this branch after its PR merges.
    fn is_reachable(&self, sha: &str) -> bool;
    fn text_at(&self, commit: &str, path: &str) -> Option<String>;
    /// Every tracked Markdown path. An untracked private checkout is not repository text.
    fn tracked_markdown(&self) -> Result<Vec<String>, String>;

    fn lines_at(&self, commit: &str, path: &str) -> Option<usize> {
        Some(self.text_at(commit, path)?.lines().count())
    }
}

struct GitHistory<'a> {
    root: &'a Path,
}

impl GitHistory<'_> {
    /// Standard output, or Git's own explanation of why it failed.
    fn git(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        let mut command = Command::new("git");
        command.arg("-C").arg(self.root).args(args);
        for name in GIT_ENV {
            command.env_remove(name);
        }
        let output = command
            .output()
            .map_err(|error| format!("cannot start git: {error}"))?;
        if output.status.success() {
            return Ok(output.stdout);
        }
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }

    fn is_shallow(&self) -> bool {
        self.git(&["rev-parse", "--is-shallow-repository"])
            .is_ok_and(|stdout| stdout.starts_with(b"true"))
    }
}

/// The one action that restores a seal this clone cannot read.
fn seal_fix(shallow: bool) -> &'static str {
    if shallow {
        "This clone is shallow: run `git fetch --unshallow`, or set `fetch-depth: 0` on the \
         checkout."
    } else {
        "This clone lacks the commit: run `git fetch origin main`. A fork runs `git fetch \
         <upstream> main`."
    }
}

impl History for GitHistory<'_> {
    fn paths_at(&self, commit: &str, dir: &str) -> Result<BTreeSet<String>, String> {
        let stdout = self
            .git(&["ls-tree", "-r", "--name-only", commit, "--", dir])
            .map_err(|detail| {
                format!(
                    "SPEC-STRUCTURE: cannot read `{dir}` at baseline seal {commit}: {detail}. {}",
                    seal_fix(self.is_shallow())
                )
            })?;
        Ok(String::from_utf8_lossy(&stdout)
            .lines()
            .map(str::to_owned)
            .collect())
    }

    fn has_commit(&self, sha: &str) -> bool {
        self.git(&["cat-file", "-e", &format!("{sha}^{{commit}}")])
            .is_ok()
    }

    fn is_reachable(&self, sha: &str) -> bool {
        self.git(&["merge-base", "--is-ancestor", sha, "HEAD"])
            .is_ok()
    }

    fn text_at(&self, commit: &str, path: &str) -> Option<String> {
        let stdout = self.git(&["show", &format!("{commit}:{path}")]).ok()?;
        Some(String::from_utf8_lossy(&stdout).into_owned())
    }

    fn tracked_markdown(&self) -> Result<Vec<String>, String> {
        let stdout = self
            .git(&["ls-files", "-z", "--", "*.md"])
            .map_err(|detail| format!("SPEC-STRUCTURE: cannot list tracked Markdown: {detail}"))?;
        Ok(String::from_utf8_lossy(&stdout)
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
            .collect())
    }
}

/// Ratchet limits, separate from the source constants so fixtures can set their own.
struct Config<'a> {
    rows: usize,
    seal: &'a str,
}

const PRODUCTION: Config<'static> = Config {
    rows: BASELINE_ROWS,
    seal: BASELINE_SEAL,
};

/// One Markdown file. `visible[i]` is `None` for fenced lines and fence markers.
struct Doc {
    rel: String,
    lines: Vec<String>,
    visible: Vec<Option<String>>,
    bytes: usize,
    /// A fence that never closes hides the rest of the file from every other rule.
    unclosed_fence: Option<usize>,
}

impl Doc {
    fn parse(rel: &str, raw: &str) -> Doc {
        let text = raw.replace("\r\n", "\n");
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let mut fence: Option<(u8, usize, usize)> = None;
        let visible = lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                if let Some((marker, width, closing_tail)) = fence_marker(line) {
                    match fence {
                        Some((open_marker, open_width, _))
                            if marker == open_marker && width >= open_width && closing_tail =>
                        {
                            fence = None;
                        }
                        None => fence = Some((marker, width, index)),
                        _ => {}
                    }
                    return None;
                }
                fence.is_none().then(|| without_struck_text(line))
            })
            .collect();
        Doc {
            rel: rel.to_owned(),
            lines,
            visible,
            bytes: text.len(),
            unclosed_fence: fence.map(|(_, _, index)| index),
        }
    }

    fn headings(&self) -> Vec<Heading> {
        self.visible
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                let line = line.as_deref()?;
                let level = line.bytes().take_while(|byte| *byte == b'#').count();
                let text = line.get(level..)?.strip_prefix(' ')?;
                ((1..=6).contains(&level)).then(|| Heading {
                    index,
                    level,
                    text: text.trim().to_owned(),
                })
            })
            .collect()
    }

    /// Lines after heading `at` up to the next heading of the same or a higher level.
    fn body_end(&self, headings: &[Heading], at: usize) -> usize {
        headings[at + 1..]
            .iter()
            .find(|heading| heading.level <= headings[at].level)
            .map_or(self.lines.len(), |heading| heading.index)
    }
}

#[derive(Debug)]
struct Heading {
    index: usize,
    level: usize,
    text: String,
}

/// Section number of a `## n. Title` heading whose title is canonical.
fn canonical_section(text: &str) -> Option<usize> {
    SECTIONS.iter().enumerate().find_map(|(index, title)| {
        let exact = format!("{}. {title}", index + 1);
        (text == exact || text.starts_with(&format!("{exact}: "))).then_some(index + 1)
    })
}

/// Why a required field holds no statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NonValue {
    Unknown,
    Placeholder,
}

/// Matches whole markers only, so `Unknown channels are refused` stays a real statement.
fn non_value(value: &str, control: bool) -> Option<NonValue> {
    let bare = value.trim_matches(|ch: char| ch.is_whitespace() || ".,:;!*_`()".contains(ch));
    let label_tail = bare.strip_prefix("UNKNOWN");
    let lower = bare.to_lowercase();
    if lower == "unknown" || label_tail.is_some_and(|tail| !tail.starts_with(char::is_alphanumeric))
    {
        Some(NonValue::Unknown)
    } else if (bare.starts_with('<') && bare.ends_with('>'))
        || NON_VALUES.contains(&lower.as_str())
        || (control && lower == "none")
    {
        Some(NonValue::Placeholder)
    } else {
        None
    }
}

/// Value of `label` on a `- **Label:** value` or `Label: value` line.
fn field_value(line: &str, label: &str) -> Option<String> {
    let mut text = line.trim_start();
    text = text
        .strip_prefix("- ")
        .or_else(|| text.strip_prefix("* "))
        .unwrap_or(text);
    text = text.strip_prefix("**").unwrap_or(text);
    let text = text.strip_prefix(label)?.strip_prefix(':')?;
    Some(text.strip_prefix("**").unwrap_or(text).trim().to_owned())
}

/// Finds `label` in a line range. A field may hold its value on nested lines.
fn find_field(doc: &Doc, start: usize, end: usize, label: &str) -> Option<(usize, String)> {
    (start..end).find_map(|index| {
        let line = doc.visible[index].as_deref()?;
        let value = field_value(line, label)?;
        if !value.is_empty() {
            return Some((index, value));
        }
        let nested = doc
            .lines
            .get(index + 1)
            .is_some_and(|next| next.starts_with(char::is_whitespace) && !next.trim().is_empty());
        Some((index, if nested { "(nested)".to_owned() } else { value }))
    })
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// One stable-ID token on a line.
#[derive(Debug, PartialEq, Eq)]
struct IdToken {
    kind: &'static str,
    number: u32,
    legacy_form: bool,
    /// Preceded by `#n`, `KEL-n`, `GH-n` or a link: another spec's ID.
    foreign: bool,
}

/// `AC-3`-style text at the start of `text`, returning its kind and number.
fn parse_id(text: &str) -> Option<(&'static str, u32, usize)> {
    KINDS.iter().find_map(|(kind, _)| {
        let digits = text.strip_prefix(kind)?.strip_prefix('-')?;
        let width = digits.bytes().take_while(u8::is_ascii_digit).count();
        let number = digits.get(..width)?.parse().ok()?;
        let end = kind.len() + 1 + width;
        let next = text.as_bytes().get(end).copied();
        (width > 0 && !next.is_some_and(|byte| is_word_byte(byte) || byte == b'-'))
            .then_some((*kind, number, end))
    })
}

fn is_tracker_word(word: &str) -> bool {
    let digits = word
        .strip_prefix('#')
        .or_else(|| word.strip_prefix("KEL-"))
        .or_else(|| word.strip_prefix("GH-"));
    digits.is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
}

fn foreign_prefix(before: &str) -> bool {
    if !before.ends_with(char::is_whitespace) {
        return false;
    }
    let word = before
        .trim_end()
        .rsplit(char::is_whitespace)
        .next()
        .unwrap_or("")
        .trim_end_matches([',', ':']);
    // `[#1311](url) AC-2` is foreign. `[goal](index.md) AC-2` is this spec's AC-2.
    let link_text = word
        .split_once("](")
        .filter(|_| word.ends_with(')'))
        .and_then(|(text, _)| text.strip_prefix('['));
    is_tracker_word(word) || link_text.is_some_and(is_tracker_word)
}

/// Every stable-ID token on a visible line. URLs are not prose.
fn id_tokens(line: &str) -> Vec<IdToken> {
    let mut tokens = Vec::new();
    let mut word_start = 0;
    let bytes = line.as_bytes();
    for word in line.split_inclusive(char::is_whitespace) {
        if !word.contains("://") {
            for (offset, _) in word.char_indices() {
                let at = word_start + offset;
                if at > 0 && (is_word_byte(bytes[at - 1]) || bytes[at - 1] == b'-') {
                    continue;
                }
                let rest = &line[at..];
                if let Some((kind, number, _)) = parse_id(rest) {
                    tokens.push(IdToken {
                        kind,
                        number,
                        legacy_form: false,
                        foreign: foreign_prefix(&line[..at]),
                    });
                } else if let Some(digits) = rest.strip_prefix("AC") {
                    let width = digits.bytes().take_while(u8::is_ascii_digit).count();
                    let next = digits.as_bytes().get(width).copied();
                    if width > 0 && !next.is_some_and(is_word_byte) {
                        tokens.push(IdToken {
                            kind: "AC",
                            number: digits[..width].parse().unwrap_or(0),
                            legacy_form: true,
                            foreign: foreign_prefix(&line[..at]),
                        });
                    }
                }
            }
        }
        word_start += word.len();
    }
    tokens
}

/// Inline Markdown link targets on a visible line.
fn link_targets(line: &str) -> Vec<String> {
    let line = without_inline_code(line);
    let mut targets = Vec::new();
    let mut rest = line.as_str();
    while let Some(at) = rest.find("](") {
        let tail = &rest[at + 2..];
        let mut depth = 1;
        let Some(end) = tail.char_indices().find_map(|(offset, ch)| {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(offset)
        }) else {
            break;
        };
        let target = tail[..end].split_whitespace().next().unwrap_or("");
        targets.push(
            target
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_owned(),
        );
        rest = &tail[end + 1..];
    }
    targets
}

/// GitHub's heading anchors, including the `-1` suffix of a repeated heading.
fn heading_slugs(doc: &Doc) -> BTreeSet<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut slugs = BTreeSet::new();
    for heading in doc.headings() {
        let mut text = heading.text.clone();
        while let (Some(open), Some(middle)) = (text.find('['), text.find("](")) {
            let Some(close) = text[middle..].find(')') else {
                break;
            };
            if open > middle {
                break;
            }
            text = format!(
                "{}{}{}",
                &text[..open],
                &text[open + 1..middle],
                &text[middle + close + 1..]
            );
        }
        let slug: String = text
            .to_lowercase()
            .chars()
            .filter_map(|ch| match ch {
                ' ' => Some('-'),
                '-' | '_' => Some(ch),
                _ if ch.is_alphanumeric() => Some(ch),
                _ => None,
            })
            .collect();
        let count = seen.entry(slug.clone()).or_insert(0);
        slugs.insert(if *count == 0 {
            slug.clone()
        } else {
            format!("{slug}-{count}")
        });
        *count += 1;
    }
    slugs
}

/// Resolves a repository-relative link; `None` when it climbs out of the repository.
fn resolve_link(doc_rel: &str, target: &str) -> Option<String> {
    let base = if target.starts_with('/') {
        Path::new("")
    } else {
        Path::new(doc_rel).parent()?
    };
    let mut parts: Vec<String> = Vec::new();
    for component in base.join(target.trim_start_matches('/')).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

/// The router selects this gate for these paths. A link to any other path could go stale
/// through a change that does not run the gate, and turn the next docs PR red instead.
fn in_docs_lane(resolved: &str) -> bool {
    resolved.ends_with(".md")
        || [".agents", "docs"].iter().any(|dir| {
            resolved == *dir
                || resolved
                    .strip_prefix(dir)
                    .is_some_and(|rest| rest.starts_with('/'))
        })
}

fn check_links(root: &Path, doc: &Doc, report: &mut Report) {
    for (index, line) in doc.visible.iter().enumerate() {
        let Some(line) = line else { continue };
        for target in link_targets(line) {
            if target.contains("://") || target.starts_with("mailto:") || target.is_empty() {
                continue;
            }
            let (path, fragment) = target.split_once('#').unwrap_or((&target, ""));
            let resolved = if path.is_empty() {
                Some(doc.rel.clone())
            } else {
                resolve_link(&doc.rel, path)
            };
            let Some(resolved) = resolved else {
                report.add(
                    Rule::Link,
                    &doc.rel,
                    index + 1,
                    format!("link `{target}` leaves the repository"),
                );
                continue;
            };
            // docs/research is a nested private checkout that hosted CI does not have.
            if resolved == "docs/research" || resolved.starts_with("docs/research/") {
                report.private_links += 1;
                continue;
            }
            if !in_docs_lane(&resolved) {
                report.add(
                    Rule::LinkCode,
                    &doc.rel,
                    index + 1,
                    format!(
                        "link `{target}` leaves the documentation. Cite code as \
                         `` `{resolved}:<line>` ``, which resolves at the spec `Pin`"
                    ),
                );
                continue;
            }
            let on_disk = root.join(&resolved);
            if !on_disk.exists() {
                report.add(
                    Rule::Link,
                    &doc.rel,
                    index + 1,
                    format!(
                        "link target `{target}` does not exist (resolved `{resolved}`). Fix the \
                         path, or remove the link"
                    ),
                );
                continue;
            }
            if fragment.is_empty()
                || !resolved.ends_with(".md")
                || fragment.starts_with('L')
                    && fragment[1..].bytes().all(|byte| byte.is_ascii_digit())
            {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&on_disk) else {
                continue;
            };
            if !heading_slugs(&Doc::parse(&resolved, &raw)).contains(fragment) {
                report.add(
                    Rule::Anchor,
                    &doc.rel,
                    index + 1,
                    format!("anchor `#{fragment}` is not a heading in `{resolved}`"),
                );
            }
        }
    }
}

/// What the header block of an index or single-file spec must state.
fn check_header(doc: &Doc, report: &mut Report) -> Option<String> {
    let headings = doc.headings();
    let end = headings
        .iter()
        .find(|heading| heading.level == 2)
        .map_or(doc.lines.len(), |heading| heading.index);
    if !doc.lines.first().is_some_and(|line| line.starts_with("# ")) {
        report.add(
            Rule::Fields,
            &doc.rel,
            1,
            "the first line must be the `# ` title".to_owned(),
        );
    }
    let mut value = |label: &str, valid: &dyn Fn(&str) -> bool, expected: &str| {
        let found = find_field(doc, 0, end, label);
        match found {
            Some((_, value)) if valid(&value) => Some(value),
            Some((index, value)) => {
                report.add(
                    Rule::Fields,
                    &doc.rel,
                    index + 1,
                    format!("`{label}: {value}` must be {expected}"),
                );
                None
            }
            None => {
                report.add(
                    Rule::Fields,
                    &doc.rel,
                    1,
                    format!("the header must state `{label}:` ({expected})"),
                );
                None
            }
        }
    };
    value(
        "Status",
        &|text| STATUSES.contains(&text),
        "one of draft, approved, implementing, done",
    );
    value(
        "Tracker",
        &|text| {
            text.strip_prefix("KEL-")
                .or_else(|| text.strip_prefix("GH-"))
                .is_some_and(|rest| {
                    !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit())
                })
        },
        "KEL-<n> or GH-<n>",
    );
    let owner = value("Owner", &|text| !text.is_empty(), "the accountable owner");
    value(
        "Updated",
        &|text| {
            text.len() == 10
                && text.bytes().enumerate().all(|(at, byte)| {
                    if at == 4 || at == 7 {
                        byte == b'-'
                    } else {
                        byte.is_ascii_digit()
                    }
                })
        },
        "YYYY-MM-DD",
    );
    let pin = value("Pin", &is_full_sha, "a full 40-hex commit SHA");
    if let Some(owner) = owner {
        add_non_value(doc, 1, "Owner", &owner, non_value(&owner, true), report);
    }
    pin
}

fn add_non_value(
    doc: &Doc,
    line: usize,
    label: &str,
    value: &str,
    kind: Option<NonValue>,
    report: &mut Report,
) {
    let rule = match kind {
        Some(NonValue::Unknown) => Rule::FieldUnknown,
        Some(NonValue::Placeholder) => Rule::FieldPlaceholder,
        None => return,
    };
    report.add(
        rule,
        &doc.rel,
        line,
        format!(
            "`{label}: {value}` states nothing. Write the real content; a new spec cannot \
             defer a required field"
        ),
    );
}

/// `kel142-slug` carries `KEL-142`. The ID in the name and the tracker are one fact.
fn check_tracker(doc: &Doc, stem: &str, report: &mut Report) {
    let id = stem.split('-').next().unwrap_or("");
    let digits = id.trim_start_matches(|ch: char| ch.is_ascii_alphabetic());
    let expected = format!("{}-{digits}", id[..id.len() - digits.len()].to_uppercase());
    if let Some((index, value)) = find_field(doc, 0, doc.lines.len(), "Tracker")
        && value != expected
    {
        report.add(
            Rule::Fields,
            &doc.rel,
            index + 1,
            format!("`Tracker: {value}` must be `{expected}`, the ID in the spec name"),
        );
    }
}

fn is_full_sha(text: &str) -> bool {
    text.len() == 40
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn section_headings(doc: &Doc) -> Vec<(usize, String)> {
    doc.headings()
        .into_iter()
        .filter(|heading| heading.level == 2)
        .map(|heading| (heading.index, heading.text))
        .collect()
}

fn check_single_sections(doc: &Doc, report: &mut Report) {
    let headings = section_headings(doc);
    let found: Vec<Option<usize>> = headings
        .iter()
        .map(|(_, text)| canonical_section(text))
        .collect();
    let expected: Vec<Option<usize>> = (1..=SECTIONS.len()).map(Some).collect();
    if found != expected {
        let titles: Vec<&str> = headings.iter().map(|(_, text)| text.as_str()).collect();
        report.add(
            Rule::Sections,
            &doc.rel,
            1,
            format!(
                "a single-file spec needs exactly the ten `## n. Title` sections of `{TEMPLATE}` \
                 in order; found {titles:?}"
            ),
        );
    }
}

fn check_index_sections(doc: &Doc, report: &mut Report) {
    let expected = [
        format!("1. {}", SECTIONS[0]),
        PARTS_HEADING.to_owned(),
        BLOCKERS_HEADING.to_owned(),
        format!("8. {}", SECTIONS[7]),
    ];
    let found: Vec<String> = section_headings(doc)
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    if found != expected {
        report.add(
            Rule::Sections,
            &doc.rel,
            1,
            format!("an index has exactly the `##` sections {expected:?}; found {found:?}"),
        );
    }
}

/// Parts carry sections 2-7, 9 and 10 in index order; each must appear.
fn check_part_sections(parts: &[Doc], report: &mut Report) {
    let mut last = 0;
    let mut seen = BTreeSet::new();
    for doc in parts {
        for (index, text) in section_headings(doc) {
            match canonical_section(&text) {
                Some(number) if INDEX_SECTIONS.contains(&number) => report.add(
                    Rule::Sections,
                    &doc.rel,
                    index + 1,
                    format!("section {number} belongs to the index"),
                ),
                Some(number) if number < last => report.add(
                    Rule::Sections,
                    &doc.rel,
                    index + 1,
                    format!("section {number} follows section {last}; order the part table"),
                ),
                Some(number) => {
                    last = number;
                    seen.insert(number);
                }
                None => report.add(
                    Rule::Sections,
                    &doc.rel,
                    index + 1,
                    format!("`## {text}` is not a canonical `## n. Title[: subtitle]` section"),
                ),
            }
        }
    }
    for number in 1..=SECTIONS.len() {
        if !INDEX_SECTIONS.contains(&number)
            && !seen.contains(&number)
            && let Some(doc) = parts.first()
        {
            report.add(
                Rule::Sections,
                &doc.rel,
                1,
                format!("no part holds section {number}. {}", SECTIONS[number - 1]),
            );
        }
    }
}

fn check_part_header(doc: &Doc, report: &mut Report) {
    let mut lines = doc.lines.iter().filter(|line| !line.trim().is_empty());
    let title = lines.next().is_some_and(|line| line.starts_with("# "));
    let backlink = lines
        .next()
        .is_some_and(|line| line == "Part of: [index](index.md)");
    let owns = lines
        .next()
        .and_then(|line| line.strip_prefix("Owns: "))
        .is_some_and(|text| !text.trim().is_empty());
    if !(title && backlink && owns) {
        report.add(
            Rule::PartHeader,
            &doc.rel,
            1,
            "a part starts with `# <title>`, `Part of: [index](index.md)` and `Owns: <scope>`"
                .to_owned(),
        );
    }
}

/// Part file names from the index table, in table order.
fn part_table(index: &Doc, report: &mut Report) -> Vec<String> {
    let headings = index.headings();
    let Some(at) = headings
        .iter()
        .position(|heading| heading.level == 2 && heading.text == PARTS_HEADING)
    else {
        return Vec::new();
    };
    let start = headings[at].index + 1;
    let end = index.body_end(&headings, at);
    let rows: Vec<(usize, &str)> = (start..end)
        .filter_map(|line| {
            let text = index.visible[line].as_deref()?.trim();
            text.starts_with('|').then_some((line, text))
        })
        .collect();
    let mut parts = Vec::new();
    if rows.first().map(|(_, text)| *text) != Some(PART_TABLE_HEADER) {
        report.add(
            Rule::PartTable,
            &index.rel,
            start,
            format!("`## {PARTS_HEADING}` needs the table header `{PART_TABLE_HEADER}`"),
        );
        return parts;
    }
    for (line, text) in rows.iter().skip(2) {
        let cells: Vec<&str> = text.trim_matches('|').split('|').map(str::trim).collect();
        let target = link_targets(cells[0]);
        // A malformed row still names its part, so one bad cell reports one defect.
        parts.extend(target.first().cloned());
        if cells.len() != 3 || target.len() != 1 || cells[1].is_empty() || cells[2].is_empty() {
            report.add(
                Rule::PartTable,
                &index.rel,
                line + 1,
                "a part row is `| [name](name.md) | what it owns | when to load it |`".to_owned(),
            );
        }
    }
    parts
}

struct Definition {
    file: String,
    line: usize,
    open: bool,
}

/// Definitions and field checks across every document of one spec.
fn check_definitions(
    docs: &[&Doc],
    report: &mut Report,
) -> BTreeMap<(&'static str, u32), Definition> {
    let mut defined: BTreeMap<(&'static str, u32), Definition> = BTreeMap::new();
    for doc in docs.iter().copied() {
        let headings = doc.headings();
        let mut section = None;
        for (at, heading) in headings.iter().enumerate() {
            if heading.level == 2 {
                section = canonical_section(&heading.text);
            }
            let Some((kind, number, end)) = parse_id(&heading.text) else {
                continue;
            };
            let tail = &heading.text[end..];
            if !(tail.is_empty() || tail.starts_with(": ")) {
                continue;
            }
            let line = heading.index + 1;
            let body = doc.body_end(&headings, at);
            if let Some(previous) = defined.get(&(kind, number)) {
                report.add(
                    Rule::IdDuplicate,
                    &doc.rel,
                    line,
                    format!(
                        "{kind}-{number} is already defined at {}:{}",
                        previous.file, previous.line
                    ),
                );
                continue;
            }
            let owner = KINDS
                .iter()
                .find(|(name, _)| *name == kind)
                .map_or(0, |(_, section)| *section);
            if section != Some(owner) {
                report.add(
                    Rule::IdSection,
                    &doc.rel,
                    line,
                    format!("{kind}-{number} must be defined in section {owner}"),
                );
            }
            let mut required = |labels: &[&str], rule: Rule| {
                for label in labels {
                    match find_field(doc, heading.index + 1, body, label) {
                        Some((index, value)) if !value.is_empty() => {
                            // UNKNOWN stays legal for a blocker's owner: root AGENTS.md tells
                            // an agent to say so and name the first identification action.
                            let found = non_value(&value, CONTROL_FIELDS.contains(label))
                                .filter(|found| kind == "AC" || *found != NonValue::Unknown);
                            add_non_value(doc, index + 1, label, &value, found, report);
                        }
                        _ => report.add(
                            rule,
                            &doc.rel,
                            line,
                            format!("{kind}-{number} needs a non-empty `{label}:` field"),
                        ),
                    }
                }
            };
            match kind {
                "AC" => required(&AC_FIELDS, Rule::AcFields),
                "B" => required(&BLOCKER_FIELDS, Rule::BlockerFields),
                "E" => required(&EVIDENCE_FIELDS, Rule::EvidenceFields),
                "T" => required(&[TASK_BLOCKERS_FIELD], Rule::TaskFields),
                _ => {}
            }
            let status = (kind == "B")
                .then(|| find_field(doc, heading.index + 1, body, "Status"))
                .flatten();
            if let Some((index, value)) = &status
                && !matches!(value.as_str(), "open" | "closed")
            {
                report.add(
                    Rule::BlockerFields,
                    &doc.rel,
                    index + 1,
                    format!("B-{number} `Status:` must be open or closed"),
                );
            }
            if kind == "T"
                && let Some((index, value)) =
                    find_field(doc, heading.index + 1, body, TASK_BLOCKERS_FIELD)
                && value != "none"
                && id_tokens(&value).is_empty()
            {
                report.add(
                    Rule::TaskFields,
                    &doc.rel,
                    index + 1,
                    format!("T-{number} `{TASK_BLOCKERS_FIELD}:` must be none or stable IDs"),
                );
            }
            defined.insert(
                (kind, number),
                Definition {
                    file: doc.rel.clone(),
                    line,
                    open: status.is_some_and(|(_, value)| value == "open"),
                },
            );
        }
    }
    defined
}

/// A definition inside a fence, or a fence that never closes, escapes every ID rule.
fn check_fences(docs: &[&Doc], report: &mut Report) {
    for doc in docs {
        if let Some(index) = doc.unclosed_fence {
            report.add(
                Rule::Fence,
                &doc.rel,
                index + 1,
                "this fence never closes, so no rule reads the text after it. Close it".to_owned(),
            );
        }
        for (index, line) in doc.lines.iter().enumerate() {
            let heading = line.trim_start_matches('#');
            let defines = doc.visible[index].is_none()
                && heading.len() < line.len()
                && heading
                    .strip_prefix(' ')
                    .and_then(parse_id)
                    .is_some_and(|(_, _, end)| {
                        let tail = &heading[1 + end..];
                        tail.is_empty() || tail.starts_with(": ")
                    });
            if defines {
                report.add(
                    Rule::Fence,
                    &doc.rel,
                    index + 1,
                    "a fence holds an ID definition, which no rule reads. Move it out of the \
                     fence, or change the example's label"
                        .to_owned(),
                );
            }
        }
    }
}

fn check_references(
    docs: &[&Doc],
    defined: &BTreeMap<(&'static str, u32), Definition>,
    report: &mut Report,
) {
    for doc in docs {
        for (index, line) in doc.visible.iter().enumerate() {
            let Some(line) = line else { continue };
            for token in id_tokens(line) {
                if token.foreign {
                    continue;
                }
                if token.legacy_form {
                    report.add(
                        Rule::IdLegacyForm,
                        &doc.rel,
                        index + 1,
                        format!(
                            "write AC-{0}, not AC{0}; a bare form escapes reference checks",
                            token.number
                        ),
                    );
                } else if !defined.contains_key(&(token.kind, token.number)) {
                    report.add(
                        Rule::IdDangling,
                        &doc.rel,
                        index + 1,
                        format!(
                            "{}-{} has no definition in this spec. Define it, or qualify a \
                             foreign ID with its tracker (`#1311 AC-2`)",
                            token.kind, token.number
                        ),
                    );
                }
            }
        }
    }
}

/// The index lists exactly the open blockers.
fn check_open_blockers(
    index: &Doc,
    defined: &BTreeMap<(&'static str, u32), Definition>,
    report: &mut Report,
) {
    let headings = index.headings();
    let Some(at) = headings
        .iter()
        .position(|heading| heading.level == 2 && heading.text == BLOCKERS_HEADING)
    else {
        return;
    };
    let listed: BTreeSet<u32> = (headings[at].index + 1..index.body_end(&headings, at))
        .filter_map(|line| index.visible[line].as_deref())
        .flat_map(id_tokens)
        .filter(|token| token.kind == "B" && !token.foreign)
        .map(|token| token.number)
        .collect();
    let open: BTreeSet<u32> = defined
        .iter()
        .filter(|((kind, _), definition)| *kind == "B" && definition.open)
        .map(|((_, number), _)| *number)
        .collect();
    if listed != open {
        report.add(
            Rule::OpenBlockers,
            &index.rel,
            headings[at].index + 1,
            format!(
                "`## {BLOCKERS_HEADING}` lists {listed:?}; the open blockers are {open:?}. List \
                 exactly the open `B-n`, or write none"
            ),
        );
    }
}

/// `path:line[-line][:column][@sha]` in one inline-code span. A citation names a
/// repository file, so an image tag, a domain or an absolute path is not one.
fn parse_citation(span: &str) -> Option<(&str, usize, Option<&str>)> {
    let (location, sha) = match span.split_once('@') {
        Some((location, sha)) => (location, Some(sha)),
        None => (span, None),
    };
    let (path, lines) = location.rsplit_once(':')?;
    let (path, lines) = match path.rsplit_once(':') {
        Some((inner, cited)) if cited.parse::<usize>().is_ok() => (inner, cited),
        _ => (path, lines),
    };
    let last = lines.rsplit('-').next()?;
    let line: usize = last.parse().ok()?;
    let valid_lines = lines.split('-').all(|part| part.parse::<usize>().is_ok());
    let first = path.split('/').next()?;
    let file = path.rsplit('/').next()?;
    let plausible = path.contains('/')
        && !path.contains(':')
        && !path.starts_with('/')
        && (first.starts_with('.') || !first.contains('.'))
        && file.contains('.');
    (valid_lines && plausible).then_some((path, line, sha))
}

fn check_citations(docs: &[&Doc], pin: &str, history: &dyn History, report: &mut Report) {
    for doc in docs {
        for (index, line) in doc.visible.iter().enumerate() {
            let Some(line) = line else { continue };
            for span in line.split('`').skip(1).step_by(2) {
                let Some((path, cited, sha)) = parse_citation(span) else {
                    continue;
                };
                let commit = match sha {
                    Some(sha) if !is_full_sha(sha) => {
                        report.add(
                            Rule::CiteSha,
                            &doc.rel,
                            index + 1,
                            format!("`{span}` must name a full 40-hex SHA"),
                        );
                        continue;
                    }
                    Some(sha) if !history.has_commit(sha) => {
                        // A foreign PR head may be absent from this clone.
                        report.citations_unverifiable += 1;
                        continue;
                    }
                    Some(sha) => sha,
                    None => pin,
                };
                match history.lines_at(commit, path) {
                    None => report.add(
                        Rule::CitePath,
                        &doc.rel,
                        index + 1,
                        format!(
                            "`{path}` does not exist at {commit}. Fix the path, or add \
                             `@<full-sha>` for a commit that holds it"
                        ),
                    ),
                    Some(count) if cited == 0 || cited > count => report.add(
                        Rule::CiteLine,
                        &doc.rel,
                        index + 1,
                        format!(
                            "`{span}` is past the {count} lines at {commit}. Cite a line that \
                             exists at that commit"
                        ),
                    ),
                    Some(_) => report.citations_verified += 1,
                }
            }
        }
    }
}

/// Everything except size, for one spec. The pin is checked only when `history` is given.
fn check_structure(
    index: &Doc,
    parts: &[Doc],
    directory: bool,
    history: Option<&dyn History>,
    report: &mut Report,
) {
    let pin = check_header(index, report);
    if directory {
        check_index_sections(index, report);
        check_part_sections(parts, report);
    } else {
        check_single_sections(index, report);
    }
    let mut docs: Vec<&Doc> = vec![index];
    docs.extend(parts);
    let defined = check_definitions(&docs, report);
    check_references(&docs, &defined, report);
    check_fences(&docs, report);
    for kind in ["AC", "T"] {
        if !defined
            .keys()
            .any(|(defined_kind, _)| *defined_kind == kind)
        {
            report.add(
                Rule::IdRequired,
                &index.rel,
                1,
                format!("a spec defines at least one `{kind}-n`; none has a heading here"),
            );
        }
    }
    if directory {
        check_open_blockers(index, &defined, report);
    }
    if let (Some(pin), Some(history)) = (pin, history) {
        if history.is_reachable(&pin) {
            check_citations(&docs, &pin, history, report);
        } else {
            report.add(
                Rule::PinMissing,
                &index.rel,
                1,
                format!(
                    "`Pin: {pin}` is not in this branch's history, so a later clone can lose \
                     it. Pin the output of `git merge-base origin/main HEAD`"
                ),
            );
        }
    }
}

/// Measured size of one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Size {
    lines: usize,
    bytes: usize,
    /// Characters past the line cap, summed, so a new or longer long line always counts.
    overflow: usize,
}

fn measure(doc: &Doc) -> Size {
    Size {
        lines: doc.lines.len(),
        bytes: doc.bytes,
        overflow: doc
            .lines
            .iter()
            .map(|line| line.chars().count().saturating_sub(MAX_LINE_CHARS))
            .sum(),
    }
}

fn check_size(doc: &Doc, report: &mut Report) {
    let size = measure(doc);
    if size.lines > MAX_LINES {
        report.add(
            Rule::SizeLines,
            &doc.rel,
            MAX_LINES + 1,
            format!(
                "{} lines; the cap is {MAX_LINES}. Split it into the directory layout of \
                 `.agents/specs.md`",
                size.lines
            ),
        );
    }
    if size.bytes > MAX_BYTES {
        report.add(
            Rule::SizeBytes,
            &doc.rel,
            1,
            format!(
                "{} bytes; the cap is {MAX_BYTES}. Split it into the directory layout of \
                 `.agents/specs.md`",
                size.bytes
            ),
        );
    }
    for (index, line) in doc.lines.iter().enumerate() {
        let chars = line.chars().count();
        if chars > MAX_LINE_CHARS {
            report.add(
                Rule::SizeLineLength,
                &doc.rel,
                index + 1,
                format!(
                    "{chars} characters; the cap is {MAX_LINE_CHARS}. Move table-cell prose \
                     under a heading"
                ),
            );
        }
    }
}

/// The structure column of a baseline row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Structure {
    Exempt,
    Conforms,
    /// Conforms except for this many `UNKNOWN` required fields.
    Migrated(usize),
}

impl Structure {
    fn parse(cell: &str) -> Option<Structure> {
        match cell {
            "exempt" => Some(Structure::Exempt),
            "ok" => Some(Structure::Conforms),
            _ => cell
                .strip_prefix("migrated:")?
                .parse()
                .ok()
                .filter(|count| *count > 0)
                .map(Structure::Migrated),
        }
    }

    fn cell(self) -> String {
        match self {
            Structure::Exempt => "exempt".to_owned(),
            Structure::Conforms => "ok".to_owned(),
            Structure::Migrated(count) => format!("migrated:{count}"),
        }
    }
}

struct Row {
    line: usize,
    lines: Option<usize>,
    bytes: Option<usize>,
    overflow: Option<usize>,
    structure: Structure,
}

fn parse_metric(text: &str) -> Option<Option<usize>> {
    if text == "-" {
        return Some(None);
    }
    let value: usize = text.parse().ok()?;
    (value > 0).then_some(Some(value))
}

fn parse_baseline(rel: &str, text: &str, report: &mut Report) -> BTreeMap<String, Row> {
    let mut rows = BTreeMap::new();
    let mut last: Option<String> = None;
    let normalized = text.replace("\r\n", "\n");
    let mut lines = normalized.lines().enumerate();
    if lines.next().map(|(_, line)| line) != Some(BASELINE_HEADER) {
        report.add(
            Rule::BaselineFormat,
            rel,
            1,
            format!("the first line must be `{BASELINE_HEADER}`"),
        );
    }
    for (index, line) in lines {
        let cells: Vec<&str> = line.split('\t').collect();
        let parsed = match cells.as_slice() {
            [path, lines, bytes, overflow, structure] => parse_metric(lines)
                .zip(parse_metric(bytes))
                .zip(parse_metric(overflow))
                .zip(Structure::parse(structure))
                .map(|(((lines, bytes), overflow), structure)| {
                    (
                        (*path).to_owned(),
                        Row {
                            line: index + 1,
                            lines,
                            bytes,
                            overflow,
                            structure,
                        },
                    )
                }),
            _ => None,
        };
        let Some((path, row)) = parsed else {
            report.add(
                Rule::BaselineFormat,
                rel,
                index + 1,
                "a row is `path<TAB>lines<TAB>bytes<TAB>line_overflow<TAB>structure`, with `-` \
                 for a value inside its cap and structure `exempt`, `ok` or `migrated:<n>`"
                    .to_owned(),
            );
            continue;
        };
        if last.as_ref().is_some_and(|previous| *previous >= path) {
            report.add(
                Rule::BaselineFormat,
                rel,
                index + 1,
                format!("`{path}` breaks sorted unique order"),
            );
        }
        last = Some(path.clone());
        rows.insert(path, row);
    }
    rows
}

fn expected_row(path: &str, size: Size, structure: Structure) -> String {
    let metric = |value: usize, over: bool| {
        if over {
            value.to_string()
        } else {
            "-".to_owned()
        }
    };
    format!(
        "{path}\t{}\t{}\t{}\t{}",
        metric(size.lines, size.lines > MAX_LINES),
        metric(size.bytes, size.bytes > MAX_BYTES),
        metric(size.overflow, size.overflow > 0),
        structure.cell()
    )
}

/// Applies one ratchet row to one landed file.
fn check_row(rel_baseline: &str, path: &str, row: &Row, doc: &Doc, report: &mut Report) {
    let size = measure(doc);
    let metrics = [
        ("lines", size.lines, MAX_LINES, row.lines),
        ("bytes", size.bytes, MAX_BYTES, row.bytes),
        ("line_overflow", size.overflow, 0, row.overflow),
    ];
    let mut replacement = false;
    for (name, measured, cap, recorded) in metrics {
        match (measured > cap, recorded) {
            (true, None) => report.add(
                Rule::BaselineGrew,
                path,
                1,
                format!(
                    "{name} is {measured}, over cap {cap}, and the row allows none. Cut the \
                     file back, or split it into the directory layout of `.agents/specs.md`"
                ),
            ),
            (true, Some(value)) if measured > value => report.add(
                Rule::BaselineGrew,
                path,
                1,
                format!(
                    "{name} grew from {value} to {measured}; a listed file may only shrink. Cut \
                     as much as you added, or split it into the directory layout of \
                     `.agents/specs.md`"
                ),
            ),
            (true, Some(value)) if measured < value => replacement = true,
            (false, Some(_)) => replacement = true,
            _ => {}
        }
    }
    if replacement {
        report.add(
            Rule::BaselineStale,
            rel_baseline,
            row.line,
            format!(
                "`{path}` shrank; the row must click down to `{}`",
                expected_row(path, size, row.structure)
            ),
        );
    }
}

/// A row may not record more than its file held at the seal, whatever the file holds now.
/// Without this, editing a file and its row together would grow the file.
fn check_seal(rel_baseline: &str, path: &str, row: &Row, sealed: &Doc, report: &mut Report) {
    let size = measure(sealed);
    let metrics = [
        ("lines", size.lines, row.lines),
        ("bytes", size.bytes, row.bytes),
        ("line_overflow", size.overflow, row.overflow),
    ];
    for (name, at_seal, recorded) in metrics {
        if let Some(value) = recorded
            && value > at_seal
        {
            report.add(
                Rule::BaselineRaised,
                rel_baseline,
                row.line,
                format!(
                    "`{path}` {name} is {value} in the row and {at_seal} at the seal. A row never \
                     rises: cut the file to {at_seal}, or split it into the directory layout of \
                     `.agents/specs.md`"
                ),
            );
        }
    }
}

/// Settles a spec's `UNKNOWN` fields against the count its `migrated:<n>` row allows.
fn settle_unknowns(
    rel_baseline: &str,
    path: &str,
    row: &Row,
    structure: &mut Report,
    report: &mut Report,
) {
    let Structure::Migrated(allowed) = row.structure else {
        return;
    };
    let before = structure.findings.len();
    structure
        .findings
        .retain(|finding| finding.rule != Rule::FieldUnknown);
    let found = before - structure.findings.len();
    match found.cmp(&allowed) {
        Ordering::Equal => {}
        Ordering::Greater => report.add(
            Rule::BaselineGrew,
            path,
            1,
            format!(
                "{found} UNKNOWN required fields; the row allows {allowed}. Fill the new field; \
                 a migrated count may only shrink"
            ),
        ),
        Ordering::Less if found == 0 => report.add(
            Rule::BaselineStale,
            rel_baseline,
            row.line,
            format!("`{path}` has no UNKNOWN required field; remove its row"),
        ),
        Ordering::Less => report.add(
            Rule::BaselineStale,
            rel_baseline,
            row.line,
            format!(
                "`{path}` has {found} UNKNOWN required fields; the row must say migrated:{found}"
            ),
        ),
    }
}

fn read_doc(root: &Path, rel: &str) -> Result<Doc, String> {
    let raw = fs::read_to_string(root.join(rel))
        .map_err(|error| format!("SPEC-STRUCTURE: cannot read `{rel}`: {error}"))?;
    Ok(Doc::parse(rel, &raw))
}

/// A directory name or file stem: `<kel|gh><n>-<kebab-slug>`.
fn is_spec_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("kel").or_else(|| name.strip_prefix("gh")) else {
        return false;
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0
        && rest[digits..].strip_prefix('-').is_some_and(|slug| {
            !slug.is_empty()
                && !slug.ends_with('-')
                && !slug.contains("--")
                && slug
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn check_directory(
    root: &Path,
    rel: &str,
    rows: &BTreeMap<String, Row>,
    history: &dyn History,
    report: &mut Report,
) -> Result<(), String> {
    let mut names = BTreeSet::new();
    let entries = fs::read_dir(root.join(rel))
        .map_err(|error| format!("SPEC-STRUCTURE: cannot read `{rel}`: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("SPEC-STRUCTURE: `{rel}`: {error}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry
            .file_type()
            .map_err(|error| format!("SPEC-STRUCTURE: `{rel}/{name}`: {error}"))?;
        if kind.is_file() && name.ends_with(".md") {
            names.insert(name);
        } else {
            report.add(
                Rule::Entry,
                &format!("{rel}/{name}"),
                1,
                "a spec directory holds only flat `.md` parts".to_owned(),
            );
        }
    }
    if !names.remove(INDEX_FILE) {
        report.add(
            Rule::IndexMissing,
            rel,
            1,
            format!("a spec directory needs `{INDEX_FILE}`"),
        );
        return Ok(());
    }
    let index = read_doc(root, &format!("{rel}/{INDEX_FILE}"))?;
    let listed = part_table(&index, report);
    let mut parts = Vec::new();
    let mut seen = BTreeSet::new();
    for part in &listed {
        if !seen.insert(part.clone()) {
            report.add(
                Rule::PartTable,
                &index.rel,
                1,
                format!("`{part}` is listed twice"),
            );
        } else if names.contains(part) {
            parts.push(read_doc(root, &format!("{rel}/{part}"))?);
        } else {
            report.add(
                Rule::PartMissing,
                &index.rel,
                1,
                format!("the index lists `{part}`, which does not exist"),
            );
        }
    }
    for name in names.iter().filter(|name| !seen.contains(*name)) {
        report.add(
            Rule::PartUnlisted,
            &format!("{rel}/{name}"),
            1,
            "an orphan part: list it in the index part table".to_owned(),
        );
    }
    let size = measure(&index);
    if size.lines > INDEX_MAX_LINES || size.bytes > INDEX_MAX_BYTES {
        report.add(
            Rule::IndexSize,
            &index.rel,
            1,
            format!(
                "{} lines, {} bytes; an index stays a map within {INDEX_MAX_LINES} lines and \
                 {INDEX_MAX_BYTES} bytes",
                size.lines, size.bytes
            ),
        );
    }
    check_size(&index, report);
    check_links(root, &index, report);
    for part in &parts {
        check_part_header(part, report);
        check_size(part, report);
        check_links(root, part, report);
    }
    let mut structure = Report::default();
    check_structure(&index, &parts, true, Some(history), &mut structure);
    let name = rel.rsplit('/').next().unwrap_or(rel);
    if is_spec_name(name) {
        check_tracker(&index, name, &mut structure);
    }
    // A migrated spec keeps the row of the flat file the seal holds.
    if let Some(row) = rows.get(&format!("{rel}.md")) {
        let baseline_rel = format!("{SPEC_ROOT}/{BASELINE_FILE}");
        settle_unknowns(&baseline_rel, rel, row, &mut structure, report);
    }
    report.absorb(structure);
    Ok(())
}

/// The template's first `markdown` fence, which must itself pass the standard.
fn check_template(root: &Path, report: &mut Report) -> Result<(), String> {
    let doc = read_doc(root, TEMPLATE)?;
    let mut body = String::new();
    let mut inside = false;
    for line in &doc.lines {
        if inside && line.trim() == "```" {
            break;
        }
        if inside {
            body.push_str(line);
            body.push('\n');
        }
        inside = inside || line.trim() == "```markdown";
    }
    if body.is_empty() {
        report.add(
            Rule::Sections,
            TEMPLATE,
            1,
            "the template needs one ```markdown fence".to_owned(),
        );
        return Ok(());
    }
    let mut structure = Report::default();
    let copy_source = Doc::parse(TEMPLATE, &body);
    check_structure(&copy_source, &[], false, None, &mut structure);
    // The fence is the copy source, so its slots are meant to be unfilled.
    structure
        .findings
        .retain(|finding| finding.rule != Rule::FieldPlaceholder);
    report.absorb(structure);
    Ok(())
}

/// A spec is the standard's business wherever it lands: outside `docs/specs` no other rule
/// runs on it. A sealed spec's file name marks a move out of the ratchet.
fn check_outside(
    root: &Path,
    sealed: &BTreeSet<String>,
    history: &dyn History,
    report: &mut Report,
) -> Result<(), String> {
    let base_name = |path: &str| path.rsplit('/').next().unwrap_or(path).to_owned();
    let sealed_names: BTreeSet<String> = sealed.iter().map(|path| base_name(path)).collect();
    for rel in history.tracked_markdown()? {
        if rel.starts_with(&format!("{SPEC_ROOT}/")) {
            continue;
        }
        if sealed_names.contains(&base_name(&rel)) {
            report.add(
                Rule::Outside,
                &rel,
                1,
                format!("a landed spec has this name; move it back to `{SPEC_ROOT}`"),
            );
            continue;
        }
        let Ok(raw) = fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        let doc = Doc::parse(&rel, &raw);
        let marked = doc
            .headings()
            .into_iter()
            .find(|heading| heading.text.to_lowercase().contains(SPEC_SIGNATURE));
        if let Some(heading) = marked {
            report.add(
                Rule::Outside,
                &rel,
                heading.index + 1,
                format!(
                    "`{}` marks a spec, and a spec lives in `{SPEC_ROOT}`. Move the file, or \
                     rename the heading if this is not a spec",
                    heading.text
                ),
            );
        }
    }
    Ok(())
}

fn check_tree(root: &Path, config: &Config, history: &dyn History) -> Result<Report, String> {
    let mut report = Report::default();
    let baseline_rel = format!("{SPEC_ROOT}/{BASELINE_FILE}");
    let baseline_text = fs::read_to_string(root.join(&baseline_rel))
        .map_err(|error| format!("SPEC-STRUCTURE: cannot read `{baseline_rel}`: {error}"))?;
    let rows = parse_baseline(&baseline_rel, &baseline_text, &mut report);

    if rows.len() != config.rows {
        report.add(
            Rule::BaselineCount,
            &baseline_rel,
            1,
            format!(
                "{} rows; the fixed count is {}. When a row drains, lower BASELINE_ROWS in \
                 tools/spec_structure.rs. A new file may not enter the baseline",
                rows.len(),
                config.rows
            ),
        );
    }
    let sealed = history.paths_at(config.seal, SPEC_ROOT)?;

    let mut units = Vec::new();
    let entries = fs::read_dir(root.join(SPEC_ROOT))
        .map_err(|error| format!("SPEC-STRUCTURE: cannot read `{SPEC_ROOT}`: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("SPEC-STRUCTURE: `{SPEC_ROOT}`: {error}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry
            .file_type()
            .map_err(|error| format!("SPEC-STRUCTURE: `{name}`: {error}"))?;
        units.push((name, kind));
    }
    units.sort_by(|left, right| left.0.cmp(&right.0));

    for (name, kind) in &units {
        let rel = format!("{SPEC_ROOT}/{name}");
        if rel == baseline_rel {
            continue;
        }
        if kind.is_dir() {
            report.specs += 1;
            if !is_spec_name(name) {
                report.add(
                    Rule::Name,
                    &rel,
                    1,
                    "a spec directory is named `<kel|gh><n>-<kebab-slug>`".to_owned(),
                );
            }
            check_directory(root, &rel, &rows, history, &mut report)?;
        } else if kind.is_file() && name.ends_with(".md") {
            report.specs += 1;
            let doc = read_doc(root, &rel)?;
            check_links(root, &doc, &mut report);
            if let Some(row) = rows.get(&rel) {
                check_row(&baseline_rel, &rel, row, &doc, &mut report);
                let mut structure = Report::default();
                check_structure(&doc, &[], false, Some(history), &mut structure);
                if row.structure == Structure::Exempt {
                    if structure.findings.is_empty() {
                        report.add(
                            Rule::BaselineStale,
                            &baseline_rel,
                            row.line,
                            format!("`{rel}` now conforms; its structure column must be `ok`"),
                        );
                    } else {
                        report.exempt += 1;
                    }
                } else {
                    settle_unknowns(&baseline_rel, &rel, row, &mut structure, &mut report);
                    report.absorb(structure);
                }
                let size = measure(&doc);
                if row.structure == Structure::Conforms
                    && size.lines <= MAX_LINES
                    && size.bytes <= MAX_BYTES
                    && size.overflow == 0
                {
                    report.add(
                        Rule::BaselineStale,
                        &baseline_rel,
                        row.line,
                        format!("`{rel}` complies; remove its row"),
                    );
                }
            } else {
                let stem = name.trim_end_matches(".md");
                if is_spec_name(stem) {
                    check_tracker(&doc, stem, &mut report);
                } else {
                    report.add(
                        Rule::Name,
                        &rel,
                        1,
                        "a spec file is named `<kel|gh><n>-<kebab-slug>.md`".to_owned(),
                    );
                }
                check_size(&doc, &mut report);
                check_structure(&doc, &[], false, Some(history), &mut report);
            }
        } else {
            report.add(
                Rule::Entry,
                &rel,
                1,
                format!("`{SPEC_ROOT}` holds only specs and `{BASELINE_FILE}`"),
            );
        }
    }

    for (path, row) in &rows {
        let migrated = matches!(row.structure, Structure::Migrated(_))
            && root
                .join(path.trim_end_matches(".md"))
                .join(INDEX_FILE)
                .is_file();
        if !root.join(path).is_file() && !migrated {
            report.add(
                Rule::BaselineStale,
                &baseline_rel,
                row.line,
                format!("`{path}` no longer exists; remove its row"),
            );
        }
        if !sealed.contains(path) {
            report.add(
                Rule::BaselineNew,
                &baseline_rel,
                row.line,
                format!(
                    "`{path}` is not in seal {}; a new file may not enter. Make it comply, and \
                     remove the row",
                    config.seal
                ),
            );
        } else if let Some(text) = history.text_at(config.seal, path) {
            check_seal(
                &baseline_rel,
                path,
                row,
                &Doc::parse(path, &text),
                &mut report,
            );
        }
    }
    check_outside(root, &sealed, history, &mut report)?;
    check_template(root, &mut report)?;
    report
        .findings
        .sort_by(|left, right| (&left.file, left.line).cmp(&(&right.file, right.line)));
    Ok(report)
}

fn check(root: &Path) -> Result<String, String> {
    let history = GitHistory { root };
    let report = check_tree(root, &PRODUCTION, &history)?;
    if !report.findings.is_empty() {
        let mut text = String::new();
        for finding in &report.findings {
            text.push_str(&format!(
                "SPEC-STRUCTURE: {}: {}:{}: {}\n",
                finding.rule.code(),
                finding.file,
                finding.line,
                finding.detail
            ));
        }
        text.push_str(&format!(
            "{} spec defect(s). Standard: .agents/specs.md.",
            report.findings.len()
        ));
        return Err(text);
    }
    Ok(format!(
        "spec-structure ok: {} spec(s), {} structure-exempt through the baseline ({} rows); \
         {} pin citation(s) verified, {} @sha citation(s) not verifiable in this clone, \
         {} private-research link(s) not checked",
        report.specs,
        report.exempt,
        BASELINE_ROWS,
        report.citations_verified,
        report.citations_unverifiable,
        report.private_links
    ))
}

/// Prints the row each flat spec needs today, for a drain or a shrink.
fn baseline_rows(root: &Path) -> Result<String, String> {
    let mut paths: Vec<PathBuf> = fs::read_dir(root.join(SPEC_ROOT))
        .map_err(|error| format!("SPEC-STRUCTURE: cannot read `{SPEC_ROOT}`: {error}"))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();
    let history = GitHistory { root };
    let mut out = format!("{BASELINE_HEADER}\n");
    for path in paths {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let rel = format!("{SPEC_ROOT}/{}", name.unwrap_or_default());
        let doc = read_doc(root, &rel)?;
        let mut structure = Report::default();
        check_structure(&doc, &[], false, Some(&history), &mut structure);
        let size = measure(&doc);
        let exempt = !structure.findings.is_empty();
        if exempt || size.lines > MAX_LINES || size.bytes > MAX_BYTES || size.overflow > 0 {
            let cell = if exempt {
                Structure::Exempt
            } else {
                Structure::Conforms
            };
            out.push_str(&expected_row(&rel, size, cell));
            out.push('\n');
        }
    }
    Ok(out)
}

fn run_cli() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next();
    let root = args.next().unwrap_or_else(|| ".".to_owned());
    match command.as_deref() {
        Some("check") => {
            println!("{}", check(Path::new(&root))?);
            Ok(())
        }
        Some("baseline-rows") => {
            print!("{}", baseline_rows(Path::new(&root))?);
            Ok(())
        }
        other => Err(format!(
            "SPEC-STRUCTURE: expected `check <root>` or `baseline-rows <root>`, got {other:?}."
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
#[path = "spec_structure_tests.rs"]
mod tests;
