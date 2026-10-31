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
//! records each one's over-cap values, a row must equal the measured value, must not
//! exceed that file's measurement at `BASELINE_SEAL`, must not exceed its own value at the
//! comparison base, and leaves when its file complies. A sealed spec that moves to the
//! standard before its gaps are filled keeps a `migrated:<slots>` row naming each `UNKNOWN`
//! required field, which is the only way one passes; a new spec cannot hold one.
//! `BASELINE_SEAL`, `BASELINE_ROWS` and the comparison base stop a new file from entering.
//! Raising the count, or moving the seal, is a visible edit under code-owner review; no
//! in-repository check can forbid a pull request that rewrites both the rule and its data.

#[path = "markdown_contract.rs"]
#[allow(
    dead_code,
    reason = "shared contract module; this checker uses only its visibility primitives"
)]
mod markdown_contract;

#[path = "git_base_contract.rs"]
mod git_base_contract;

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path};
use std::process::{Command, Output};

use git_base_contract::{BASE_ENV, BaseError, base_ref, merge_base};
use markdown_contract::{
    CodeSpan, code_span_at, fence_marker, inline_code_spans, without_inline_code,
    without_struck_text,
};

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
const HEADER_FIELDS: [&str; 5] = ["Status", "Tracker", "Owner", "Updated", "Pin"];
/// Header fields nothing else may state. `Status` and `Owner` also label a blocker.
const HEADER_ONLY_FIELDS: [&str; 3] = ["Tracker", "Updated", "Pin"];
const PART_FIELDS: [&str; 2] = ["Part of", "Owns"];
/// Marks around a field label or an ID. They change how it looks, not which one it is.
const LABEL_MARKS: [char; 3] = ['*', '_', '`'];
/// Characters a reader cannot see, so they cannot make a second label.
const INVISIBLE: [char; 6] = [
    '\u{ad}', '\u{200b}', '\u{200c}', '\u{200d}', '\u{2060}', '\u{feff}',
];
/// AC fields where a bare `none` states nothing. `Given: none` is a real statement.
const CONTROL_FIELDS: [&str; 5] = [
    "Then",
    "Negative control",
    "Single fault",
    "At-risk precondition",
    "Expected RED",
];
/// Whole values that fill a required field without saying anything. A value with no
/// letter or digit is one too, whatever its punctuation.
const NON_VALUES: [&str; 6] = ["tbd", "tba", "todo", "n/a", "na", "pending"];
/// A heading with this text marks a spec, wherever the file sits.
const SPEC_SIGNATURE: &str = "acceptance criteria";
/// File extensions GitHub renders as Markdown, so a spec can land under any of them.
const MARKDOWN_EXTENSIONS: [&str; 8] = [
    "md", "markdown", "mdown", "mdwn", "mdx", "mkd", "mkdn", "mkdown",
];
/// The one header field that can stay `UNKNOWN`, as a `migrated:` slot.
const HEADER_OWNER_SLOT: &str = "header.Owner";
/// CommonMark block-level tag names: an HTML block of these ends at a blank line.
const HTML_BLOCK_TAGS: [&str; 62] = [
    "address",
    "article",
    "aside",
    "base",
    "basefont",
    "blockquote",
    "body",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "iframe",
    "legend",
    "li",
    "link",
    "main",
    "menu",
    "menuitem",
    "meta",
    "nav",
    "noframes",
    "ol",
    "optgroup",
    "option",
    "p",
    "param",
    "search",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
];
/// HTML elements whose raw content runs across blank lines.
const RAW_HTML_ELEMENTS: [&str; 4] = ["pre", "script", "style", "textarea"];
/// Any of these ends such a block, whichever element opened it.
const RAW_HTML_ENDS: [&str; 4] = ["</pre>", "</script>", "</style>", "</textarea>"];

/// List nesting past this reads as text, so a line of markers stays linear.
const MAX_LIST_DEPTH: usize = 32;

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
    Comment,
    Collision,
    Unreadable,
    AcFields,
    TaskFields,
    BlockerFields,
    EvidenceFields,
    FieldUnknown,
    FieldPlaceholder,
    FieldDuplicate,
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
            Rule::Comment => "COMMENT",
            Rule::Collision => "COLLISION",
            Rule::Unreadable => "UNREADABLE",
            Rule::AcFields => "AC-FIELDS",
            Rule::TaskFields => "TASK-FIELDS",
            Rule::BlockerFields => "BLOCKER-FIELDS",
            Rule::EvidenceFields => "EVIDENCE-FIELDS",
            Rule::FieldUnknown => "FIELD-UNKNOWN",
            Rule::FieldPlaceholder => "FIELD-PLACEHOLDER",
            Rule::FieldDuplicate => "FIELD-DUPLICATE",
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
    /// The `id.field` a non-value occupies, which a `migrated:` row lists.
    slot: Option<String>,
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
    /// Tracked Markdown a local deletion removed from the worktree, so no rule read it.
    absent_markdown: usize,
    /// What the ratchet compared with: the base commit, or why it had no baseline.
    ratchet: String,
}

impl Report {
    fn add(&mut self, rule: Rule, file: &str, line: usize, detail: String) {
        self.findings.push(Finding {
            rule,
            file: file.to_owned(),
            line,
            detail,
            slot: None,
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
    /// Every tracked path. An untracked private checkout is not repository text.
    fn tracked_files(&self) -> Result<Vec<Tracked>, String>;
    /// The baseline at the comparison base, which the working baseline may not exceed.
    fn previous_baseline(&self, path: &str) -> Result<Previous, String>;

    fn lines_at(&self, commit: &str, path: &str) -> Option<usize> {
        Some(self.text_at(commit, path)?.lines().count())
    }
}

/// One path in the Git index.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Tracked {
    path: String,
    /// Mode 120000. A Windows checkout may hold this as a text file, so the index decides.
    symlink: bool,
}

/// The baseline text at the comparison base.
struct Previous {
    commit: String,
    /// `None` when that commit predates the baseline, so the seal is the only bound.
    text: Option<String>,
}

struct GitHistory<'a> {
    root: &'a Path,
    base_ref: String,
}

impl GitHistory<'_> {
    fn git_output(&self, args: &[&str]) -> Result<Output, String> {
        let mut command = Command::new("git");
        command.arg("-C").arg(self.root).args(args);
        for name in GIT_ENV {
            command.env_remove(name);
        }
        command
            .output()
            .map_err(|error| format!("cannot start git: {error}"))
    }

    /// Standard output, or Git's own explanation of why it failed.
    fn git(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        let output = self.git_output(args)?;
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

/// `git ls-files -s -z` output. A lossy decode would turn a non-UTF-8 name into a path no
/// read finds, so such a name is an error, never an absent file.
fn parse_tracked(stdout: &[u8]) -> Result<Vec<Tracked>, String> {
    let mut tracked = Vec::new();
    for entry in stdout.split(|byte| *byte == 0) {
        let text = std::str::from_utf8(entry).map_err(|_| {
            format!(
                "SPEC-STRUCTURE: tracked path `{}` is not UTF-8, so no rule can read it. Rename it",
                String::from_utf8_lossy(entry)
            )
        })?;
        if let Some((stage, path)) = text.split_once('\t') {
            tracked.push(Tracked {
                path: path.to_owned(),
                symlink: stage.starts_with("120000 "),
            });
        }
    }
    Ok(tracked)
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

    fn tracked_files(&self) -> Result<Vec<Tracked>, String> {
        let stdout = self
            .git(&["ls-files", "-s", "-z"])
            .map_err(|detail| format!("SPEC-STRUCTURE: cannot list tracked files: {detail}"))?;
        parse_tracked(&stdout)
    }

    fn previous_baseline(&self, path: &str) -> Result<Previous, String> {
        let commit =
            merge_base(&|args| self.git_output(args), &self.base_ref).map_err(|error| {
                let detail = match error {
                    BaseError::Git(detail) => detail,
                    BaseError::Unresolved(detail) => detail.to_owned(),
                };
                format!(
                    "SPEC-STRUCTURE: cannot resolve the ratchet comparison base `{}` ({detail}). \
                     {} Or set {BASE_ENV} to an available commit. The ratchet never passes \
                     without a base.",
                    self.base_ref,
                    seal_fix(self.is_shallow())
                )
            })?;
        let unreadable =
            |detail: String| format!("SPEC-STRUCTURE: cannot read `{path}` at {commit}: {detail}");
        let listed = self
            .git(&["ls-tree", "--name-only", &commit, "--", path])
            .map_err(&unreadable)?;
        if listed.iter().all(u8::is_ascii_whitespace) {
            return Ok(Previous { commit, text: None });
        }
        let shown = self
            .git(&["show", &format!("{commit}:{path}")])
            .map_err(&unreadable)?;
        let text = String::from_utf8_lossy(&shown).into_owned();
        Ok(Previous {
            commit,
            text: Some(text),
        })
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

/// One Markdown file. `visible[i]` is `None` for a line that renders as no prose: a fence,
/// indented code, an HTML block or a comment. A visible line loses its inline comments and
/// its struck text, and keeps its indentation.
struct Doc {
    rel: String,
    lines: Vec<String>,
    visible: Vec<Option<String>>,
    /// `fenced[i]` marks a line of a fence, or a fence marker.
    fenced: Vec<bool>,
    headings: Vec<Heading>,
    bytes: usize,
    /// A fence that never closes hides the rest of the file from every other rule.
    unclosed_fence: Option<usize>,
    /// So does a comment or raw HTML block.
    unclosed_raw: Option<usize>,
}

impl Doc {
    fn parse(rel: &str, raw: &str) -> Doc {
        // CommonMark ends a line at LF, CR or CRLF, and `str::lines` splits only on LF.
        let text = raw
            .strip_prefix('\u{feff}')
            .unwrap_or(raw)
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let mut lexer = Lexer::default();
        let mut visible = Vec::with_capacity(lines.len());
        let mut fenced = Vec::with_capacity(lines.len());
        for (index, line) in lines.iter().enumerate() {
            let lexed = lexer.step(index, line);
            fenced.push(matches!(lexed, Lexed::Fenced));
            visible.push(match lexed {
                Lexed::Text(shown) => Some(shown),
                Lexed::Fenced | Lexed::Hidden => None,
            });
        }
        hide_paragraph_comments(&lines, &mut visible, &lexer.paragraphs);
        Doc {
            rel: rel.to_owned(),
            lines,
            visible,
            fenced,
            headings: lexer.headings,
            bytes: text.len(),
            unclosed_fence: lexer.fence.map(|fence| fence.line),
            unclosed_raw: lexer.raw.map(|raw| raw.line),
        }
    }

    fn headings(&self) -> &[Heading] {
        &self.headings
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

/// What one source line is, for the rules that read prose.
enum Lexed {
    Text(String),
    Fenced,
    Hidden,
}

#[derive(Clone, Copy)]
struct Fence {
    marker: u8,
    width: usize,
    line: usize,
    /// Column of the list item that holds it. A fence ends with its item.
    base: usize,
}

/// An open comment or raw HTML block.
#[derive(Clone, Copy)]
struct RawBlock {
    ends: &'static [&'static str],
    line: usize,
    /// Column of the list item that holds it. The block ends with its item.
    base: usize,
}

/// An open paragraph, which a setext underline turns into a heading.
struct Paragraph {
    first: usize,
    text: String,
    /// In a block quote: lazy text continues it, and an underline cannot reach it.
    quoted: bool,
}

/// The CommonMark block structure the rules depend on, one line at a time: list items for
/// their columns, paragraphs, fences, indented code, HTML blocks and comments. A block quote
/// stays plain text: its lines hold no heading and no field, so it can satisfy no rule.
#[derive(Default)]
struct Lexer {
    /// Content column of each open list item, outermost first.
    items: Vec<usize>,
    paragraph: Option<Paragraph>,
    fence: Option<Fence>,
    raw: Option<RawBlock>,
    /// Column of the list item holding an open HTML block, which a blank line ends.
    html: Option<usize>,
    headings: Vec<Heading>,
    /// First and last line of each paragraph, the only blocks an inline comment can span.
    paragraphs: Vec<(usize, usize)>,
}

impl Lexer {
    fn step(&mut self, index: usize, line: &str) -> Lexed {
        let column = indent(line);
        let blank = line.trim().is_empty();
        if let Some(raw) = self.raw {
            if blank || column >= raw.base {
                if closes_raw(line, raw.ends) {
                    self.raw = None;
                }
                return Lexed::Hidden;
            }
            self.raw = None;
        }
        if let Some(fence) = self.fence {
            if blank || column >= fence.base {
                let inner = dedent(line, fence.base);
                if fence_marker(&inner).is_some_and(|(marker, width, closing)| {
                    marker == fence.marker && width >= fence.width && closing
                }) {
                    self.fence = None;
                }
                return Lexed::Fenced;
            }
            self.fence = None;
        }
        if blank {
            self.html = None;
            self.paragraph = None;
            return Lexed::Text(String::new());
        }
        if let Some(html) = self.html {
            if column >= html {
                return Lexed::Hidden;
            }
            self.html = None;
        }
        let depth = self
            .items
            .iter()
            .take_while(|content| column >= **content)
            .count();
        let base = depth.checked_sub(1).map_or(0, |at| self.items[at]);
        let text = dedent(line, base);
        if depth < self.items.len() {
            if self.paragraph.is_some() && !starts_block(&text, false) {
                return self.paragraph_line(index, line, text.trim());
            }
            self.items.truncate(depth);
            self.paragraph = None;
        }
        self.leaf(index, line, text, base)
    }

    fn leaf(&mut self, index: usize, line: &str, mut text: String, mut base: usize) -> Lexed {
        loop {
            let column = indent(&text);
            let body = text.trim_start();
            if column >= 4 {
                return if self.paragraph.is_some() {
                    self.paragraph_line(index, line, body)
                } else {
                    Lexed::Hidden
                };
            }
            if let Some(level) = setext_level(body)
                && self.paragraph.as_ref().is_some_and(|open| !open.quoted)
                && let Some(paragraph) = self.paragraph.take()
            {
                self.headings.push(Heading {
                    index: paragraph.first,
                    level,
                    text: paragraph.text,
                });
                return Lexed::Text(shown_text(line));
            }
            if is_thematic(body) {
                self.paragraph = None;
                return Lexed::Text(shown_text(line));
            }
            if body.starts_with('>') {
                return self.quote_line(index, line, body);
            }
            // A comment opening the line is a block, so what follows it is no heading.
            if let Some(ends) = raw_block_ends(body) {
                self.paragraph = None;
                if !closes_raw(body, ends) {
                    self.raw = Some(RawBlock {
                        ends,
                        line: index,
                        base,
                    });
                }
                return Lexed::Hidden;
            }
            // Struck text or a comment may hide part of a title, never make a line a heading.
            if let Some((level, title)) = atx_heading(body) {
                self.paragraph = None;
                self.headings.push(Heading {
                    index,
                    level,
                    text: shown_text(&title).trim().to_owned(),
                });
                return Lexed::Text(shown_text(line));
            }
            if html_block_start(body, self.paragraph.is_some()) {
                self.paragraph = None;
                self.html = Some(base);
                return Lexed::Hidden;
            }
            if let Some((marker, width, _)) = fence_marker(&text) {
                self.paragraph = None;
                self.fence = Some(Fence {
                    marker,
                    width,
                    line: index,
                    base,
                });
                return Lexed::Fenced;
            }
            let Some(marker) = list_marker(body, self.paragraph.is_some())
                .filter(|_| self.items.len() < MAX_LIST_DEPTH)
            else {
                return self.paragraph_line(index, line, body);
            };
            let after = &body[marker..];
            let spaces = indent_from(after, base + column + marker);
            let content = after.trim_start_matches([' ', '\t']);
            let (width, pad) = if content.is_empty() || spaces > 4 {
                (marker + 1, spaces.saturating_sub(1))
            } else {
                (marker + spaces, 0)
            };
            let rest = format!("{}{content}", " ".repeat(pad));
            base += column + width;
            self.items.push(base);
            self.paragraph = None;
            if rest.trim().is_empty() {
                return Lexed::Text(shown_text(line));
            }
            text = rest;
        }
    }

    /// A quote holds no heading and no field. Its last paragraph can still continue lazily.
    fn quote_line(&mut self, index: usize, line: &str, body: &str) -> Lexed {
        let mut inner = body.trim_start_matches(['>', ' ']);
        while let Some(marker) = list_marker(inner, false) {
            inner = inner[marker..].trim_start();
        }
        if inner.is_empty() || starts_block(inner, false) {
            self.paragraph = None;
            return Lexed::Text(shown_text(line));
        }
        if self.paragraph.as_ref().is_some_and(|open| !open.quoted) {
            self.paragraph = None;
        }
        let shown = self.paragraph_line(index, line, inner);
        if let Some(paragraph) = &mut self.paragraph {
            paragraph.quoted = true;
        }
        shown
    }

    fn paragraph_line(&mut self, index: usize, line: &str, body: &str) -> Lexed {
        let shown = shown_text(body);
        if let Some(paragraph) = &mut self.paragraph {
            paragraph.text.push(' ');
            paragraph.text.push_str(shown.trim());
            if let Some(run) = self.paragraphs.last_mut() {
                run.1 = index;
            }
        } else {
            self.paragraph = Some(Paragraph {
                first: index,
                text: shown.trim().to_owned(),
                quoted: false,
            });
            self.paragraphs.push((index, index));
        }
        Lexed::Text(shown_text(line))
    }
}

/// Hides each comment that spans lines. Blocks parse first, so a comment spans lines only
/// inside one paragraph. A `<!--` no later line of its paragraph closes is literal text.
fn hide_paragraph_comments(
    lines: &[String],
    visible: &mut [Option<String>],
    paragraphs: &[(usize, usize)],
) {
    for &(first, last) in paragraphs {
        let mut index = first;
        let mut text = lines[first].clone();
        while index < last {
            let (kept, open) = without_comments(&text);
            let Some(at) = open else {
                index += 1;
                text.clone_from(&lines[index]);
                continue;
            };
            let Some(close) = (index + 1..=last).find(|line| lines[*line].contains("-->")) else {
                break;
            };
            visible[index] = Some(without_struck_text(&kept[..at]));
            visible[index + 1..close].fill(None);
            text = lines[close]
                .split_once("-->")
                .map_or_else(String::new, |(_, tail)| tail.to_owned());
            visible[close] = Some(shown_text(&text));
            index = close;
        }
    }
}

/// Columns of leading whitespace. A tab moves to the next multiple of four.
fn indent(line: &str) -> usize {
    indent_from(line, 0)
}

/// Columns of leading whitespace in text that starts at column `start`.
fn indent_from(line: &str, start: usize) -> usize {
    let mut column = start;
    for byte in line.bytes() {
        match byte {
            b' ' => column += 1,
            b'\t' => column += 4 - column % 4,
            _ => break,
        }
    }
    column - start
}

/// `line` without its first `columns` columns of leading whitespace.
fn dedent(line: &str, columns: usize) -> String {
    let body = line.trim_start_matches([' ', '\t']);
    format!("{}{body}", " ".repeat(indent(line).saturating_sub(columns)))
}

/// `line` as prose: no inline comments, no struck text.
fn shown_text(line: &str) -> String {
    without_struck_text(&without_comments(line).0)
}

/// `line` without its closed HTML comments, and the offset in that text of a `<!--` left
/// open. Code spans and comments compete left to right, so one in a code span opens nothing.
fn without_comments(line: &str) -> (String, Option<usize>) {
    let bytes = line.as_bytes();
    let mut kept = String::with_capacity(line.len());
    let mut copied = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'`' => index = code_span_at(line, index).map_or_else(|end| end, |span| span.end),
            b'<' if line[index..].starts_with("<!--") => {
                kept.push_str(&line[copied..index]);
                // `<!-->` is a whole comment: its closing run overlaps its opening run.
                let Some(close) = line[index + 2..].find("-->") else {
                    let at = kept.len();
                    kept.push_str(&line[index..]);
                    return (kept, Some(at));
                };
                index += close + 5;
                copied = index;
            }
            _ => index += 1,
        }
    }
    kept.push_str(&line[copied..]);
    (kept, None)
}

/// True for a line that ends a paragraph instead of continuing it.
fn starts_block(text: &str, interrupts: bool) -> bool {
    let body = text.trim_start();
    indent(text) < 4
        && (is_thematic(body)
            || body.starts_with('>')
            || atx_heading(body).is_some()
            || raw_block_ends(body).is_some()
            || html_block_start(body, true)
            || fence_marker(text).is_some()
            || list_marker(body, interrupts).is_some())
}

/// CommonMark ATX heading: up to three spaces, one to six `#`, then a space, a tab or the
/// end of the line. A closing `#` run is not part of the title.
fn atx_heading(line: &str) -> Option<(usize, String)> {
    let text = line.trim_start_matches(' ');
    if line.len() - text.len() > 3 {
        return None;
    }
    let level = text.bytes().take_while(|byte| *byte == b'#').count();
    let rest = &text[level..];
    if !(1..=6).contains(&level) || !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
        return None;
    }
    let title = rest.trim();
    let open = title.trim_end_matches('#');
    let title = if open.is_empty() {
        ""
    } else if open.ends_with([' ', '\t']) {
        open.trim_end()
    } else {
        title
    };
    Some((level, title.to_owned()))
}

/// The heading level of a setext underline.
fn setext_level(body: &str) -> Option<usize> {
    let underline = body.trim_end();
    if !underline.is_empty() && underline.bytes().all(|byte| byte == b'=') {
        Some(1)
    } else if !underline.is_empty() && underline.bytes().all(|byte| byte == b'-') {
        Some(2)
    } else {
        None
    }
}

fn is_thematic(body: &str) -> bool {
    let mut marks = body.bytes().filter(|byte| !matches!(byte, b' ' | b'\t'));
    let first = marks.next();
    matches!(first, Some(b'-' | b'*' | b'_'))
        && marks.try_fold(1_usize, |count, byte| {
            (Some(byte) == first).then_some(count + 1)
        }) >= Some(3)
}

/// Byte width of a list marker. Inside a paragraph only a nonempty bullet or `1.` opens a
/// list, so `2024. was a year` stays prose.
fn list_marker(body: &str, interrupts: bool) -> Option<usize> {
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    let width = if digits == 0 {
        body.starts_with(['-', '+', '*']).then_some(1)?
    } else {
        let ordered = digits <= 9 && body[digits..].starts_with(['.', ')']);
        ordered.then_some(digits + 1)?
    };
    let after = &body[width..];
    let spaced = after.is_empty() || after.starts_with([' ', '\t']);
    let can_open = !interrupts
        || (!after.trim().is_empty() && (digits == 0 || body[..digits].parse() == Ok(1_usize)));
    (spaced && can_open).then_some(width)
}

/// The terminators of a block that runs to one of them, whatever lies between.
fn raw_block_ends(body: &str) -> Option<&'static [&'static str]> {
    if body.starts_with("<!--") {
        return Some(&["-->"]);
    }
    if body.starts_with("<?") {
        return Some(&["?>"]);
    }
    if body.starts_with("<![CDATA[") {
        return Some(&["]]>"]);
    }
    let declaration = body.strip_prefix("<!");
    if declaration.is_some_and(|rest| rest.starts_with(|ch: char| ch.is_ascii_alphabetic())) {
        return Some(&[">"]);
    }
    let tag = body.strip_prefix('<')?.to_ascii_lowercase();
    RAW_HTML_ELEMENTS.iter().find_map(|name| {
        let rest = tag.strip_prefix(name)?;
        (rest.is_empty() || rest.starts_with([' ', '\t', '>'])).then_some(&RAW_HTML_ENDS[..])
    })
}

/// True when `line` holds a terminator, in any case.
fn closes_raw(line: &str, ends: &[&str]) -> bool {
    let lower = line.to_ascii_lowercase();
    ends.iter().any(|end| lower.contains(end))
}

/// An HTML block that a blank line ends. Markdown inside it renders as raw text, so a
/// heading there is no heading. A lone other tag starts one only outside a paragraph.
fn html_block_start(body: &str, in_paragraph: bool) -> bool {
    let Some(tail) = body.strip_prefix('<') else {
        return false;
    };
    let tag = tail.strip_prefix('/').unwrap_or(tail);
    let width = tag
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        .count();
    let after = &tag[width..];
    let ends_name =
        after.is_empty() || after.starts_with([' ', '\t', '>']) || after.starts_with("/>");
    if !ends_name || !tag.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
        return false;
    }
    let name = tag[..width].to_ascii_lowercase();
    let lone_tag = body.trim_end().ends_with('>') && !after.contains('<');
    HTML_BLOCK_TAGS.contains(&name.as_str()) || (lone_tag && !in_paragraph)
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
    let wrapped = value.trim_matches(|ch: char| ch.is_whitespace() || ".,:;!*_`()".contains(ch));
    // `[TBD]` and `"UNKNOWN"` say what `TBD` and `UNKNOWN` say.
    let bare = wrapped.trim_matches(|ch: char| !ch.is_alphanumeric());
    let label_tail = bare.strip_prefix("UNKNOWN");
    let lower = bare.to_lowercase();
    // `TBD: the owner decides` is a marker with a label. `Pending calls fail` is a statement.
    let labelled = NON_VALUES.iter().any(|marker| {
        lower
            .strip_prefix(marker)
            .is_some_and(|tail| !tail.trim_start().starts_with(char::is_alphanumeric))
    });
    if lower == "unknown" || label_tail.is_some_and(|tail| !tail.starts_with(char::is_alphanumeric))
    {
        Some(NonValue::Unknown)
    } else if (wrapped.starts_with('<') && wrapped.ends_with('>') && !wrapped.contains("://"))
        || !bare.contains(char::is_alphanumeric)
        || labelled
        || (control && lower == "none")
    {
        Some(NonValue::Placeholder)
    } else {
        None
    }
}

/// How far a line may differ from a label in case. Marks and spacing never make a second
/// label, and only `Exact` can hold a value, so a variant alone leaves the field absent.
#[derive(Clone, Copy)]
enum Spelling {
    Exact,
    /// Any case but a lower-case start: `owner: x` in a list or a wrapped line is prose,
    /// as in landed specs gh566 and gh527.
    Body,
    /// Any case: above the first `##`, every label is a statement of the spec.
    Header,
}

impl Spelling {
    fn matches(self, name: &str, label: &str) -> bool {
        let lower = |text: &str| text.starts_with(|ch: char| ch.is_ascii_lowercase());
        name == label
            || match self {
                Spelling::Exact => false,
                Spelling::Body => name.eq_ignore_ascii_case(label) && !lower(name),
                Spelling::Header => name.eq_ignore_ascii_case(label),
            }
    }
}

/// `text` from its first word, past every list marker, so `- - x` reads as `- x` does.
fn without_list_markers(text: &str) -> &str {
    let mut text = text.trim_start();
    while let Some(marker) = list_marker(text, false) {
        text = text[marker..].trim_start();
    }
    text
}

/// Value of `label` on a `- **Label:** value` or `Label: value` line.
fn field_value(line: &str, label: &str, spelling: Spelling) -> Option<String> {
    let line: String = line.chars().filter(|ch| !INVISIBLE.contains(ch)).collect();
    let (spelled, tail) = without_list_markers(&line).split_once(':')?;
    // A code span that runs past the colon quotes a field. `Pin:` in code only marks one.
    let quoted = spelled.matches('`').count() % 2 == 1 && !tail.starts_with('`');
    let plain: String = spelled
        .chars()
        .filter(|ch| !LABEL_MARKS.contains(ch))
        .collect();
    let words: Vec<&str> = plain.split_whitespace().collect();
    (!quoted && spelling.matches(&words.join(" "), label))
        .then(|| tail.trim_start_matches(LABEL_MARKS).trim().to_owned())
}

/// Each line of a range that states `label`.
fn field_lines(doc: &Doc, start: usize, end: usize, label: &str, spelling: Spelling) -> Vec<usize> {
    (start..end)
        .filter(|index| {
            doc.visible[*index]
                .as_deref()
                .is_some_and(|line| field_value(line, label, spelling).is_some())
        })
        .collect()
}

/// A field stated once.
struct Stated {
    line: usize,
    /// The value on its line, then each statement nested under it. See `field_statements`.
    statements: Vec<String>,
}

impl Stated {
    fn read(doc: &Doc, line: usize, end: usize, label: &str) -> Stated {
        let inline = doc.visible[line]
            .as_deref()
            .and_then(|text| field_value(text, label, Spelling::Exact))
            .filter(|value| !value.is_empty());
        let statements = field_statements(doc, line, end, inline);
        Stated { line, statements }
    }

    /// The whole value, so a format rule reads every statement.
    fn value(&self) -> String {
        self.statements.join(" ")
    }
}

/// What a line range states for one field.
enum Field {
    Absent,
    Once(Stated),
    /// Two statements are two authorities, so no rule reads a value, in either order.
    Repeated,
}

fn add_duplicate(
    doc: &Doc,
    line: usize,
    detail: String,
    slot: Option<String>,
    report: &mut Report,
) {
    report.findings.push(Finding {
        rule: Rule::FieldDuplicate,
        file: doc.rel.clone(),
        line,
        detail,
        slot,
    });
}

/// The one reader of a single-valued field. `slot` is the `id.field` a `migrated:` row may
/// name, so a repeat does not read as a filled slot.
fn read_field(
    doc: &Doc,
    start: usize,
    end: usize,
    label: &str,
    spelling: Spelling,
    slot: Option<String>,
    report: &mut Report,
) -> Field {
    let exact = |line: usize| !field_lines(doc, line, line + 1, label, Spelling::Exact).is_empty();
    match field_lines(doc, start, end, label, spelling)[..] {
        [] => Field::Absent,
        [line] if exact(line) => Field::Once(Stated::read(doc, line, end, label)),
        [_] => Field::Absent,
        [first, second, ..] => {
            let detail = format!(
                "`{label}:` is already stated at line {}, so neither value has authority. Keep \
                 one",
                first + 1
            );
            add_duplicate(doc, second + 1, detail, slot, report);
            Field::Repeated
        }
    }
}

/// What a field states: `inline`, then each block nested under it, without list markers,
/// so the non-value rules examine what a reader sees. A list item or a paragraph opens a
/// statement. A wrapped line, a table row or a fence only continues one, so `unknown.`
/// on its own line ends a sentence and a code sample is no value.
fn field_statements(doc: &Doc, field: usize, end: usize, inline: Option<String>) -> Vec<String> {
    let own = indent(&doc.lines[field]);
    let mut statements: Vec<String> = inline.into_iter().collect();
    let mut open = !statements.is_empty();
    for index in field + 1..end {
        let line = &doc.lines[index];
        if line.trim().is_empty() {
            open = false;
            continue;
        }
        if indent(line) <= own {
            break;
        }
        let fenced = doc.fenced[index];
        let text = if fenced {
            Some(line.as_str())
        } else {
            doc.visible[index].as_deref()
        };
        let Some(text) = text.map(str::trim) else {
            continue;
        };
        let value = without_list_markers(text);
        if fenced && fence_marker(value).is_some() {
            continue;
        }
        let item = value.len() < text.len();
        match statements.last_mut().filter(|_| fenced || (open && !item)) {
            Some(statement) => {
                statement.push(' ');
                statement.push_str(value);
            }
            None if !value.is_empty() => statements.push(value.to_owned()),
            None => {}
        }
        open = !fenced;
    }
    statements
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
    // `(#1311 AC-2)` and `**#1311** AC-2` are foreign too.
    let bare = word.trim_matches(|ch: char| "()[]*_`".contains(ch));
    // `[#1311](url) AC-2` is foreign. `[goal](index.md) AC-2` is this spec's AC-2.
    let link_text = word
        .split_once("](")
        .filter(|_| word.ends_with(')'))
        .and_then(|(text, _)| text.rsplit_once('['))
        .map(|(_, text)| text);
    is_tracker_word(bare) || link_text.is_some_and(is_tracker_word)
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

/// The destination opening `text`, and what follows it. A title or a stray parenthesis
/// never decides where the next link starts.
fn link_destination(text: &str) -> (String, &str) {
    let text = text.trim_start();
    if let Some(inner) = text.strip_prefix('<')
        && let Some(end) = inner.find('>')
    {
        return (inner[..end].to_owned(), &inner[end + 1..]);
    }
    let mut depth = 0_usize;
    let mut escaped = false;
    for (offset, ch) in text.char_indices() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '(' {
            depth += 1;
        } else if ch == ')' && depth > 0 {
            depth -= 1;
        } else if ch == ')' || ch.is_whitespace() {
            return (text[..offset].to_owned(), &text[offset..]);
        }
    }
    (text.to_owned(), "")
}

/// The label and the tail of a `[label]: tail` line.
fn definition_parts(line: &str) -> Option<(&str, &str)> {
    without_list_markers(line)
        .strip_prefix('[')?
        .split_once("]:")
}

/// The target of a `[label]: target` reference definition. `[^1]:` is a footnote.
fn reference_target(line: &str) -> Option<String> {
    let (label, tail) = definition_parts(line)?;
    let (target, title) = link_destination(tail);
    let title = title.trim();
    // `[FACT]: the host owns it` is prose: CommonMark allows only a title after the target.
    let definition = title.is_empty() || title.starts_with(['"', '\'', '(']);
    (definition && !label.is_empty() && !label.starts_with('^') && !label.contains(']'))
        .then_some(target)
}

/// The target of a `[label]:` line that leaves its target to the next line, as CommonMark
/// allows. Prose on that line makes no definition.
fn continued_target(line: &str, next: &str) -> Option<String> {
    // A heading, a rule or a list ends the paragraph, so `[label]:` stays text.
    if starts_block(next, true) || setext_level(next.trim()).is_some() {
        return None;
    }
    reference_target(line).filter(String::is_empty)?;
    reference_target(&format!("{line} {next}")).filter(|target| !target.is_empty())
}

/// `text` as CommonMark matches labels: only spaces and tabs collapse, and lower case comes
/// first so `\u{1e9e}` meets `SS`. The last step only keeps a message readable.
fn folded(text: &str) -> String {
    let words: Vec<&str> = text
        .split([' ', '\t'])
        .filter(|word| !word.is_empty())
        .collect();
    words.join(" ").to_lowercase().to_uppercase().to_lowercase()
}

/// The label a definition or footnote claims. `[label]:` with no target anywhere is text.
fn defined_label(line: &str, next: Option<&str>) -> Option<String> {
    let (label, tail) = definition_parts(line)?;
    let whole = !tail.trim().is_empty() && reference_target(line).is_some();
    let continued = next.is_some_and(|next| continued_target(line, next).is_some());
    (label.starts_with('^') || whole || continued).then(|| folded(label))
}

/// Link targets on a visible line: inline links, images and reference definitions.
fn link_targets(line: &str) -> Vec<String> {
    let line = without_inline_code(line);
    let mut targets: Vec<String> = reference_target(&line).into_iter().collect();
    let mut rest = line.as_str();
    while let Some(at) = rest.find("](") {
        let (target, tail) = link_destination(&rest[at + 2..]);
        targets.push(target);
        rest = tail;
    }
    targets
}

/// True for a URL with a scheme, which no repository rule can check. The scheme must open
/// the target: `../x.rs?y=://` is a relative path.
fn is_external(target: &str) -> bool {
    target.starts_with("mailto:")
        || target.split_once("://").is_some_and(|(scheme, _)| {
            scheme.starts_with(|ch: char| ch.is_ascii_alphabetic())
                && scheme
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"+-.".contains(&byte))
        })
}

/// GitHub's heading anchors. A repeated heading takes the first `-n` no heading holds.
fn heading_slugs(doc: &Doc) -> BTreeSet<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
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
        let mut unique = slug.clone();
        while seen.contains_key(&unique) {
            let count = seen.entry(slug.clone()).or_insert(0);
            *count += 1;
            unique = format!("{slug}-{count}");
        }
        seen.insert(unique, 0);
    }
    seen.into_keys().collect()
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

/// The first symlink on a resolved path. The lane is a rule on path text, so a link
/// through a symlink can reach any file. A Windows checkout may hold a tracked symlink as
/// a text file, so the index speaks as well as the file system.
fn symlink_on_path(root: &Path, resolved: &str, symlinks: &BTreeSet<String>) -> Option<String> {
    let mut prefix = String::new();
    for part in resolved.split('/').filter(|part| !part.is_empty()) {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        let on_disk = fs::symlink_metadata(root.join(&prefix));
        if symlinks.contains(&prefix) || on_disk.is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Some(prefix);
        }
    }
    None
}

fn check_links(root: &Path, doc: &Doc, symlinks: &BTreeSet<String>, report: &mut Report) {
    // A spec can link one target many times. Each parse of it costs the whole target.
    let mut anchors: BTreeMap<String, Option<BTreeSet<String>>> = BTreeMap::new();
    let mut labels: BTreeMap<String, usize> = BTreeMap::new();
    for (index, line) in doc.visible.iter().enumerate() {
        let Some(line) = line else { continue };
        let next = doc.visible.get(index + 1).and_then(Option::as_deref);
        if let Some(label) = defined_label(line, next) {
            if let Some(first) = labels.get(&label) {
                report.add(
                    Rule::Link,
                    &doc.rel,
                    index + 1,
                    format!(
                        "`[{label}]` is already defined at line {first}. CommonMark keeps the \
                         first and renders this one nowhere. Remove one"
                    ),
                );
            }
            labels.entry(label).or_insert(index + 1);
        }
        let continued = next.and_then(|next| continued_target(line, next));
        for target in link_targets(line).into_iter().chain(continued) {
            if is_external(&target) || target.is_empty() {
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
            if let Some(symlink) = symlink_on_path(root, &resolved, symlinks) {
                report.add(
                    Rule::LinkCode,
                    &doc.rel,
                    index + 1,
                    format!(
                        "link `{target}` passes through symlink `{symlink}`, so its text does not \
                         say where it leads. Link the real file"
                    ),
                );
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
            let slugs = anchors.entry(resolved.clone()).or_insert_with(|| {
                let raw = fs::read_to_string(&on_disk).ok()?;
                Some(heading_slugs(&Doc::parse(&resolved, &raw)))
            });
            let Some(slugs) = slugs else {
                report.add(
                    Rule::Anchor,
                    &doc.rel,
                    index + 1,
                    format!("cannot read `{resolved}` to check anchor `#{fragment}`"),
                );
                continue;
            };
            if !slugs.contains(fragment) {
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
    let end = header_end(doc);
    if !doc.lines.first().is_some_and(|line| line.starts_with("# ")) {
        report.add(
            Rule::Fields,
            &doc.rel,
            1,
            "the first line must be the `# ` title".to_owned(),
        );
    }
    let mut value = |label: &str, valid: &dyn Fn(&str) -> bool, expected: &str| {
        let slot = format!("header.{label}");
        match read_field(doc, 0, end, label, Spelling::Header, Some(slot), report) {
            Field::Once(field) if valid(&field.value()) => Some(field),
            Field::Once(field) => {
                report.add(
                    Rule::Fields,
                    &doc.rel,
                    field.line + 1,
                    format!("`{label}: {}` must be {expected}", field.value()),
                );
                None
            }
            Field::Absent => {
                report.add(
                    Rule::Fields,
                    &doc.rel,
                    1,
                    format!("the header must state `{label}:` ({expected})"),
                );
                None
            }
            Field::Repeated => None,
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
    for statement in owner.iter().flat_map(|field| &field.statements) {
        add_non_value(
            doc,
            1,
            HEADER_OWNER_SLOT,
            statement,
            non_value(statement, true),
            report,
        );
    }
    pin.map(|field| field.value())
}

/// The line of the first `##`. What stands above it is the file's header.
fn header_end(doc: &Doc) -> usize {
    doc.headings()
        .iter()
        .find(|heading| heading.level == 2)
        .map_or(doc.lines.len(), |heading| heading.index)
}

/// The header owns the facts of a spec. The same field in a part or a section would be a
/// second authority that no rule reads, however well it matches.
fn check_restated_header(index: &Doc, parts: &[Doc], report: &mut Report) {
    let mut scopes = Vec::new();
    for label in HEADER_ONLY_FIELDS {
        let start = header_end(index);
        scopes.push((index, start, index.lines.len(), label, Spelling::Body));
    }
    for part in parts {
        let header = header_end(part);
        for label in HEADER_FIELDS {
            scopes.push((part, 0, header, label, Spelling::Header));
        }
        for label in HEADER_ONLY_FIELDS {
            scopes.push((part, header, part.lines.len(), label, Spelling::Body));
        }
    }
    for (doc, start, end, label, spelling) in scopes {
        if let Some(line) = field_lines(doc, start, end, label, spelling).first() {
            let detail = format!(
                "`{label}:` belongs to the header of `{}` alone. Remove it here",
                index.rel
            );
            add_duplicate(doc, line + 1, detail, None, report);
        }
    }
}

fn add_non_value(
    doc: &Doc,
    line: usize,
    slot: &str,
    value: &str,
    kind: Option<NonValue>,
    report: &mut Report,
) {
    let rule = match kind {
        Some(NonValue::Unknown) => Rule::FieldUnknown,
        Some(NonValue::Placeholder) => Rule::FieldPlaceholder,
        None => return,
    };
    report.findings.push(Finding {
        rule,
        file: doc.rel.clone(),
        line,
        detail: format!(
            "`{slot}` holds `{value}`, which states nothing. Write the real content; a new spec \
             cannot defer a required field"
        ),
        slot: Some(slot.to_owned()),
    });
}

/// `kel142-slug` carries `KEL-142`. The ID in the name and the tracker are one fact.
fn check_tracker(doc: &Doc, stem: &str, report: &mut Report) {
    let id = stem.split('-').next().unwrap_or("");
    let digits = id.trim_start_matches(|ch: char| ch.is_ascii_alphabetic());
    let expected = format!("{}-{digits}", id[..id.len() - digits.len()].to_uppercase());
    // `check_header` reports a repeat or a variant, which has no single value to compare.
    let mut reported = Report::default();
    let end = header_end(doc);
    let read = read_field(
        doc,
        0,
        end,
        "Tracker",
        Spelling::Header,
        None,
        &mut reported,
    );
    if let Field::Once(field) = read {
        let value = field.value();
        if value != expected {
            report.add(
                Rule::Fields,
                &doc.rel,
                field.line + 1,
                format!("`Tracker: {value}` must be `{expected}`, the ID in the spec name"),
            );
        }
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
        .iter()
        .filter(|heading| heading.level == 2)
        .map(|heading| (heading.index, heading.text.clone()))
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
    let mut titles = BTreeSet::new();
    for doc in parts {
        for (index, text) in section_headings(doc) {
            // Parts may split a section under subtitles, never restate one.
            if !titles.insert(folded(&text)) {
                report.add(
                    Rule::Sections,
                    &doc.rel,
                    index + 1,
                    format!("`## {text}` repeats. A split section gives each part a subtitle"),
                );
                continue;
            }
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

/// A file has one title, or a reader cannot tell which one names it.
fn check_titles(docs: &[&Doc], report: &mut Report) {
    for doc in docs {
        let mut titles = doc.headings().iter().filter(|heading| heading.level == 1);
        if let (Some(_), Some(second)) = (titles.next(), titles.next()) {
            report.add(
                Rule::Sections,
                &doc.rel,
                second.index + 1,
                format!("a file has one `# ` title; `# {}` is a second", second.text),
            );
        }
    }
}

fn check_part_header(doc: &Doc, report: &mut Report) {
    for label in PART_FIELDS {
        read_field(
            doc,
            0,
            header_end(doc),
            label,
            Spelling::Header,
            None,
            report,
        );
    }
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

/// The cells of a table row. GFM splits on every pipe that no backslash escapes.
fn table_cells(row: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut escaped = false;
    for ch in row.chars() {
        if ch == '|' && !escaped {
            cells.push(String::new());
        } else if let Some(cell) = cells.last_mut() {
            cell.push(ch);
        }
        escaped = ch == '\\' && !escaped;
    }
    if cells.first().is_some_and(|cell| cell.trim().is_empty()) {
        cells.remove(0);
    }
    if cells.len() > 1 && cells.last().is_some_and(|cell| cell.trim().is_empty()) {
        cells.pop();
    }
    cells.iter().map(|cell| cell.trim().to_owned()).collect()
}

fn is_table_delimiter(row: &str) -> bool {
    let cells = table_cells(row);
    cells.len() == 3
        && cells.iter().all(|cell| {
            let dashes = cell.trim_matches(':');
            !dashes.is_empty() && dashes.bytes().all(|byte| byte == b'-')
        })
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
    let end = index.body_end(headings, at);
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
    if !rows
        .get(1)
        .is_some_and(|(_, text)| is_table_delimiter(text))
    {
        report.add(
            Rule::PartTable,
            &index.rel,
            start,
            "the part table header needs its `|---|---|---|` row, or no table renders".to_owned(),
        );
    }
    for (line, text) in rows.iter().skip(2) {
        let cells = table_cells(text);
        let target = link_targets(&cells[0]);
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

/// The ID a heading defines: it opens the heading, whatever follows, so a differently
/// worded restatement is still a second definition.
fn defined_id(text: &str) -> Option<(&'static str, u32)> {
    let (kind, number, _) = parse_id(text.trim_start_matches(LABEL_MARKS))?;
    Some((kind, number))
}

/// Only a blocker's owner stays legal as `UNKNOWN`: root AGENTS.md tells an agent to say
/// so and name the first identification action. Every other field states a fact.
fn allows_unknown(kind: &str, label: &str) -> bool {
    kind == "B" && label == "Owner"
}

struct Definition {
    file: String,
    line: usize,
    /// `None` for a blocker with no single `Status:`, which a field rule already refused.
    open: Option<bool>,
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
            let Some((kind, number)) = defined_id(&heading.text) else {
                continue;
            };
            let line = heading.index + 1;
            // A definition ends at the next one, so a nested ID lends its parent no field.
            let body = headings[at + 1..]
                .iter()
                .find(|next| next.level <= heading.level || defined_id(&next.text).is_some())
                .map_or(doc.lines.len(), |next| next.index);
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
            let mut stated: BTreeMap<&str, Stated> = BTreeMap::new();
            let mut required = |labels: &[&'static str], rule: Rule| {
                for label in labels.iter().copied() {
                    let slot = format!("{kind}-{number}.{label}");
                    let read = read_field(
                        doc,
                        heading.index + 1,
                        body,
                        label,
                        Spelling::Body,
                        Some(slot.clone()),
                        report,
                    );
                    let field = match read {
                        Field::Once(field) if !field.statements.is_empty() => field,
                        Field::Repeated => continue,
                        Field::Absent | Field::Once(_) => {
                            report.add(
                                rule,
                                &doc.rel,
                                line,
                                format!("{kind}-{number} needs a non-empty `{label}:` field"),
                            );
                            continue;
                        }
                    };
                    for statement in &field.statements {
                        let found =
                            non_value(statement, CONTROL_FIELDS.contains(&label)).filter(|found| {
                                *found != NonValue::Unknown || !allows_unknown(kind, label)
                            });
                        add_non_value(doc, field.line + 1, &slot, statement, found, report);
                    }
                    stated.insert(label, field);
                }
            };
            match kind {
                "AC" => required(&AC_FIELDS, Rule::AcFields),
                "B" => required(&BLOCKER_FIELDS, Rule::BlockerFields),
                "E" => required(&EVIDENCE_FIELDS, Rule::EvidenceFields),
                "T" => required(&[TASK_BLOCKERS_FIELD], Rule::TaskFields),
                _ => {}
            }
            let status = stated.get("Status");
            if let Some(field) = status
                && !matches!(field.value().as_str(), "open" | "closed")
            {
                report.add(
                    Rule::BlockerFields,
                    &doc.rel,
                    field.line + 1,
                    format!("B-{number} `Status:` must be open or closed"),
                );
            }
            if let Some(field) = stated.get(TASK_BLOCKERS_FIELD) {
                let none = field.statements.iter().any(|text| text == "none");
                let prose = field
                    .statements
                    .iter()
                    .any(|text| id_tokens(text).is_empty());
                let settled = if none {
                    field.statements.len() == 1
                } else {
                    !prose
                };
                if !settled {
                    report.add(
                        Rule::TaskFields,
                        &doc.rel,
                        field.line + 1,
                        format!("T-{number} `{TASK_BLOCKERS_FIELD}:` must be none or stable IDs"),
                    );
                }
            }
            defined.insert(
                (kind, number),
                Definition {
                    file: doc.rel.clone(),
                    line,
                    open: status.and_then(|field| match field.value().as_str() {
                        "open" => Some(true),
                        "closed" => Some(false),
                        _ => None,
                    }),
                },
            );
        }
    }
    defined
}

/// A definition inside a fence, or a block that never closes, escapes every ID rule.
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
        if let Some(index) = doc.unclosed_raw {
            report.add(
                Rule::Comment,
                &doc.rel,
                index + 1,
                "this HTML comment or raw HTML block never closes, so no rule reads the text \
                 after it. Close it"
                    .to_owned(),
            );
        }
        for (index, line) in doc.lines.iter().enumerate() {
            let defines = doc.fenced[index]
                && atx_heading(line).is_some_and(|(_, text)| defined_id(&text).is_some());
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
    let blockers = |wanted: Option<bool>| -> BTreeSet<u32> {
        defined
            .iter()
            .filter(|((kind, _), definition)| *kind == "B" && definition.open == wanted)
            .map(|((_, number), _)| *number)
            .collect()
    };
    let open = blockers(Some(true));
    let unsettled = blockers(None);
    let listed: BTreeSet<u32> = (headings[at].index + 1..index.body_end(headings, at))
        .filter_map(|line| index.visible[line].as_deref())
        .flat_map(id_tokens)
        .filter(|token| token.kind == "B" && !token.foreign)
        .map(|token| token.number)
        .filter(|number| !unsettled.contains(number))
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

/// One `path:line[-line][:column][@sha]` citation.
#[derive(Debug, PartialEq, Eq)]
struct Citation<'a> {
    path: &'a str,
    /// First and last cited line. `None` is a line part no range can parse.
    lines: Option<(usize, usize)>,
    sha: Option<&'a str>,
}

/// Digits and dashes: a line part, valid or not, so a bad range is refused, never ignored.
fn is_line_part(text: &str) -> bool {
    text.contains(|ch: char| ch.is_ascii_digit())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-')
}

/// Reads one inline-code span. A citation names a repository file, so an image tag, a
/// domain or an absolute path is not one.
fn parse_citation(span: &str) -> Option<Citation<'_>> {
    // A package path holds `@` before the line part. The first `@` after it opens the
    // commit, so a second pin stays in the commit text for `CITE-SHA` to refuse.
    let colon = span.rfind(':')?;
    let (location, sha) = match span[colon..].split_once('@') {
        Some((lines, sha)) => (&span[..colon + lines.len()], Some(sha)),
        None => (span, None),
    };
    let (path, last) = location.rsplit_once(':')?;
    let (path, lines) = match path.rsplit_once(':') {
        Some((inner, cited)) if is_line_part(cited) => (inner, cited),
        _ => (path, last),
    };
    let first = path.split('/').next()?;
    let file = path.rsplit('/').next()?;
    let plausible = is_line_part(lines)
        && path.contains('/')
        && !path.contains(':')
        && !path.starts_with('/')
        && (first.starts_with('.') || !first.contains('.'))
        && file.contains('.');
    let (start, end) = lines.split_once('-').unwrap_or((lines, lines));
    plausible.then(|| Citation {
        path,
        lines: start.parse().ok().zip(end.parse().ok()),
        sha,
    })
}

/// True when both ends of a cited range are lines of a `count`-line file, in order.
fn range_exists(lines: Option<(usize, usize)>, count: usize) -> bool {
    lines.is_some_and(|(start, end)| 1 <= start && start <= end && end <= count)
}

/// The code a span holds. Backticks that wrap a citation inside a wider span are its quoting.
fn span_text<'a>(line: &'a str, span: &CodeSpan) -> &'a str {
    line[span.content.clone()].trim_matches('`')
}

fn check_citations(docs: &[&Doc], pin: &str, history: &dyn History, report: &mut Report) {
    for doc in docs {
        for (index, line) in doc.visible.iter().enumerate() {
            let Some(line) = line else { continue };
            for code in inline_code_spans(line) {
                let span = span_text(line, &code);
                let Some(citation) = parse_citation(span) else {
                    continue;
                };
                let commit = match citation.sha {
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
                let path = citation.path;
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
                    Some(count) if !range_exists(citation.lines, count) => report.add(
                        Rule::CiteLine,
                        &doc.rel,
                        index + 1,
                        format!(
                            "`{span}` is not a line range of the {count} lines at {commit}. Cite \
                             `line` or `start-end`, with 1 <= start <= end <= {count}"
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
    check_titles(&docs, report);
    check_restated_header(index, parts, report);
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

/// True for one `id.field` of a `migrated:` row.
fn is_slot(text: &str) -> bool {
    text == HEADER_OWNER_SLOT
        || text.split_once('.').is_some_and(|(id, label)| {
            parse_id(id).is_some_and(|(_, _, end)| end == id.len())
                && [
                    &AC_FIELDS[..],
                    &BLOCKER_FIELDS[..],
                    &EVIDENCE_FIELDS[..],
                    &[TASK_BLOCKERS_FIELD],
                ]
                .concat()
                .contains(&label)
        })
}

/// The structure column of a baseline row.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Structure {
    Exempt,
    Conforms,
    /// Conforms except that these `id.field` slots hold `UNKNOWN`. Naming each slot binds
    /// it, so a filled field cannot trade places with an unfilled one.
    Migrated(BTreeSet<String>),
}

impl Structure {
    fn parse(cell: &str) -> Option<Structure> {
        match cell {
            "exempt" => Some(Structure::Exempt),
            "ok" => Some(Structure::Conforms),
            _ => {
                let slots: BTreeSet<String> = cell
                    .strip_prefix("migrated:")?
                    .split(',')
                    .map(str::to_owned)
                    .collect();
                let parsed = Structure::Migrated(slots.clone());
                // The printed form is the only form, so `baseline-rows` is a fixed point.
                (slots.iter().all(|slot| is_slot(slot)) && parsed.cell() == cell).then_some(parsed)
            }
        }
    }

    fn cell(&self) -> String {
        match self {
            Structure::Exempt => "exempt".to_owned(),
            Structure::Conforms => "ok".to_owned(),
            Structure::Migrated(slots) => {
                let list: Vec<&str> = slots.iter().map(String::as_str).collect();
                format!("migrated:{}", list.join(","))
            }
        }
    }

    /// True when this cell allows something `previous` did not.
    fn loosens(&self, previous: &Structure) -> bool {
        match (self, previous) {
            (Structure::Conforms, _) | (Structure::Exempt, Structure::Exempt) => false,
            (Structure::Exempt, _) | (Structure::Migrated(_), Structure::Conforms) => true,
            (Structure::Migrated(_), Structure::Exempt) => false,
            (Structure::Migrated(now), Structure::Migrated(then)) => !now.is_subset(then),
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
    // The printed form is the only form, as in `Structure::parse`.
    (value > 0 && value.to_string() == text).then_some(Some(value))
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
                 for a value inside its cap and structure `exempt`, `ok` or \
                 `migrated:<id.field,...>`, sorted"
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

fn expected_row(path: &str, size: Size, structure: &Structure) -> String {
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
                expected_row(path, size, &row.structure)
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

/// A shrink is permanent: no row may allow more than it allowed at the comparison base.
/// The seal alone would let a file regrow to its first size across two pull requests.
fn check_previous(
    rel_baseline: &str,
    rows: &BTreeMap<String, Row>,
    previous: &BTreeMap<String, Row>,
    commit: &str,
    report: &mut Report,
) {
    for (path, row) in rows {
        let Some(before) = previous.get(path) else {
            report.add(
                Rule::BaselineNew,
                rel_baseline,
                row.line,
                format!(
                    "`{path}` has no row at {commit}. A drained row does not return and a new \
                     file may not enter. Make it comply, and remove the row"
                ),
            );
            continue;
        };
        let metrics = [
            ("lines", row.lines, before.lines),
            ("bytes", row.bytes, before.bytes),
            ("line_overflow", row.overflow, before.overflow),
        ];
        for (name, now, then) in metrics {
            let raised = match (now, then) {
                (Some(now), Some(then)) => now > then,
                (Some(_), None) => true,
                (None, _) => false,
            };
            if raised {
                let then = then.map_or("-".to_owned(), |value| value.to_string());
                report.add(
                    Rule::BaselineGrew,
                    rel_baseline,
                    row.line,
                    format!(
                        "`{path}` {name} rose to {} from {then} at {commit}. A shrink is \
                         permanent: cut the file to {then}, or split it into the directory \
                         layout of `.agents/specs.md`",
                        now.unwrap_or_default()
                    ),
                );
            }
        }
        if row.structure.loosens(&before.structure) {
            report.add(
                Rule::BaselineGrew,
                rel_baseline,
                row.line,
                format!(
                    "`{path}` structure is `{}`, wider than `{}` at {commit}. An allowance only \
                     narrows: restore the field and drop the slot",
                    row.structure.cell(),
                    before.structure.cell()
                ),
            );
        }
    }
}

/// Removes a spec's `UNKNOWN` findings and returns the slots they occupy.
fn take_unknowns(structure: &mut Report) -> BTreeSet<String> {
    let mut slots = BTreeSet::new();
    structure.findings.retain(|finding| {
        let unknown = finding.rule == Rule::FieldUnknown;
        if unknown {
            slots.extend(finding.slot.clone());
        }
        !unknown
    });
    slots
}

/// Settles a spec's `UNKNOWN` fields against the slots its `migrated:` row names. A slot
/// the row does not name is a regression, even when another slot was filled in exchange.
fn settle_unknowns(
    rel_baseline: &str,
    path: &str,
    row: &Row,
    structure: &mut Report,
    report: &mut Report,
) {
    let Structure::Migrated(allowed) = &row.structure else {
        return;
    };
    let mut found = take_unknowns(structure);
    // A repeated slot holds no single value yet. The repeat is the finding, not the row.
    let repeated = structure
        .findings
        .iter()
        .filter(|finding| finding.rule == Rule::FieldDuplicate)
        .filter_map(|finding| finding.slot.clone());
    found.extend(repeated.filter(|slot| allowed.contains(slot)));
    if let Some(slot) = found.difference(allowed).next() {
        report.add(
            Rule::BaselineGrew,
            path,
            1,
            format!(
                "`{slot}` holds UNKNOWN and the row does not name it. Fill the field; a migrated \
                 row may only lose slots"
            ),
        );
    } else if found.is_empty() {
        report.add(
            Rule::BaselineStale,
            rel_baseline,
            row.line,
            format!("`{path}` has no UNKNOWN required field; remove its row"),
        );
    } else if found != *allowed {
        report.add(
            Rule::BaselineStale,
            rel_baseline,
            row.line,
            format!(
                "`{path}` holds fewer UNKNOWN fields than its row names; the row must say `{}`",
                Structure::Migrated(found).cell()
            ),
        );
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
    symlinks: &BTreeSet<String>,
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
    check_links(root, &index, symlinks, report);
    for part in &parts {
        check_part_header(part, report);
        check_size(part, report);
        check_links(root, part, symlinks, report);
    }
    let mut structure = Report::default();
    check_structure(&index, &parts, true, Some(history), &mut structure);
    let name = rel.rsplit('/').next().unwrap_or(rel);
    if is_spec_name(name) {
        check_tracker(&index, name, &mut structure);
    }
    // A migrated spec keeps the row of the flat file the seal holds. Each part obeys the
    // caps, so that row has no size to allow.
    if let Some(row) = rows.get(&format!("{rel}.md")) {
        let baseline_rel = format!("{SPEC_ROOT}/{BASELINE_FILE}");
        if row.lines.is_some() || row.bytes.is_some() || row.overflow.is_some() {
            report.add(
                Rule::BaselineRaised,
                &baseline_rel,
                row.line,
                format!("`{rel}` is a directory: its row keeps `-` in all three size columns"),
            );
        }
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
    let fences = doc.lines.iter().filter(|line| line.trim() == "```markdown");
    if body.is_empty() || fences.count() != 1 {
        report.add(
            Rule::Sections,
            TEMPLATE,
            1,
            "the template needs exactly one ```markdown fence".to_owned(),
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

/// True for a path GitHub renders as Markdown.
fn is_markdown_path(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, extension)| {
        MARKDOWN_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
    })
}

/// True for text a reader takes for the spec heading, whatever its emphasis or spacing.
fn marks_spec(text: &str) -> bool {
    let plain: String = text
        .chars()
        .filter(|ch| !"*_`~\\".contains(*ch))
        .flat_map(char::to_lowercase)
        .collect();
    let words: Vec<&str> = plain.split_whitespace().collect();
    words.join(" ").contains(SPEC_SIGNATURE)
}

/// The first line that marks `doc` as a spec. Struck text is not binding in a spec, but
/// here it hides one, so the raw heading speaks too, and so does a block quote's heading.
fn spec_mark(doc: &Doc) -> Option<(usize, String)> {
    let heading = doc
        .headings()
        .iter()
        .find(|heading| marks_spec(&heading.text) || marks_spec(&doc.lines[heading.index]));
    let quoted = || {
        doc.visible.iter().enumerate().find_map(|(index, line)| {
            let line = line.as_deref()?.trim_start();
            let inner = line.strip_prefix('>')?.trim_start_matches(['>', ' ']);
            let (_, title) = atx_heading(inner)?;
            marks_spec(&title).then_some((index, title))
        })
    };
    heading
        .map(|heading| (heading.index, heading.text.clone()))
        .or_else(quoted)
}

/// A spec is the standard's business wherever it lands: outside `docs/specs` no other rule
/// runs on it. A sealed spec's file name marks a move out of the ratchet. A tracked file
/// no rule can read is an unverified input, never proof that it is no spec.
fn check_outside(root: &Path, sealed: &BTreeSet<String>, tracked: &[Tracked], report: &mut Report) {
    let base_name = |path: &str| path.rsplit('/').next().unwrap_or(path).to_owned();
    let sealed_names: BTreeSet<String> = sealed.iter().map(|path| base_name(path)).collect();
    for rel in tracked.iter().map(|file| file.path.as_str()) {
        if rel.starts_with(&format!("{SPEC_ROOT}/")) || !is_markdown_path(rel) {
            continue;
        }
        if sealed_names.contains(&base_name(rel)) {
            report.add(
                Rule::Outside,
                rel,
                1,
                format!("a landed spec has this name; move it back to `{SPEC_ROOT}`"),
            );
            continue;
        }
        let bytes = match fs::read(root.join(rel)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                report.absent_markdown += 1;
                continue;
            }
            Err(error) => {
                report.add(
                    Rule::Unreadable,
                    rel,
                    1,
                    format!("cannot read this tracked file: {error}. Restore it"),
                );
                continue;
            }
        };
        let Ok(raw) = String::from_utf8(bytes) else {
            report.add(
                Rule::Unreadable,
                rel,
                1,
                "this tracked Markdown is not UTF-8, so no rule can read it. Convert it to \
                 UTF-8, or rename it if it is not Markdown"
                    .to_owned(),
            );
            continue;
        };
        if let Some((index, title)) = spec_mark(&Doc::parse(rel, &raw)) {
            report.add(
                Rule::Outside,
                rel,
                index + 1,
                format!(
                    "`{title}` marks a spec, and a spec lives in `{SPEC_ROOT}`. Move the file, or \
                     rename the heading if this is not a spec"
                ),
            );
        }
    }
}

/// Compares the working baseline with its comparison base, and records what it used.
fn check_ratchet(
    baseline_rel: &str,
    rows: &BTreeMap<String, Row>,
    history: &dyn History,
    report: &mut Report,
) -> Result<(), String> {
    let previous = history.previous_baseline(baseline_rel)?;
    let Some(text) = &previous.text else {
        report.ratchet = format!("no baseline at comparison base {}", previous.commit);
        return Ok(());
    };
    let mut parsed = Report::default();
    let before = parse_baseline(baseline_rel, text, &mut parsed);
    if let Some(finding) = parsed.findings.first() {
        return Err(format!(
            "SPEC-STRUCTURE: the baseline at comparison base {} does not parse: {}:{}: {}. Fix \
             the base, or set {BASE_ENV} to a commit whose baseline parses",
            previous.commit, finding.file, finding.line, finding.detail
        ));
    }
    check_previous(baseline_rel, rows, &before, &previous.commit, report);
    report.ratchet = format!("comparison base {}", previous.commit);
    Ok(())
}

fn symlink_set(tracked: &[Tracked]) -> BTreeSet<String> {
    tracked
        .iter()
        .filter(|file| file.symlink)
        .map(|file| file.path.clone())
        .collect()
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
    check_ratchet(&baseline_rel, &rows, history, &mut report)?;
    let tracked = history.tracked_files()?;
    let symlinks = symlink_set(&tracked);

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
            let flat = format!("{name}.md");
            if units.iter().any(|(other, _)| *other == flat) {
                report.add(
                    Rule::Collision,
                    &rel,
                    1,
                    format!(
                        "`{flat}` shadows this directory. One spec has one location: remove one"
                    ),
                );
            }
            check_directory(root, &rel, &rows, history, &symlinks, &mut report)?;
        } else if kind.is_file() && name.ends_with(".md") {
            report.specs += 1;
            let doc = read_doc(root, &rel)?;
            check_links(root, &doc, &symlinks, &mut report);
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
        } else {
            report.add(
                Rule::Unreadable,
                &baseline_rel,
                row.line,
                format!(
                    "cannot read `{path}` at seal {}, so its row has no bound",
                    config.seal
                ),
            );
        }
    }
    check_outside(root, &sealed, &tracked, &mut report);
    check_template(root, &mut report)?;
    report
        .findings
        .sort_by(|left, right| (&left.file, left.line).cmp(&(&right.file, right.line)));
    Ok(report)
}

fn check(root: &Path) -> Result<String, String> {
    let history = GitHistory {
        root,
        base_ref: base_ref(),
    };
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
         {} private-research link(s) not checked, {} tracked Markdown file(s) absent from the \
         worktree; ratchet: {}",
        report.specs,
        report.exempt,
        BASELINE_ROWS,
        report.citations_verified,
        report.citations_unverifiable,
        report.private_links,
        report.absent_markdown,
        report.ratchet
    ))
}

/// Prints the row each landed spec needs today, for a drain or a shrink: exactly the
/// rows `check` accepts. A directory spec keeps the row of the flat file the seal holds.
fn baseline_rows(root: &Path) -> Result<String, String> {
    let history = GitHistory {
        root,
        base_ref: base_ref(),
    };
    baseline_rows_at(root, &history)
}

fn baseline_rows_at(root: &Path, history: &dyn History) -> Result<String, String> {
    let symlinks = symlink_set(&history.tracked_files()?);
    let entries = fs::read_dir(root.join(SPEC_ROOT))
        .map_err(|error| format!("SPEC-STRUCTURE: cannot read `{SPEC_ROOT}`: {error}"))?;
    let mut rows = BTreeMap::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("SPEC-STRUCTURE: `{SPEC_ROOT}`: {error}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry
            .file_type()
            .map_err(|error| format!("SPEC-STRUCTURE: `{name}`: {error}"))?;
        let rel = format!("{SPEC_ROOT}/{name}");
        if kind.is_dir() {
            let mut report = Report::default();
            check_directory(
                root,
                &rel,
                &BTreeMap::new(),
                history,
                &symlinks,
                &mut report,
            )?;
            let unknowns = take_unknowns(&mut report);
            if !unknowns.is_empty() && report.findings.is_empty() {
                let size = Size {
                    lines: 0,
                    bytes: 0,
                    overflow: 0,
                };
                let flat = format!("{rel}.md");
                let row = expected_row(&flat, size, &Structure::Migrated(unknowns));
                rows.insert(flat, row);
            }
        } else if kind.is_file() && name.ends_with(".md") {
            let doc = read_doc(root, &rel)?;
            let mut structure = Report::default();
            check_structure(&doc, &[], false, Some(history), &mut structure);
            let size = measure(&doc);
            let unknowns = take_unknowns(&mut structure);
            let others = structure.findings.len();
            if others > 0
                || !unknowns.is_empty()
                || size.lines > MAX_LINES
                || size.bytes > MAX_BYTES
                || size.overflow > 0
            {
                let cell = if others > 0 {
                    Structure::Exempt
                } else if !unknowns.is_empty() {
                    Structure::Migrated(unknowns)
                } else {
                    Structure::Conforms
                };
                let row = expected_row(&rel, size, &cell);
                rows.insert(rel, row);
            }
        }
    }
    let mut out = format!("{BASELINE_HEADER}\n");
    for row in rows.values() {
        out.push_str(row);
        out.push('\n');
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
