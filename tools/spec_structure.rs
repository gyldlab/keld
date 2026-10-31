//! Enforces the repository spec standard owned by `.agents/specs.md` (#1321).
//!
//! Review rounds on #1280, #1307 and #1311 kept finding dangling references and counts
//! that disagree, because no agent can hold a 2,000-line spec and nothing checked one.
//! This gate enforces only what a machine can decide without false positives: size,
//! layout, stable IDs, required fields, relative links and pin citations. Prose quality
//! (ASD-STE100 sentence length, FACT/INFERRED/UNKNOWN labels, quote accuracy) stays a
//! review duty; `.agents/specs.md` says why.
//!
//! Rules read the rendered page, through `mod rendered`, because a rule on source text can
//! always be passed by a spelling that renders the same. That module owns the source. What
//! a rule still gets from it is named there.
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
    reason = "shared contract module; the rendered view uses only its block primitives"
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
use rendered::{
    Doc, Heading, Inline, MAX_INLINE_BYTES, MAX_LIST_DEPTH, atx_heading, folded, is_delimiter_cell,
    list_marker, render_text, without_references,
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
/// What GFM renders as a checkbox at the start of a list item.
const TASK_BOXES: [&str; 3] = ["[ ]", "[x]", "[X]"];
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
/// Every label a field rule reads.
const ALL_LABELS: [&[&str]; 6] = [
    &HEADER_FIELDS,
    &PART_FIELDS,
    &AC_FIELDS,
    &BLOCKER_FIELDS,
    &EVIDENCE_FIELDS,
    &[TASK_BLOCKERS_FIELD],
];
/// A heading with this text marks a spec, wherever the file sits.
const SPEC_SIGNATURE: &str = "acceptance criteria";
/// File extensions GitHub renders as Markdown, so a spec can land under any of them: every
/// name of the Markdown family in `github/markup` and Linguist.
const MARKDOWN_EXTENSIONS: [&str; 13] = [
    "litcoffee",
    "livemd",
    "markdown",
    "md",
    "mdown",
    "mdwn",
    "mdx",
    "mkd",
    "mkdn",
    "mkdown",
    "ronn",
    "scd",
    "workbook",
];
/// The one header field that can stay `UNKNOWN`, as a `migrated:` slot.
const HEADER_OWNER_SLOT: &str = "header.Owner";
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
    Entity,
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
            Rule::Entity => "ENTITY",
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

/// The rendered view of a Markdown file, and the one owner of its source text.
///
/// `Doc::parse` consumes the source and keeps none of it, so a rule cannot read a raw line:
/// it reads what a reader of the rendered page sees. Blocks parse first, as CommonMark
/// orders them, then each paragraph and heading renders through one inline parser, which
/// decodes references and escapes and drops the markup that shows no text. A link
/// definition renders nothing. Struck text is not binding, so a `Line` loses it and only
/// `Heading::shown` keeps it.
///
/// Three facts about the source stay for structure: `source_bytes`, `source_line_widths`,
/// and `Line::column`, which sets how fields nest. Markup that shows no text also stays, for
/// the rules that check it: link targets, definition and footnote labels, fence languages,
/// and inline tags, which a slot such as `<state>` reads as.
mod rendered {
    use super::markdown_contract::{code_span_at, fence_marker};
    use std::collections::{BTreeMap, BTreeSet};
    use std::ops::Range;

    /// cmark-gfm's block-level tag names, which GitHub uses: an HTML block of these ends at a
    /// blank line. CommonMark 0.31 adds `search` and drops `source`; GitHub does neither.
    const HTML_BLOCK_TAGS: [&str; 63] = [
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
        "section",
        "source",
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
        "ul",
    ];
    /// Nested parentheses cmark accepts in a link destination.
    const MAX_DESTINATION_PARENS: usize = 32;
    /// Columns that keep a line in a footnote body.
    const FOOTNOTE_INDENT: usize = 4;
    /// HTML elements whose raw content runs across blank lines.
    const RAW_HTML_ELEMENTS: [&str; 4] = ["pre", "script", "style", "textarea"];
    /// Any of these ends such a block, whichever element opened it.
    const RAW_HTML_ENDS: [&str; 4] = ["</pre>", "</script>", "</style>", "</textarea>"];

    /// List and quote nesting past this reads as text, so a line of markers stays linear, and
    /// the file is refused, since a heading past it is on the page.
    pub(super) const MAX_LIST_DEPTH: usize = 32;
    /// Inline text one block may hold. The inline parser scans ahead from each construct, so
    /// a block past this is not parsed and its file is refused. It exceeds the byte cap, so a
    /// spec within its caps never meets it.
    pub(super) const MAX_INLINE_BYTES: usize = 64 * 1024;

    /// Every HTML5 entity whose text is ASCII, a space or a character no reader can see. A
    /// name outside this table renders as some other visible character, so it can spell no
    /// label. It stays unresolved, and a candidate heading or label is refused for it.
    const NAMED_ENTITIES: [(&str, &str); 77] = [
        ("AMP", "&"),
        ("ApplyFunction", "\u{2061}"),
        ("DiacriticalGrave", "`"),
        ("GT", ">"),
        ("Hat", "^"),
        ("InvisibleComma", "\u{2063}"),
        ("InvisibleTimes", "\u{2062}"),
        ("LT", "<"),
        ("MediumSpace", "\u{205f}"),
        ("NegativeMediumSpace", "\u{200b}"),
        ("NegativeThickSpace", "\u{200b}"),
        ("NegativeThinSpace", "\u{200b}"),
        ("NegativeVeryThinSpace", "\u{200b}"),
        ("NewLine", "\n"),
        ("NoBreak", "\u{2060}"),
        ("NonBreakingSpace", "\u{a0}"),
        ("QUOT", "\""),
        ("Tab", "\t"),
        ("ThickSpace", "\u{205f}\u{200a}"),
        ("ThinSpace", "\u{2009}"),
        ("UnderBar", "_"),
        ("VerticalLine", "|"),
        ("VeryThinSpace", "\u{200a}"),
        ("ZeroWidthSpace", "\u{200b}"),
        ("af", "\u{2061}"),
        ("amp", "&"),
        ("apos", "'"),
        ("ast", "*"),
        ("bsol", "\\"),
        ("colon", ":"),
        ("comma", ","),
        ("commat", "@"),
        ("dollar", "$"),
        ("emsp", "\u{2003}"),
        ("emsp13", "\u{2004}"),
        ("emsp14", "\u{2005}"),
        ("ensp", "\u{2002}"),
        ("equals", "="),
        ("excl", "!"),
        ("fjlig", "fj"),
        ("grave", "`"),
        ("gt", ">"),
        ("hairsp", "\u{200a}"),
        ("ic", "\u{2063}"),
        ("it", "\u{2062}"),
        ("lbrace", "{"),
        ("lbrack", "["),
        ("lcub", "{"),
        ("lowbar", "_"),
        ("lpar", "("),
        ("lrm", "\u{200e}"),
        ("lsqb", "["),
        ("lt", "<"),
        ("midast", "*"),
        ("nbsp", "\u{a0}"),
        ("num", "#"),
        ("numsp", "\u{2007}"),
        ("percnt", "%"),
        ("period", "."),
        ("plus", "+"),
        ("puncsp", "\u{2008}"),
        ("quest", "?"),
        ("quot", "\""),
        ("rbrace", "}"),
        ("rbrack", "]"),
        ("rcub", "}"),
        ("rlm", "\u{200f}"),
        ("rpar", ")"),
        ("rsqb", "]"),
        ("semi", ";"),
        ("shy", "\u{ad}"),
        ("sol", "/"),
        ("thinsp", "\u{2009}"),
        ("verbar", "|"),
        ("vert", "|"),
        ("zwj", "\u{200d}"),
        ("zwnj", "\u{200c}"),
    ];

    /// True for a character that takes no space on the page, so it cannot make a second label.
    fn is_invisible(ch: char) -> bool {
        matches!(
            ch,
            '\u{ad}'
                | '\u{34f}'
                | '\u{61c}'
                | '\u{180e}'
                | '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{fe00}'..='\u{fe0f}'
                | '\u{feff}'
        )
    }

    /// Rendered inline text: what a reader sees on one line, in reading order.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(super) struct Inline {
        pub(super) text: String,
        /// Ranges of `text` that a code span holds.
        pub(super) code: Vec<Range<usize>>,
        /// Decoded link and image destinations, without percent decoding.
        pub(super) links: Vec<String>,
        /// Ranges of `text` holding a `&name;` that `NAMED_ENTITIES` does not resolve.
        pub(super) unresolved: Vec<Range<usize>>,
        /// Inline HTML tags, which show no text, with their offset in `text`.
        pub(super) tags: Vec<(usize, String)>,
    }

    impl Inline {
        fn push(&mut self, text: &str) -> Range<usize> {
            let start = self.text.len();
            let kept = text.chars().filter(|ch| !is_invisible(*ch));
            self.text.extend(kept);
            start..self.text.len()
        }

        fn append(&mut self, other: Inline) {
            let shift = self.text.len();
            let moved = |range: Range<usize>| range.start + shift..range.end + shift;
            self.text.push_str(&other.text);
            self.code.extend(other.code.into_iter().map(moved));
            self.links.extend(other.links);
            self.unresolved
                .extend(other.unresolved.into_iter().map(moved));
            let tags = other.tags.into_iter();
            self.tags.extend(tags.map(|(at, tag)| (at + shift, tag)));
        }

        /// Without the whitespace around it, which renders as nothing.
        fn trimmed(mut self) -> Inline {
            let start = self.text.len() - self.text.trim_start().len();
            let end = self.text.trim_end().len().max(start);
            let moved = |range: &Range<usize>| {
                range.start.clamp(start, end) - start..range.end.clamp(start, end) - start
            };
            self.code = self.code.iter().map(moved).collect();
            self.unresolved = self.unresolved.iter().map(moved).collect();
            for (at, _) in &mut self.tags {
                *at = (*at).clamp(start, end) - start;
            }
            self.text = self.text[start..end].to_owned();
            self
        }

        /// The text of each code span.
        pub(super) fn code_spans(&self) -> impl Iterator<Item = &str> {
            self.code.iter().map(|range| &self.text[range.clone()])
        }

        /// The text around each unresolved reference, in order.
        pub(super) fn around_unresolved(&self) -> Vec<&str> {
            let mut pieces = Vec::with_capacity(self.unresolved.len() + 1);
            let mut kept = 0;
            for range in &self.unresolved {
                pieces.push(&self.text[kept..range.start]);
                kept = range.end;
            }
            pieces.push(&self.text[kept..]);
            pieces
        }
    }

    /// One line's two readings: `binding` loses struck text, and `shown` keeps it.
    #[derive(Clone, Debug, Default)]
    struct Rendered {
        binding: Inline,
        shown: Inline,
    }

    /// One rendered heading. A setext heading joins its lines.
    #[derive(Debug)]
    pub(super) struct Heading {
        pub(super) index: usize,
        pub(super) level: usize,
        /// The binding title.
        pub(super) title: Inline,
        /// The title on the page, struck text included.
        pub(super) shown: Inline,
    }

    impl Heading {
        pub(super) fn text(&self) -> &str {
            &self.title.text
        }
    }

    /// One fenced code block on the page.
    #[derive(Debug)]
    pub(super) struct CodeFence {
        pub(super) line: usize,
        /// The first word of the info string, decoded.
        pub(super) language: String,
        pub(super) body: String,
        /// A closing marker ends it. Otherwise the end of its container or the file does.
        pub(super) closed: bool,
        /// A block quote holds it, so it is no top-level block of the page.
        pub(super) quoted: bool,
    }

    /// One `[label]: target` definition, which renders nothing.
    #[derive(Debug)]
    pub(super) struct Definition {
        pub(super) line: usize,
        pub(super) label: String,
        pub(super) target: String,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Role {
        Paragraph,
        /// A paragraph's first line, right after a list marker.
        Item,
        Heading,
        /// A block quote holds it, or a lazy line continues one.
        Quoted,
        Row,
    }

    /// What one source line is on the page.
    #[derive(Debug)]
    enum Kind {
        Blank,
        /// It shows nothing: markup, a fence marker, a comment, an HTML block or indented code.
        Hidden,
        Code(String),
        Prose {
            role: Role,
            inline: Inline,
            /// Each cell, when the line opens with `|`, table or not.
            cells: Option<Vec<Inline>>,
        },
    }

    /// One source line in the rendered view.
    #[derive(Debug)]
    pub(super) struct Line {
        column: usize,
        kind: Kind,
    }

    impl Line {
        /// The source column where its text starts. Only nesting reads this.
        pub(super) fn column(&self) -> usize {
            self.column
        }

        pub(super) fn is_blank(&self) -> bool {
            match &self.kind {
                Kind::Blank => true,
                Kind::Code(text) => is_blank(text),
                _ => false,
            }
        }

        /// The line of a fenced code block.
        pub(super) fn code(&self) -> Option<&str> {
            match &self.kind {
                Kind::Code(text) => Some(text),
                _ => None,
            }
        }

        /// Its rendered text, or `None` when the line shows nothing.
        pub(super) fn prose(&self) -> Option<&Inline> {
            match &self.kind {
                Kind::Prose { inline, .. } if !inline.text.is_empty() => Some(inline),
                _ => None,
            }
        }

        /// Text that can state a field: no heading, no quote and no table row can.
        pub(super) fn field(&self) -> Option<&Inline> {
            match &self.kind {
                Kind::Prose {
                    role: Role::Paragraph | Role::Item,
                    ..
                } => self.prose(),
                _ => None,
            }
        }

        /// True on the first line of a list item.
        pub(super) fn is_item(&self) -> bool {
            matches!(
                self.kind,
                Kind::Prose {
                    role: Role::Item,
                    ..
                }
            )
        }

        pub(super) fn cells(&self) -> Option<&[Inline]> {
            match &self.kind {
                Kind::Prose { cells, .. } => cells.as_deref(),
                _ => None,
            }
        }
    }

    /// One Markdown file as a reader sees it. It holds no source text.
    #[derive(Debug)]
    pub(super) struct Doc {
        pub(super) rel: String,
        lines: Vec<Line>,
        headings: Vec<Heading>,
        quoted_headings: Vec<Heading>,
        fences: Vec<CodeFence>,
        definitions: Vec<Definition>,
        /// Each `[^label]:` footnote line, which a reference may not repeat.
        footnotes: Vec<(usize, String)>,
        bytes: usize,
        widths: Vec<usize>,
        /// A fence that never closes hides the rest of the file from every other rule.
        pub(super) unclosed_fence: Option<usize>,
        /// So does a comment or raw HTML block.
        pub(super) unclosed_raw: Option<usize>,
        /// The first line past a bound, which no rule read.
        pub(super) unread: Option<usize>,
        /// The first tag that continues a quote or list here and opens an HTML block on
        /// GitHub. CommonMark and cmark-gfm disagree, so the text after it has no one reading.
        pub(super) lazy_tag: Option<usize>,
    }

    impl Doc {
        pub(super) fn parse(rel: &str, raw: &str) -> Doc {
            // CommonMark ends a line at LF, CR or CRLF, and `str::lines` splits only on LF.
            let text = raw
                .strip_prefix('\u{feff}')
                .unwrap_or(raw)
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            let mut lexer = Lexer::default();
            let mut lines = Vec::new();
            let mut widths = Vec::new();
            for (index, line) in text.lines().enumerate() {
                let kind = lexer.step(index, line);
                widths.push(line.chars().count());
                lines.push(Line {
                    column: indent(line),
                    kind,
                });
            }
            let unclosed_fence = lexer.fence.as_ref().map(|fence| fence.line);
            let unclosed_raw = lexer.raw.map(|raw| raw.line);
            let mut blocks = lexer.finish();
            let unread = blocks.bound_inline();
            let definitions = take_definitions(&mut blocks.paragraphs);
            let footnotes = std::mem::take(&mut blocks.notes);
            let links = definitions
                .iter()
                .rev()
                .map(|definition| (definition.label.as_str(), definition.target.as_str()))
                .collect();
            let notes = footnotes.iter().map(|(_, label)| label.as_str()).collect();
            let refs = Refs { links, notes };
            let mut headings = blocks.headings;
            for paragraph in blocks.paragraphs {
                match paragraph.setext {
                    Some(level) if !paragraph.lines.is_empty() => {
                        headings.push(HeadingBlock {
                            level,
                            quoted: paragraph.quoted,
                            lines: paragraph.lines,
                        });
                    }
                    _ => render_lines(&paragraph.lines, paragraph.quoted, &refs, &mut lines),
                }
            }
            let mut quoted_headings = Vec::new();
            let mut plain = Vec::new();
            for block in headings {
                let quoted = block.quoted;
                let heading = render_heading(block, &refs, &mut lines);
                if quoted {
                    quoted_headings.push(heading);
                } else {
                    plain.push(heading);
                }
            }
            plain.sort_by_key(|heading| heading.index);
            quoted_headings.sort_by_key(|heading| heading.index);
            let mut fences = blocks.fences;
            fences.sort_by_key(|fence| fence.line);
            Doc {
                rel: rel.to_owned(),
                lines,
                headings: plain,
                quoted_headings,
                fences,
                definitions,
                footnotes,
                bytes: text.len(),
                widths,
                unclosed_fence,
                unclosed_raw,
                unread,
                lazy_tag: blocks.lazy_tag,
            }
        }

        pub(super) fn lines(&self) -> &[Line] {
            &self.lines
        }

        pub(super) fn line(&self, index: usize) -> &Line {
            &self.lines[index]
        }

        pub(super) fn line_count(&self) -> usize {
            self.lines.len()
        }

        pub(super) fn headings(&self) -> &[Heading] {
            &self.headings
        }

        /// Headings a block quote holds: they show on the page, and they define nothing.
        pub(super) fn quoted_headings(&self) -> &[Heading] {
            &self.quoted_headings
        }

        pub(super) fn fences(&self) -> &[CodeFence] {
            &self.fences
        }

        /// Every link target with its line, definitions included, in line order.
        pub(super) fn targets(&self) -> Vec<(usize, &str)> {
            let mut targets: Vec<(usize, &str)> = Vec::new();
            for (index, line) in self.lines.iter().enumerate() {
                if let Kind::Prose { inline, .. } = &line.kind {
                    targets.extend(inline.links.iter().map(|link| (index, link.as_str())));
                }
            }
            let defined = self.definitions.iter();
            targets.extend(defined.map(|found| (found.line, found.target.as_str())));
            targets.sort_by_key(|(index, _)| *index);
            targets
        }

        /// Each definition and footnote label with its line, in line order.
        pub(super) fn labels(&self) -> Vec<(usize, &str)> {
            let mut labels: Vec<(usize, &str)> = self
                .definitions
                .iter()
                .map(|found| (found.line, found.label.as_str()))
                .collect();
            let notes = self.footnotes.iter();
            labels.extend(notes.map(|(line, label)| (*line, label.as_str())));
            labels.sort_by_key(|(line, _)| *line);
            labels
        }

        /// Lines after heading `at` up to the next heading of the same or a higher level.
        pub(super) fn body_end(&self, headings: &[Heading], at: usize) -> usize {
            headings[at + 1..]
                .iter()
                .find(|heading| heading.level <= headings[at].level)
                .map_or(self.lines.len(), |heading| heading.index)
        }

        /// The file's size in bytes, for the byte caps. A cap reads the source, not the page.
        pub(super) fn source_bytes(&self) -> usize {
            self.bytes
        }

        /// Characters on each source line, for the line-length cap, which counts fences too.
        pub(super) fn source_line_widths(&self) -> &[usize] {
            &self.widths
        }
    }

    /// An open fence, which collects its body.
    struct OpenFence {
        marker: u8,
        width: usize,
        line: usize,
        /// Column of the list item that holds it. A fence ends with its item.
        base: usize,
        /// Spaces before its opening marker, which each body line loses.
        indent: usize,
        language: String,
        body: String,
    }

    /// An open comment or raw HTML block.
    #[derive(Clone, Copy)]
    struct RawBlock {
        ends: &'static [&'static str],
        line: usize,
        /// Column of the list item that holds it. The block ends with its item.
        base: usize,
    }

    /// How a paragraph line got there, which decides what it can be in a table.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Origin {
        Plain,
        /// The first line of a list item.
        Item,
        /// Indented four columns or more: text in a paragraph, code past a table.
        Deep,
        /// Outside the containers of its paragraph: no delimiter row, no table row.
        Lazy,
    }

    /// One paragraph line, without its list marker and indentation.
    struct SourceLine {
        index: usize,
        text: String,
        origin: Origin,
    }

    impl SourceLine {
        fn plain(index: usize, text: &str) -> SourceLine {
            SourceLine {
                index,
                text: text.to_owned(),
                origin: Origin::Plain,
            }
        }
    }

    /// An open paragraph, which a setext underline turns into a heading.
    #[derive(Default)]
    struct Paragraph {
        lines: Vec<SourceLine>,
        /// A block quote or a footnote holds it: lazy text continues it, and it states no field.
        quoted: bool,
        setext: Option<usize>,
        /// Its last lines opened a GFM table, which is a block of its own from there on: no
        /// lazy line, no list that only a paragraph refuses, and code after four columns.
        table: bool,
    }

    impl Paragraph {
        /// True when an underline has text to underline: no definitions only, and no table.
        /// Each such line continues the block instead.
        fn takes_underline(&self) -> bool {
            let text = joined_lines(&self.lines);
            let mut rest = text.as_str();
            while let Some((_, _, end)) = definition_at(rest) {
                rest = &rest[end..];
            }
            !rest.is_empty() && !self.table
        }

        /// True when definitions took the whole paragraph: cmark keeps an underline as text
        /// here, `---` included, so it opens no thematic break.
        fn definitions_only(&self) -> bool {
            let text = joined_lines(&self.lines);
            let opens = definition_at(&text).is_some();
            opens && !self.takes_underline() && !self.table
        }

        /// The most inline text one render of this block reads: a table renders each row
        /// alone.
        fn inline_bytes(&self) -> usize {
            let start = table_start(&self.lines).unwrap_or(self.lines.len());
            let (text, rows) = self.lines.split_at(start);
            let widest = rows.iter().map(|row| row.text.len()).max();
            let breaks = text.len().saturating_sub(1);
            let joined: usize = text.iter().map(|line| line.text.len()).sum();
            (joined + breaks).max(widest.unwrap_or(0))
        }
    }

    struct HeadingBlock {
        level: usize,
        quoted: bool,
        lines: Vec<SourceLine>,
    }

    /// Blocks in the order they closed.
    #[derive(Default)]
    struct Blocks {
        paragraphs: Vec<Paragraph>,
        headings: Vec<HeadingBlock>,
        fences: Vec<CodeFence>,
        /// Each `[^label]:` that opens a footnote, with its line.
        notes: Vec<(usize, String)>,
        unread: Option<usize>,
        lazy_tag: Option<usize>,
    }

    impl Blocks {
        /// Takes over what a block quote held.
        fn absorb(&mut self, mut quote: Blocks) {
            self.paragraphs.append(&mut quote.paragraphs);
            self.headings.append(&mut quote.headings);
            self.fences.append(&mut quote.fences);
            self.notes.append(&mut quote.notes);
            self.unread = earliest(self.unread, quote.unread);
            self.lazy_tag = earliest(self.lazy_tag, quote.lazy_tag);
        }

        /// Drops each block past `MAX_INLINE_BYTES`, and returns the first line no rule reads.
        fn bound_inline(&mut self) -> Option<usize> {
            let mut unread = self.unread;
            self.paragraphs.retain(|paragraph| {
                let over = paragraph.inline_bytes() > MAX_INLINE_BYTES;
                let line = paragraph.lines.first().map(|line| line.index);
                unread = earliest(unread, line.filter(|_| over));
                !over
            });
            self.headings.retain(|heading| {
                let over = joined_lines(&heading.lines).len() > MAX_INLINE_BYTES;
                let line = heading.lines.first().map(|line| line.index);
                unread = earliest(unread, line.filter(|_| over));
                !over
            });
            unread
        }
    }

    /// The first of two lines.
    fn earliest(left: Option<usize>, right: Option<usize>) -> Option<usize> {
        match (left, right) {
            (Some(left), Some(right)) => Some(left.min(right)),
            _ => left.or(right),
        }
    }

    /// The CommonMark block structure, one line at a time: list items for their columns,
    /// quotes, paragraphs, fences, indented code, HTML blocks and comments. A block quote is
    /// a nested lexer over the text after its marker.
    #[derive(Default)]
    struct Lexer {
        /// Content column of each open list item, outermost first.
        items: Vec<usize>,
        paragraph: Option<Paragraph>,
        fence: Option<OpenFence>,
        raw: Option<RawBlock>,
        /// Column of the list item holding an open HTML block, which a blank line ends.
        html: Option<usize>,
        quote: Option<Box<Lexer>>,
        /// The last line opened a list item with nothing in it, which a blank line ends.
        empty_item: bool,
        /// Block quotes around this lexer.
        depth: usize,
        /// The entry of `items` that is the outermost open footnote.
        note: Option<usize>,
        blocks: Blocks,
    }

    impl Lexer {
        fn step(&mut self, index: usize, line: &str) -> Kind {
            let column = indent(line);
            let blank = is_blank(line);
            if let Some(raw) = self.raw {
                if blank || column >= raw.base {
                    if closes_raw(line, raw.ends) {
                        self.raw = None;
                    }
                    return Kind::Hidden;
                }
                self.raw = None;
            }
            if let Some(fence) = &mut self.fence {
                if blank || column >= fence.base {
                    let inner = dedent(line, fence.base);
                    if fence_marker(&inner).is_some_and(|(marker, width, closing)| {
                        marker == fence.marker && width >= fence.width && closing
                    }) {
                        self.end_fence(true);
                        return Kind::Hidden;
                    }
                    let code = without_columns(line, fence.base + fence.indent);
                    fence.body.push_str(&code);
                    fence.body.push('\n');
                    return Kind::Code(code);
                }
                self.end_fence(false);
            }
            if blank {
                self.close_quote();
                self.html = None;
                self.close_paragraph();
                if std::mem::take(&mut self.empty_item) {
                    self.items.pop();
                }
                return Kind::Blank;
            }
            self.empty_item = false;
            if let Some(html) = self.html {
                if column >= html {
                    return Kind::Hidden;
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
            if self.quote.is_some() {
                let body = text.trim_start_matches([' ', '\t']);
                let inside =
                    depth == self.items.len() && indent(&text) < 4 && body.starts_with('>');
                if !inside {
                    if self.quote.as_ref().is_some_and(|quote| quote.lazy_open())
                        && !starts_block(&text)
                    {
                        self.note_lazy_tag(index, &text);
                        self.lazy_line(index, trim_spaces(&text));
                        return Kind::Hidden;
                    }
                    self.close_quote();
                }
            }
            if depth < self.items.len() {
                if self.lazy_open() && !starts_block(&text) {
                    self.note_lazy_tag(index, &text);
                    return self.paragraph_line(index, trim_spaces(&text), Origin::Lazy);
                }
                self.items.truncate(depth);
                self.note = self.note.filter(|at| *at < depth);
                self.close_paragraph();
            }
            self.leaf(index, text, base)
        }

        fn leaf(&mut self, index: usize, mut text: String, mut base: usize) -> Kind {
            let mut origin = Origin::Plain;
            loop {
                let column = indent(&text);
                let body = text.trim_start_matches([' ', '\t']);
                // A table is no paragraph: what only a paragraph refuses opens here.
                let in_text = self.in_text();
                if column >= 4 {
                    if in_text {
                        return self.paragraph_line(index, trim_spaces(body), Origin::Deep);
                    }
                    self.close_paragraph();
                    return Kind::Hidden;
                }
                if let Some(level) = setext_level(body)
                    && let Some(paragraph) = &mut self.paragraph
                    && paragraph.takes_underline()
                {
                    paragraph.setext = Some(level);
                    self.close_paragraph();
                    return Kind::Hidden;
                }
                let kept = setext_level(body).is_some()
                    && self
                        .paragraph
                        .as_ref()
                        .is_some_and(Paragraph::definitions_only);
                if !kept && is_thematic(body) {
                    self.close_paragraph();
                    return Kind::Hidden;
                }
                if body.starts_with('>') {
                    if self.depth < MAX_LIST_DEPTH {
                        return self.quote_line(index, body, base + column);
                    }
                    self.blocks.unread = earliest(self.blocks.unread, Some(index));
                }
                // A comment opening the line is a block, so what follows it is no heading.
                if let Some(ends) = raw_block_ends(body) {
                    self.close_paragraph();
                    if !closes_raw(body, ends) {
                        self.raw = Some(RawBlock {
                            ends,
                            line: index,
                            base,
                        });
                    }
                    return Kind::Hidden;
                }
                if let Some((level, title)) = atx_heading(body) {
                    self.close_paragraph();
                    self.blocks.headings.push(HeadingBlock {
                        level,
                        quoted: self.quoted(),
                        lines: vec![SourceLine::plain(index, &title)],
                    });
                    return Kind::Hidden;
                }
                if html_block_start(body, in_text) {
                    self.close_paragraph();
                    self.html = Some(base);
                    return Kind::Hidden;
                }
                if let Some((marker, width, _)) = fence_marker(&text) {
                    self.close_paragraph();
                    let info = unescaped(trim_spaces(&body[width..]));
                    let language = info.split_whitespace().next().unwrap_or("");
                    self.fence = Some(OpenFence {
                        marker,
                        width,
                        line: index,
                        base,
                        indent: column,
                        language: language.to_owned(),
                        body: String::new(),
                    });
                    return Kind::Hidden;
                }
                let opens = footnote_at(body).is_some() || list_marker(body, in_text).is_some();
                if opens && self.items.len() >= MAX_LIST_DEPTH {
                    self.blocks.unread = earliest(self.blocks.unread, Some(index));
                    return self.paragraph_line(index, trim_spaces(body), origin);
                }
                if let Some((label, end)) = footnote_at(body) {
                    self.close_paragraph();
                    self.blocks.notes.push((index, folded(label)));
                    self.note.get_or_insert(self.items.len());
                    base += FOOTNOTE_INDENT;
                    self.items.push(base);
                    text = trim_spaces(&body[end..]).to_owned();
                    if text.is_empty() {
                        return Kind::Hidden;
                    }
                    continue;
                }
                let Some(marker) = list_marker(body, in_text) else {
                    return self.paragraph_line(index, trim_spaces(body), origin);
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
                self.close_paragraph();
                if is_blank(&rest) {
                    self.empty_item = true;
                    return Kind::Hidden;
                }
                text = rest;
                origin = Origin::Item;
            }
        }

        /// A quote opens or continues: the rest of the line is the nested lexer's. A nested
        /// fence or blank line shows as neither here, so no rule reads a quote as one.
        fn quote_line(&mut self, index: usize, body: &str, column: usize) -> Kind {
            self.close_paragraph();
            let rest = &body[1..];
            let spaces = indent_from(rest, column + 1);
            let content = rest.trim_start_matches([' ', '\t']);
            let text = format!("{}{content}", " ".repeat(spaces.saturating_sub(1)));
            let depth = self.depth + 1;
            let quote = self.quote.get_or_insert_with(|| {
                Box::new(Lexer {
                    depth,
                    ..Lexer::default()
                })
            });
            quote.step(index, &text);
            Kind::Hidden
        }

        /// True inside a block quote or a footnote, which define nothing and state no field.
        fn quoted(&self) -> bool {
            self.depth > 0 || self.note.is_some()
        }

        /// True while the open block is a paragraph, and no table it became.
        fn in_text(&self) -> bool {
            self.paragraph.as_ref().is_some_and(|open| !open.table)
        }

        fn lazy_open(&self) -> bool {
            self.in_text() || self.quote.as_ref().is_some_and(|quote| quote.lazy_open())
        }

        /// Records a tag that is lazy text here and opens an HTML block on GitHub.
        fn note_lazy_tag(&mut self, index: usize, text: &str) {
            let body = text.trim_start_matches([' ', '\t']);
            if indent(text) < 4 && html_block_start(body, false) {
                self.blocks.lazy_tag = earliest(self.blocks.lazy_tag, Some(index));
            }
        }

        fn lazy_line(&mut self, index: usize, text: &str) {
            if let Some(quote) = &mut self.quote {
                quote.lazy_line(index, text);
            } else {
                self.paragraph_line(index, text, Origin::Lazy);
            }
        }

        fn paragraph_line(&mut self, index: usize, text: &str, origin: Origin) -> Kind {
            let quoted = self.quoted();
            let paragraph = self.paragraph.get_or_insert_with(|| Paragraph {
                quoted,
                ..Paragraph::default()
            });
            paragraph.lines.push(SourceLine {
                index,
                text: text.to_owned(),
                origin,
            });
            if let [.., header, delimiter] = &paragraph.lines[..] {
                paragraph.table |= opens_table(header, delimiter);
            }
            Kind::Hidden
        }

        fn close_paragraph(&mut self) {
            if let Some(paragraph) = self.paragraph.take() {
                self.blocks.paragraphs.push(paragraph);
            }
        }

        fn close_quote(&mut self) {
            if let Some(quote) = self.quote.take() {
                self.blocks.absorb(quote.finish());
            }
        }

        fn end_fence(&mut self, closed: bool) {
            if let Some(fence) = self.fence.take() {
                self.blocks.fences.push(CodeFence {
                    line: fence.line,
                    language: fence.language,
                    body: fence.body,
                    closed,
                    quoted: self.quoted(),
                });
            }
        }

        fn finish(mut self) -> Blocks {
            self.close_quote();
            self.close_paragraph();
            self.end_fence(false);
            self.blocks
        }
    }

    fn is_blank(line: &str) -> bool {
        line.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
    }

    /// `text` without the spaces and tabs around it. A no-break space is text.
    fn trim_spaces(text: &str) -> &str {
        text.trim_matches([' ', '\t'])
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

    /// `line` without its first `columns` columns of whitespace. Code keeps its tabs, and a
    /// tab cut in two leaves spaces.
    fn without_columns(line: &str, columns: usize) -> String {
        let mut column = 0;
        for (at, byte) in line.bytes().enumerate() {
            if column >= columns {
                return format!("{}{}", " ".repeat(column - columns), &line[at..]);
            }
            match byte {
                b' ' => column += 1,
                b'\t' => column += 4 - column % 4,
                _ => return line[at..].to_owned(),
            }
        }
        " ".repeat(column.saturating_sub(columns))
    }

    /// True for a line outside a paragraph's containers that ends the paragraph. No container
    /// holds that paragraph here, so the rule that a list cannot interrupt one does not bind.
    fn starts_block(text: &str) -> bool {
        let body = text.trim_start_matches([' ', '\t']);
        indent(text) < 4
            && (is_thematic(body)
                || body.starts_with('>')
                || atx_heading(body).is_some()
                || raw_block_ends(body).is_some()
                || html_block_start(body, true)
                || fence_marker(text).is_some()
                || footnote_at(body).is_some()
                || list_marker(body, false).is_some())
    }

    /// CommonMark ATX heading: up to three spaces, one to six `#`, then a space, a tab or the
    /// end of the line. A closing `#` run is not part of the title.
    pub(super) fn atx_heading(line: &str) -> Option<(usize, String)> {
        let text = line.trim_start_matches(' ');
        if line.len() - text.len() > 3 {
            return None;
        }
        let level = text.bytes().take_while(|byte| *byte == b'#').count();
        let rest = &text[level..];
        if !(1..=6).contains(&level) || !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
            return None;
        }
        let title = trim_spaces(rest);
        let open = title.trim_end_matches('#');
        let title = if open.is_empty() {
            ""
        } else if open.ends_with([' ', '\t']) {
            trim_spaces(open)
        } else {
            title
        };
        Some((level, title.to_owned()))
    }

    /// The heading level of a setext underline.
    fn setext_level(body: &str) -> Option<usize> {
        let underline = body.trim_end_matches([' ', '\t']);
        if !underline.is_empty() && underline.bytes().all(|byte| byte == b'=') {
            Some(1)
        } else if !underline.is_empty() && underline.bytes().all(|byte| byte == b'-') {
            Some(2)
        } else {
            None
        }
    }

    pub(super) fn is_thematic(body: &str) -> bool {
        let mut marks = body.bytes().filter(|byte| !matches!(byte, b' ' | b'\t'));
        let first = marks.next();
        matches!(first, Some(b'-' | b'*' | b'_'))
            && marks.try_fold(1_usize, |count, byte| {
                (Some(byte) == first).then_some(count + 1)
            }) >= Some(3)
    }

    /// Byte width of a list marker. Inside a paragraph only a nonempty bullet or `1.` opens a
    /// list, so `2024. was a year` stays prose.
    pub(super) fn list_marker(body: &str, interrupts: bool) -> Option<usize> {
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
            || (!is_blank(after) && (digits == 0 || body[..digits].parse() == Ok(1_usize)));
        (spaced && can_open).then_some(width)
    }

    /// The label, with its `^`, of the footnote definition opening `body`, and the offset past
    /// its colon. A footnote body is a block container, as a list item is.
    fn footnote_at(body: &str) -> Option<(&str, usize)> {
        let inner = body.strip_prefix("[^")?;
        let width = inner.find([']', ' ', '\t', '\0'])?;
        (width > 0 && inner[width..].starts_with("]:")).then(|| (&body[1..2 + width], width + 4))
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
        // cmark-gfm, as GitHub: an upper-case letter opens a declaration, and `<!x` is text.
        if declaration.is_some_and(|rest| rest.starts_with(|ch: char| ch.is_ascii_uppercase())) {
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
        let lone_tag = html_end(body).is_some_and(|end| is_blank(&body[end..]));
        HTML_BLOCK_TAGS.contains(&name.as_str()) || (lone_tag && !in_paragraph)
    }

    /// `text` as CommonMark matches labels: only spaces and tabs collapse, and lower case
    /// comes first so `\u{1e9e}` meets `SS`. The last step only keeps a message readable.
    pub(super) fn folded(text: &str) -> String {
        let words: Vec<&str> = text
            .split([' ', '\t', '\n'])
            .filter(|word| !word.is_empty())
            .collect();
        words.join(" ").to_lowercase().to_uppercase().to_lowercase()
    }

    /// The label at `text`, which opens with `[`, and the offset past its `]`.
    fn link_label(text: &str) -> Option<(&str, usize)> {
        if !text.starts_with('[') {
            return None;
        }
        let bytes = text.as_bytes();
        let mut at = 1;
        while at < bytes.len() {
            match bytes[at] {
                b'\\' => at += 2,
                b'[' => return None,
                b']' => return Some((&text[1..at], at + 1)),
                _ => at += 1,
            }
        }
        None
    }

    /// The offset past spaces, tabs and at most one line ending.
    fn after_gap(text: &str, at: usize) -> usize {
        let mut end = at;
        let mut lines = 0;
        for byte in text[at..].bytes() {
            match byte {
                b' ' | b'\t' => end += 1,
                b'\n' if lines == 0 => {
                    lines = 1;
                    end += 1;
                }
                _ => break,
            }
        }
        end
    }

    /// True when a backslash before `rest` escapes: only ASCII punctuation can follow one.
    fn is_escapable(rest: &str) -> bool {
        rest.starts_with(|ch: char| ch.is_ascii_punctuation())
    }

    /// The raw destination opening `text`, and the offset past it. A title or a stray
    /// parenthesis never decides where the next link starts.
    fn link_destination(text: &str) -> Option<(&str, usize)> {
        if let Some(inner) = text.strip_prefix('<') {
            let mut escaped = false;
            for (offset, ch) in inner.char_indices() {
                match ch {
                    '\n' => return None,
                    _ if escaped => escaped = false,
                    '\\' => escaped = is_escapable(&inner[offset + 1..]),
                    '<' => return None,
                    '>' => return Some((&inner[..offset], offset + 2)),
                    _ => {}
                }
            }
            return None;
        }
        let mut depth = 0_usize;
        let mut escaped = false;
        for (offset, ch) in text.char_indices() {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = is_escapable(&text[offset + 1..]);
            } else if ch == '(' {
                depth += 1;
                if depth > MAX_DESTINATION_PARENS {
                    return None;
                }
            } else if ch == ')' && depth > 0 {
                depth -= 1;
            } else if ch == ')' || ch.is_ascii_whitespace() || ch.is_ascii_control() {
                return (depth == 0).then_some((&text[..offset], offset));
            }
        }
        (depth == 0).then_some((text, text.len()))
    }

    /// The offset past the link title opening `text`.
    fn link_title(text: &str) -> Option<usize> {
        let close = match text.as_bytes().first()? {
            b'"' => b'"',
            b'\'' => b'\'',
            b'(' => b')',
            _ => return None,
        };
        let bytes = text.as_bytes();
        let mut at = 1;
        while at < bytes.len() {
            match bytes[at] {
                b'\\' => at += 2,
                byte if byte == close => return Some(at + 1),
                b'(' if close == b')' => return None,
                _ => at += 1,
            }
        }
        None
    }

    /// The offset past the end of the line holding `at`, when only spaces remain on it.
    fn line_end(text: &str, at: usize) -> Option<usize> {
        let end = text[at..].find('\n').map_or(text.len(), |found| at + found);
        is_blank(&text[at..end]).then(|| (end + 1).min(text.len()))
    }

    /// One `[label]: target` opening `text`: label, raw target, and the offset past it. Prose
    /// makes none. A footnote opened its block before any paragraph, so a `[^a b]:` here,
    /// which opens no footnote, is a definition on GitHub.
    fn definition_at(text: &str) -> Option<(&str, &str, usize)> {
        let (label, end) = link_label(text)?;
        if label.trim().is_empty() || !text[end..].starts_with(':') {
            return None;
        }
        let start = after_gap(text, end + 1);
        let (target, width) = link_destination(&text[start..])?;
        let after = start + width;
        if target.is_empty() && !text[start..].starts_with('<') {
            return None;
        }
        let gap = after_gap(text, after);
        let titled = link_title(&text[gap..])
            .filter(|_| gap > after)
            .and_then(|title| line_end(text, gap + title));
        Some((label, target, titled.or_else(|| line_end(text, after))?))
    }

    /// Moves each definition out of the paragraphs that open with it, as CommonMark does: it
    /// shows nothing. GitHub keeps the text of a paragraph that a table ends.
    fn take_definitions(paragraphs: &mut [Paragraph]) -> Vec<Definition> {
        let mut definitions = Vec::new();
        for paragraph in paragraphs.iter_mut().filter(|open| !open.table) {
            let text = joined_lines(&paragraph.lines);
            let mut rest = text.as_str();
            let mut used = 0;
            while let Some((label, target, end)) = definition_at(rest) {
                let lines = rest[..end].matches('\n').count();
                let lines = lines + usize::from(!rest[..end].ends_with('\n'));
                definitions.push(Definition {
                    line: paragraph.lines[used].index,
                    label: folded(label),
                    target: unescaped(target),
                });
                used += lines;
                rest = &rest[end..];
            }
            paragraph.lines.drain(..used);
        }
        definitions
    }

    fn joined_lines(lines: &[SourceLine]) -> String {
        let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
        texts.join("\n")
    }

    /// The cells of a table row. GFM splits on every pipe that no backslash escapes, and
    /// that backslash leaves the cell before inline parsing, even in a code span.
    pub(super) fn split_cells(row: &str) -> Vec<String> {
        let mut cells = vec![String::new()];
        let mut chars = row.chars().peekable();
        while let Some(ch) = chars.next() {
            let Some(cell) = cells.last_mut() else { break };
            match (ch, chars.peek()) {
                ('\\', Some('|')) => {
                    cell.push('|');
                    chars.next();
                }
                ('\\', Some(next)) => {
                    cell.push(ch);
                    cell.push(*next);
                    chars.next();
                }
                ('|', _) => cells.push(String::new()),
                _ => cell.push(ch),
            }
        }
        if cells.first().is_some_and(|cell| is_blank(cell)) {
            cells.remove(0);
        }
        if cells.len() > 1 && cells.last().is_some_and(|cell| is_blank(cell)) {
            cells.pop();
        }
        cells
            .iter()
            .map(|cell| trim_spaces(cell).to_owned())
            .collect()
    }

    /// True for a GFM delimiter cell: `---`, with a colon at either end.
    pub(super) fn is_delimiter_cell(cell: &str) -> bool {
        let dashes = cell.strip_prefix(':').unwrap_or(cell);
        let dashes = dashes.strip_suffix(':').unwrap_or(dashes);
        !dashes.is_empty() && dashes.bytes().all(|byte| byte == b'-')
    }

    /// The first row of a table, which GFM lets end a paragraph: a row, then a delimiter row
    /// of the same width. GitHub needs no pipe in either: `a` over `:--` renders a table.
    fn table_start(lines: &[SourceLine]) -> Option<usize> {
        lines
            .windows(2)
            .position(|pair| opens_table(&pair[0], &pair[1]))
    }

    /// cmark tries an underline before a table, so a lone `---` never opens one.
    fn opens_table(header: &SourceLine, delimiter: &SourceLine) -> bool {
        let cells = split_cells(&delimiter.text);
        !matches!(delimiter.origin, Origin::Deep | Origin::Lazy)
            && setext_level(&delimiter.text).is_none()
            && cells.iter().all(|cell| is_delimiter_cell(cell))
            && split_cells(&header.text).len() == cells.len()
    }

    /// Renders the lines of one paragraph, which GFM may end with a table.
    fn render_lines(source: &[SourceLine], quoted: bool, refs: &Refs, lines: &mut [Line]) {
        let table = table_start(source);
        let (text, rows) = source.split_at(table.unwrap_or(source.len()));
        let rendered = render(&joined_lines(text), refs);
        for (line, shown) in text.iter().zip(rendered) {
            let role = if quoted {
                Role::Quoted
            } else if line.origin == Origin::Item {
                Role::Item
            } else {
                Role::Paragraph
            };
            lines[line.index].kind = Kind::Prose {
                role,
                inline: shown.binding,
                cells: line
                    .text
                    .starts_with('|')
                    .then(|| rendered_cells(&line.text, refs)),
            };
        }
        for (at, row) in rows.iter().enumerate() {
            let cells = rendered_cells(&row.text, refs);
            let mut inline = Inline::default();
            // The delimiter row shows nothing, and a rule still reads its cells.
            if at != 1 {
                inline.push("|");
                for cell in cells.iter().cloned() {
                    inline.push(" ");
                    inline.append(cell);
                    inline.push(" |");
                }
            }
            lines[row.index].kind = Kind::Prose {
                role: Role::Row,
                inline,
                cells: Some(cells),
            };
        }
    }

    fn rendered_cells(row: &str, refs: &Refs) -> Vec<Inline> {
        split_cells(row)
            .iter()
            .map(|cell| render_line(cell, refs).binding)
            .collect()
    }

    fn render_heading(block: HeadingBlock, refs: &Refs, lines: &mut [Line]) -> Heading {
        let mut title = Inline::default();
        let mut shown = Inline::default();
        let rendered = render(&joined_lines(&block.lines), refs);
        for (at, (line, inline)) in block.lines.iter().zip(rendered).enumerate() {
            if at > 0 {
                title.push(" ");
                shown.push(" ");
            }
            title.append(inline.binding.clone());
            shown.append(inline.shown);
            lines[line.index].kind = Kind::Prose {
                role: Role::Heading,
                inline: inline.binding,
                cells: None,
            };
        }
        Heading {
            index: block.lines.first().map_or(0, |line| line.index),
            level: block.level,
            title: title.trimmed(),
            shown: shown.trimmed(),
        }
    }

    /// What the file defines, which decides whether brackets are a link or a footnote mark.
    #[derive(Default)]
    struct Refs<'a> {
        links: BTreeMap<&'a str, &'a str>,
        notes: BTreeSet<&'a str>,
    }

    /// Renders one piece of text that has no line break and no link definition around it.
    pub(super) fn render_text(source: &str) -> Inline {
        render_line(source, &Refs::default()).binding
    }

    fn render_line(source: &str, refs: &Refs) -> Rendered {
        let mut all = Rendered::default();
        for (at, line) in render(source, refs).into_iter().enumerate() {
            if at > 0 {
                all.binding.push(" ");
                all.shown.push(" ");
            }
            all.binding.append(line.binding);
            all.shown.append(line.shown);
        }
        all.binding = all.binding.trimmed();
        all.shown = all.shown.trimmed();
        all
    }

    /// Decodes a destination or an info string: escapes and references, nothing else.
    fn unescaped(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(ch) = rest.chars().next() {
            if ch == '\\'
                && let Some(next) = rest[1..].chars().next().filter(char::is_ascii_punctuation)
            {
                out.push(next);
                rest = &rest[1 + next.len_utf8()..];
            } else if ch == '&'
                && let Some((entity, end)) = entity_at(rest)
            {
                match entity {
                    Entity::Resolved(text) => out.push_str(&text),
                    Entity::Unresolved => out.push_str(&rest[..end]),
                }
                rest = &rest[end..];
            } else {
                out.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
        out
    }

    enum Entity {
        Resolved(String),
        /// A well-formed `&name;` that `NAMED_ENTITIES` does not hold.
        Unresolved,
    }

    /// The reference opening `text`, which starts with `&`, and the offset past it. GitHub
    /// reads up to eight digits in either base, past CommonMark's seven and six: `POST
    /// /markdown` renders `&#00000065;` as `A`. A digit run that names no Unicode scalar, or
    /// names zero, renders as U+FFFD.
    fn entity_at(text: &str) -> Option<(Entity, usize)> {
        let body = text.strip_prefix('&')?;
        if let Some(number) = body.strip_prefix('#') {
            let hex = number.starts_with(['x', 'X']);
            let (digits, radix, skip) = if hex {
                (&number[1..], 16, 3)
            } else {
                (number, 10, 2)
            };
            let width = digits
                .bytes()
                .take_while(|byte| (*byte as char).is_digit(radix))
                .count();
            if !(1..=8).contains(&width) || !digits[width..].starts_with(';') {
                return None;
            }
            let code = u32::from_str_radix(&digits[..width], radix).ok()?;
            let ch = char::from_u32(code).filter(|ch| *ch != '\0');
            let shown = ch.unwrap_or('\u{fffd}').to_string();
            return Some((Entity::Resolved(shown), skip + width + 1));
        }
        let width = body.bytes().take_while(u8::is_ascii_alphanumeric).count();
        let opens = body.starts_with(|ch: char| ch.is_ascii_alphabetic());
        if !opens || !(2..=32).contains(&width) || !body[width..].starts_with(';') {
            return None;
        }
        let name = &body[..width];
        let entity = NAMED_ENTITIES
            .iter()
            .find(|(known, _)| *known == name)
            .map_or(Entity::Unresolved, |(_, text)| {
                Entity::Resolved((*text).to_owned())
            });
        Some((entity, width + 2))
    }

    /// `text` without any well-formed `&name;`, which states nothing a reader can use.
    pub(super) fn without_references(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(ch) = rest.chars().next() {
            match entity_at(rest) {
                Some((Entity::Unresolved, end)) => rest = &rest[end..],
                _ => {
                    out.push(ch);
                    rest = &rest[ch.len_utf8()..];
                }
            }
        }
        out
    }

    /// The offset past the inline HTML construct opening `text`. Attributes are checked, so
    /// `<id, x>` stays text as it does on the page.
    fn html_end(text: &str) -> Option<usize> {
        let found = |open: usize, close: &str| {
            text.get(open..)?
                .find(close)
                .map(|at| open + at + close.len())
        };
        if text.starts_with("<!--") {
            // `<!-->` is a whole comment: its closing run overlaps its opening run.
            return found(2, "-->");
        }
        if text.starts_with("<?") {
            return found(2, "?>");
        }
        if text.starts_with("<![CDATA[") {
            return found(9, "]]>");
        }
        if text
            .strip_prefix("<!")
            .is_some_and(|rest| rest.starts_with(|ch: char| ch.is_ascii_uppercase()))
        {
            return found(2, ">");
        }
        let bytes = text.as_bytes();
        let space = |byte: &u8| matches!(*byte, b' ' | b'\t' | b'\n');
        let spaces = |at: usize| bytes[at..].iter().take_while(|byte| space(byte)).count();
        let word = |at: usize, first: &dyn Fn(u8) -> bool, rest: &dyn Fn(u8) -> bool| {
            let opens = bytes.get(at).copied().is_some_and(first);
            let more = bytes.get(at + 1..).unwrap_or(&[]);
            opens.then(|| 1 + more.iter().take_while(|byte| rest(**byte)).count())
        };
        let name = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'-';
        if text.starts_with("</") {
            let end = 2 + word(2, &|byte| byte.is_ascii_alphabetic(), &name)?;
            let end = end + spaces(end);
            return (bytes.get(end) == Some(&b'>')).then_some(end + 1);
        }
        let attribute_first = |byte: u8| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':');
        let attribute_rest =
            |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':' | b'.' | b'-');
        let mut at = 1 + word(1, &|byte| byte.is_ascii_alphabetic(), &name)?;
        loop {
            let gap = spaces(at);
            at += gap;
            match bytes.get(at)? {
                b'>' => return Some(at + 1),
                b'/' => return (bytes.get(at + 1) == Some(&b'>')).then_some(at + 2),
                _ if gap == 0 => return None,
                _ => {}
            }
            at += word(at, &attribute_first, &attribute_rest)?;
            let before = at + spaces(at);
            if bytes.get(before) != Some(&b'=') {
                continue;
            }
            at = before + 1;
            at += spaces(at);
            at = match bytes.get(at)? {
                quote @ (b'"' | b'\'') => {
                    at + 2 + bytes[at + 1..].iter().position(|byte| byte == quote)?
                }
                _ => {
                    let plain = |byte: &&u8| {
                        !space(byte) && !matches!(**byte, b'"' | b'\'' | b'=' | b'<' | b'>' | b'`')
                    };
                    let width = bytes[at..].iter().take_while(plain).count();
                    if width == 0 {
                        return None;
                    }
                    at + width
                }
            };
        }
    }

    /// The URL of the autolink opening `text`, and the offset past it.
    fn autolink_at(text: &str) -> Option<(&str, usize)> {
        let end = text.find('>')?;
        let url = &text[1..end];
        let bad = |ch: char| ch.is_ascii_control() || ch == ' ' || ch == '<';
        if url.contains(bad) {
            return None;
        }
        let scheme = url.split(':').next().filter(|_| url.contains(':'))?;
        let valid = (2..=32).contains(&scheme.len())
            && scheme.starts_with(|ch: char| ch.is_ascii_alphabetic())
            && scheme
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"+.-".contains(&byte));
        valid.then_some((url, end + 1))
    }

    /// The address of the email autolink opening `text`, and the offset past it.
    fn email_at(text: &str) -> Option<(&str, usize)> {
        let end = text.find('>')?;
        let address = &text[1..end];
        let (local, domain) = address.split_once('@')?;
        let local_ok = !local.is_empty()
            && local.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&byte)
            });
        let label_ok = |label: &str| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        };
        (local_ok && domain.split('.').all(label_ok)).then_some((address, end + 1))
    }

    /// True for a character CommonMark's flanking rules read as punctuation. std has no
    /// Unicode category, so a non-ASCII mark reads as punctuation too.
    fn is_punctuation(ch: char) -> bool {
        if ch.is_ascii() {
            ch.is_ascii_punctuation()
        } else {
            !ch.is_alphanumeric() && !ch.is_whitespace() && !ch.is_control() && !is_invisible(ch)
        }
    }

    /// One delimiter run: `*`, `_` or `~`.
    #[derive(Clone, Copy, Debug)]
    struct Run {
        mark: u8,
        /// Delimiters left.
        width: usize,
        /// Delimiters the source held, for CommonMark's rule of three.
        original: usize,
        can_open: bool,
        can_close: bool,
    }

    #[derive(Clone, Debug)]
    enum NodeKind {
        Text(String),
        Code(String),
        Unresolved(String),
        Tag(String),
        Target(String),
        Run(Run),
        /// An open `[` or `![`. `start` is where its text begins.
        Open {
            image: bool,
            active: bool,
            nested: bool,
            start: usize,
        },
        /// Struck text opens or closes.
        Strike(bool),
    }

    #[derive(Clone, Debug)]
    struct Node {
        line: usize,
        kind: NodeKind,
    }

    /// One pass over the inline source of a block, following CommonMark's parsing strategy.
    struct InlineParser<'a> {
        source: &'a str,
        at: usize,
        line: usize,
        refs: &'a Refs<'a>,
        nodes: Vec<Node>,
        /// Index in `nodes` of each open bracket, innermost last.
        brackets: Vec<usize>,
    }

    impl InlineParser<'_> {
        fn push(&mut self, kind: NodeKind) {
            let line = self.line;
            self.nodes.push(Node { line, kind });
        }

        fn advance(&mut self, end: usize) {
            self.line += self.source[self.at..end].matches('\n').count();
            self.at = end;
        }

        fn parse(&mut self) {
            let specials: &[char] = &['\\', '`', '<', '&', '*', '_', '~', '!', '[', ']', '\n'];
            while self.at < self.source.len() {
                let rest = &self.source[self.at..];
                match rest.as_bytes()[0] {
                    b'\\' => self.escape(rest),
                    b'`' => self.code(),
                    b'<' => self.angle(rest),
                    b'&' => self.reference(rest),
                    b'*' | b'_' | b'~' => self.delimiter_run(rest),
                    b'!' if rest.starts_with("![") => self.open_bracket(true),
                    b'[' => self.open_bracket(false),
                    b']' => self.close_bracket(),
                    b'\n' => self.advance(self.at + 1),
                    _ => {
                        let first = rest.chars().next().map_or(1, char::len_utf8);
                        let tail = rest[first..].find(specials);
                        let width = tail.map_or(rest.len(), |at| at + first);
                        self.push(NodeKind::Text(rest[..width].to_owned()));
                        self.advance(self.at + width);
                    }
                }
            }
            match_runs(&mut self.nodes, 0);
        }

        fn escape(&mut self, rest: &str) {
            match rest[1..].chars().next() {
                // A hard line break shows no backslash.
                Some('\n') => self.advance(self.at + 1),
                Some(next) if next.is_ascii_punctuation() => {
                    self.push(NodeKind::Text(next.to_string()));
                    self.advance(self.at + 2);
                }
                _ => {
                    self.push(NodeKind::Text("\\".to_owned()));
                    self.advance(self.at + 1);
                }
            }
        }

        fn code(&mut self) {
            match code_span_at(self.source, self.at) {
                Ok(span) => {
                    let width = self.source[span.start..]
                        .bytes()
                        .take_while(|byte| *byte == b'`')
                        .count();
                    let inner = &self.source[span.start + width..span.end - width];
                    let text = inner.replace('\n', " ");
                    let padded = text.starts_with(' ')
                        && text.ends_with(' ')
                        && text.bytes().any(|byte| byte != b' ');
                    let code = if padded {
                        text[1..text.len() - 1].to_owned()
                    } else {
                        text
                    };
                    self.push(NodeKind::Code(code));
                    self.advance(span.end);
                }
                Err(end) => {
                    self.push(NodeKind::Text(self.source[self.at..end].to_owned()));
                    self.advance(end);
                }
            }
        }

        fn angle(&mut self, rest: &str) {
            if let Some((url, end)) = autolink_at(rest) {
                self.push(NodeKind::Target(url.to_owned()));
                self.push(NodeKind::Text(url.to_owned()));
                self.advance(self.at + end);
            } else if let Some((address, end)) = email_at(rest) {
                self.push(NodeKind::Target(format!("mailto:{address}")));
                self.push(NodeKind::Text(address.to_owned()));
                self.advance(self.at + end);
            } else if let Some(end) = html_end(rest) {
                let tag = &rest[..end];
                let name: String = tag[1..]
                    .chars()
                    .take_while(char::is_ascii_alphanumeric)
                    .collect();
                if name.eq_ignore_ascii_case("br") {
                    self.push(NodeKind::Text(" ".to_owned()));
                } else if !name.is_empty() {
                    self.push(NodeKind::Tag(tag.to_owned()));
                }
                self.advance(self.at + end);
            } else {
                self.push(NodeKind::Text("<".to_owned()));
                self.advance(self.at + 1);
            }
        }

        fn reference(&mut self, rest: &str) {
            match entity_at(rest) {
                Some((Entity::Resolved(text), end)) => {
                    self.push(NodeKind::Text(text));
                    self.advance(self.at + end);
                }
                Some((Entity::Unresolved, end)) => {
                    self.push(NodeKind::Unresolved(rest[..end].to_owned()));
                    self.advance(self.at + end);
                }
                None => {
                    self.push(NodeKind::Text("&".to_owned()));
                    self.advance(self.at + 1);
                }
            }
        }

        fn delimiter_run(&mut self, rest: &str) {
            let mark = rest.as_bytes()[0];
            let width = rest.bytes().take_while(|byte| *byte == mark).count();
            let before = self.source[..self.at].chars().next_back();
            let after = rest[width..].chars().next();
            let space = |ch: Option<char>| ch.is_none_or(char::is_whitespace);
            let mark_at = |ch: Option<char>| ch.is_some_and(is_punctuation);
            let left = !space(after) && (!mark_at(after) || space(before) || mark_at(before));
            let right = !space(before) && (!mark_at(before) || space(after) || mark_at(after));
            let (can_open, can_close) = match mark {
                b'_' => (
                    left && (!right || mark_at(before)),
                    right && (!left || mark_at(after)),
                ),
                b'~' if width > 2 => (false, false),
                _ => (left, right),
            };
            self.push(NodeKind::Run(Run {
                mark,
                width,
                original: width,
                can_open,
                can_close,
            }));
            self.advance(self.at + width);
        }

        fn open_bracket(&mut self, image: bool) {
            if let Some(last) = self.brackets.last().copied()
                && let NodeKind::Open { nested, .. } = &mut self.nodes[last].kind
            {
                *nested = true;
            }
            let width = if image { 2 } else { 1 };
            let start = self.at + width;
            self.brackets.push(self.nodes.len());
            self.push(NodeKind::Open {
                image,
                active: true,
                nested: false,
                start,
            });
            self.advance(start);
        }

        /// The destination of the link this `]` closes, and the offset past the link.
        fn link_after(&self, inner: &str, nested: bool) -> Option<(String, usize)> {
            let after = self.at + 1;
            let tail = &self.source[after..];
            if let Some((target, end)) = inline_tail(tail) {
                return Some((unescaped(&target), after + end));
            }
            let lookup = |label: &str| self.refs.links.get(folded(label).as_str()).copied();
            if let Some((label, end)) = tail
                .starts_with('[')
                .then(|| link_label(tail))
                .flatten()
                .filter(|(label, _)| !label.trim().is_empty())
            {
                return lookup(label).map(|target| (target.to_owned(), after + end));
            }
            if nested {
                return None;
            }
            let end = if tail.starts_with("[]") {
                after + 2
            } else {
                after
            };
            lookup(inner).map(|target| (target.to_owned(), end))
        }

        fn close_bracket(&mut self) {
            let Some(at) = self.brackets.pop() else {
                self.push(NodeKind::Text("]".to_owned()));
                self.advance(self.at + 1);
                return;
            };
            let NodeKind::Open {
                image,
                active,
                nested,
                start,
            } = self.nodes[at].kind
            else {
                return;
            };
            let found = active
                .then(|| self.link_after(&self.source[start..self.at], nested))
                .flatten();
            let Some((target, end)) = found else {
                // A footnote mark shows only a number, which is no text of the line.
                let inner = &self.source[start..self.at];
                if !image && active && self.refs.notes.contains(folded(inner).as_str()) {
                    self.nodes.truncate(at);
                    self.advance(self.at + 1);
                    return;
                }
                let bracket = if image { "![" } else { "[" };
                self.nodes[at].kind = NodeKind::Text(bracket.to_owned());
                self.push(NodeKind::Text("]".to_owned()));
                self.advance(self.at + 1);
                return;
            };
            match_runs(&mut self.nodes, at + 1);
            self.nodes[at].kind = NodeKind::Target(target);
            if !image {
                for open in &self.brackets {
                    if let NodeKind::Open {
                        image: false,
                        active,
                        ..
                    } = &mut self.nodes[*open].kind
                    {
                        *active = false;
                    }
                }
            }
            self.advance(end);
        }
    }

    /// The raw target of the `(target "title")` opening `text`, and the offset past it.
    fn inline_tail(text: &str) -> Option<(String, usize)> {
        let bytes = text.as_bytes();
        if bytes.first() != Some(&b'(') {
            return None;
        }
        let gap = |at: usize| {
            at + bytes[at..]
                .iter()
                .take_while(|byte| matches!(**byte, b' ' | b'\t' | b'\n'))
                .count()
        };
        let start = gap(1);
        if bytes.get(start) == Some(&b')') {
            return Some((String::new(), start + 1));
        }
        let (target, width) = link_destination(&text[start..])?;
        let mut at = start + width;
        let next = gap(at);
        if next > at
            && let Some(title) = link_title(&text[next..])
        {
            at = gap(next + title);
        } else {
            at = next;
        }
        (bytes.get(at) == Some(&b')')).then(|| (target.to_owned(), at + 1))
    }

    /// CommonMark's emphasis pass over `nodes[from..]`, with GFM's strikethrough: `~` pairs
    /// only with a run of its own width. Every run left over reads as literal text.
    fn match_runs(nodes: &mut [Node], from: usize) {
        // The node below which no opener can match a closer of each kind, so a run with no
        // partner is searched for once.
        let mut floors = [from; 18];
        let kind = |run: &Run| {
            let mark = match run.mark {
                b'*' => 0,
                b'_' => 1,
                _ => 2,
            };
            mark * 6 + usize::from(run.can_open) * 3 + run.original % 3
        };
        let mut closer = from;
        while closer < nodes.len() {
            let NodeKind::Run(end) = nodes[closer].kind else {
                closer += 1;
                continue;
            };
            if !end.can_close || end.width == 0 {
                closer += 1;
                continue;
            }
            let floor = kind(&end);
            let opener = (floors[floor]..closer).rev().find(|at| {
                let NodeKind::Run(open) = nodes[*at].kind else {
                    return false;
                };
                let odd = (open.can_close || end.can_open)
                    && end.original % 3 != 0
                    && (open.original + end.original) % 3 == 0;
                open.mark == end.mark
                    && open.can_open
                    && open.width > 0
                    && if end.mark == b'~' {
                        open.width == end.width
                    } else {
                        !odd
                    }
            });
            let Some(opener) = opener else {
                floors[floor] = closer;
                if !end.can_open
                    && let NodeKind::Run(run) = &mut nodes[closer].kind
                {
                    run.can_close = false;
                }
                closer += 1;
                continue;
            };
            for node in &mut nodes[opener + 1..closer] {
                if let NodeKind::Run(run) = &mut node.kind {
                    run.can_open = false;
                    run.can_close = false;
                }
            }
            if end.mark == b'~' {
                nodes[opener].kind = NodeKind::Strike(true);
                nodes[closer].kind = NodeKind::Strike(false);
                closer += 1;
                continue;
            }
            // Strong takes two marks and emphasis one, and both show the same text.
            for at in [opener, closer] {
                if let NodeKind::Run(run) = &mut nodes[at].kind {
                    run.width -= 1;
                }
            }
            if end.width == 1 {
                closer += 1;
            }
        }
        for node in &mut nodes[from..] {
            if let NodeKind::Run(run) = &mut node.kind {
                run.can_open = false;
                run.can_close = false;
            }
        }
    }

    /// Renders inline source into one `Rendered` per source line. A construct that spans
    /// lines belongs to the line that opens it.
    fn render(source: &str, refs: &Refs) -> Vec<Rendered> {
        let mut parser = InlineParser {
            source,
            at: 0,
            line: 0,
            refs,
            nodes: Vec::new(),
            brackets: Vec::new(),
        };
        parser.parse();
        let mut lines = vec![Rendered::default(); source.matches('\n').count() + 1];
        let mut struck = 0_usize;
        for node in parser.nodes {
            let Some(line) = lines.get_mut(node.line) else {
                continue;
            };
            let binding = struck == 0;
            let literal = match node.kind {
                NodeKind::Text(text) => text,
                NodeKind::Run(run) => (run.mark as char).to_string().repeat(run.width),
                NodeKind::Open { image, .. } => (if image { "![" } else { "[" }).to_owned(),
                NodeKind::Code(text) => {
                    line.shown.push(&text);
                    if binding {
                        let range = line.binding.push(&text);
                        line.binding.code.push(range);
                    }
                    continue;
                }
                NodeKind::Unresolved(text) => {
                    let range = line.shown.push(&text);
                    line.shown.unresolved.push(range);
                    if binding {
                        let range = line.binding.push(&text);
                        line.binding.unresolved.push(range);
                    }
                    continue;
                }
                NodeKind::Tag(tag) => {
                    if binding {
                        line.binding.tags.push((line.binding.text.len(), tag));
                    }
                    continue;
                }
                NodeKind::Target(target) => {
                    if binding {
                        line.binding.links.push(target);
                    }
                    continue;
                }
                NodeKind::Strike(open) => {
                    struck = if open {
                        struck + 1
                    } else {
                        struck.saturating_sub(1)
                    };
                    continue;
                }
            };
            line.shown.push(&literal);
            if binding {
                line.binding.push(&literal);
            }
        }
        for line in &mut lines {
            line.binding = std::mem::take(&mut line.binding).trimmed();
            line.shown = std::mem::take(&mut line.shown).trimmed();
        }
        lines
    }
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
    let value = without_references(value);
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
    } else if (wrapped.starts_with('<') && wrapped.ends_with('>') && !is_autolink(wrapped))
        || !bare.contains(char::is_alphanumeric)
        || labelled
        || (control && lower == "none")
    {
        Some(NonValue::Placeholder)
    } else {
        None
    }
}

/// `<scheme://x>`. A tag such as `<a href="https://x">` shows no text, and holds a space.
fn is_autolink(text: &str) -> bool {
    text.contains("://") && !text.contains(char::is_whitespace)
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

/// `text` from its first word, past every list marker, so `- - x` reads as `- x` does, and
/// past a task box, which shows a checkbox.
fn without_list_markers(text: &str) -> &str {
    let mut text = text.trim_start();
    while let Some(marker) = list_marker(text, false) {
        text = text[marker..].trim_start();
    }
    TASK_BOXES
        .iter()
        .find_map(|task_box| text.strip_prefix(task_box))
        .filter(|rest| rest.starts_with([' ', '\t']))
        .map_or(text, str::trim_start)
}

/// A label as a reader compares it. Marks and spacing never make a second label.
fn label_words(spelled: &str) -> String {
    let plain: String = spelled
        .chars()
        .filter(|ch| !LABEL_MARKS.contains(ch))
        .collect();
    let words: Vec<&str> = plain.split_whitespace().collect();
    words.join(" ")
}

/// Value of `label` on a rendered `Label: value` line. A slot such as `<state>` is a tag that
/// shows nothing, so a value holding only a tag keeps it for the non-value rules to read.
fn field_value(line: &Inline, label: &str, spelling: Spelling) -> Option<String> {
    let text = without_list_markers(&line.text);
    let (spelled, tail) = text.split_once(':')?;
    let colon = line.text.len() - text.len() + spelled.len();
    // A code span that runs past the colon quotes a field. `Pin:` in code only marks one.
    let quoted = line
        .code
        .iter()
        .any(|span| span.contains(&colon) && span.end > colon + 1);
    (!quoted && spelling.matches(&label_words(spelled), label)).then(|| {
        let value = tail.trim_start_matches(LABEL_MARKS).trim();
        let slot = line.tags.iter().find(|(at, _)| *at > colon);
        match slot {
            Some((_, tag)) if value.is_empty() => tag.clone(),
            _ => value.to_owned(),
        }
    })
}

/// Each line of a range that states `label`.
fn field_lines(doc: &Doc, start: usize, end: usize, label: &str, spelling: Spelling) -> Vec<usize> {
    (start..end)
        .filter(|index| {
            doc.line(*index)
                .field()
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
        let inline = doc
            .line(line)
            .field()
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
    let own = doc.line(field).column();
    let mut statements: Vec<String> = inline.into_iter().collect();
    let mut open = !statements.is_empty();
    for index in field + 1..end {
        let line = doc.line(index);
        if line.is_blank() {
            open = false;
            continue;
        }
        if line.column() <= own {
            break;
        }
        let fenced = line.code();
        let shown = line.prose().map(|inline| inline.text.as_str());
        let Some(text) = fenced.or(shown).map(str::trim) else {
            continue;
        };
        let value = without_list_markers(text);
        let item = line.is_item() || value.len() < text.len();
        let fenced = fenced.is_some();
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
    /// Preceded by `#n`, `KEL-n` or `GH-n`: another spec's ID.
    foreign: bool,
}

/// `AC-3`-style text at the start of `text`, returning its kind and number.
fn parse_id(text: &str) -> Option<(&'static str, u32, usize)> {
    KINDS.iter().find_map(|(kind, _)| {
        let digits = text.strip_prefix(kind)?.strip_prefix('-')?;
        let width = digits.bytes().take_while(u8::is_ascii_digit).count();
        // Too long to be a number is still an ID, so its fields are read.
        let number = digits.get(..width)?.parse().unwrap_or(u32::MAX);
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
    // `(#1311 AC-2)` is foreign too, and a link or emphasis has already rendered away.
    is_tracker_word(word.trim_matches(|ch: char| "()[]".contains(ch)))
}

/// Every stable-ID token in rendered text. URLs are not prose.
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
        let text = &heading.shown.text;
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

/// A link path as the server reads it: `%2F` is a separator, so no spelling hides a path.
/// A byte run that is not UTF-8 stays as it is written.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let pair = bytes
            .get(at + 1..at + 3)
            .filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))
            .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        match pair {
            Some(byte) if bytes[at] == b'%' => {
                out.push(byte);
                at += 3;
            }
            _ => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
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
    let mut labels: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, label) in doc.labels() {
        if let Some(first) = labels.get(label) {
            report.add(
                Rule::Link,
                &doc.rel,
                index + 1,
                format!(
                    "`[{label}]` is already defined at line {first}. CommonMark keeps the first \
                     and renders this one nowhere. Remove one"
                ),
            );
        }
        labels.entry(label).or_insert(index + 1);
    }
    for (index, target) in doc.targets() {
        if is_external(target) || target.is_empty() {
            continue;
        }
        let (path, fragment) = target.split_once('#').unwrap_or((target, ""));
        let (path, fragment) = (percent_decoded(path), percent_decoded(fragment));
        let resolved = if path.is_empty() {
            Some(doc.rel.clone())
        } else {
            resolve_link(&doc.rel, &path)
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
            || fragment.starts_with('L') && fragment[1..].bytes().all(|byte| byte.is_ascii_digit())
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
        if !slugs.contains(&fragment) {
            report.add(
                Rule::Anchor,
                &doc.rel,
                index + 1,
                format!("anchor `#{fragment}` is not a heading in `{resolved}`"),
            );
        }
    }
}

/// What the header block of an index or single-file spec must state.
fn check_header(doc: &Doc, report: &mut Report) -> Option<String> {
    let end = header_end(doc);
    let titled = doc.headings().first().is_some_and(|heading| {
        heading.index == 0 && heading.level == 1 && !heading.text().is_empty()
    });
    if !titled {
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
        .map_or(doc.line_count(), |heading| heading.index)
}

/// The header owns the facts of a spec. The same field in a part or a section would be a
/// second authority that no rule reads, however well it matches.
fn check_restated_header(index: &Doc, parts: &[Doc], report: &mut Report) {
    let mut scopes = Vec::new();
    for label in HEADER_ONLY_FIELDS {
        let start = header_end(index);
        scopes.push((index, start, index.line_count(), label, Spelling::Body));
    }
    for part in parts {
        let header = header_end(part);
        for label in HEADER_FIELDS {
            scopes.push((part, 0, header, label, Spelling::Header));
        }
        for label in HEADER_ONLY_FIELDS {
            scopes.push((part, header, part.line_count(), label, Spelling::Body));
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
        .map(|heading| (heading.index, heading.text().to_owned()))
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
                format!(
                    "a file has one `# ` title; `# {}` is a second",
                    second.text()
                ),
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
    let mut lines = doc
        .lines()
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.is_blank());
    let first = doc.headings().first();
    let title = lines
        .next()
        .is_some_and(|(at, _)| first.is_some_and(|top| top.index == at && top.level == 1));
    let backlink = lines
        .next()
        .and_then(|(_, line)| line.field())
        .is_some_and(|text| {
            let value = field_value(text, "Part of", Spelling::Exact);
            value.as_deref() == Some("index") && text.links == ["index.md"]
        });
    let owns = lines
        .next()
        .and_then(|(_, line)| line.field())
        .and_then(|text| field_value(text, "Owns", Spelling::Exact))
        .is_some_and(|value| !value.is_empty());
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

fn is_table_delimiter(cells: &[Inline]) -> bool {
    cells.len() == 3 && cells.iter().all(|cell| is_delimiter_cell(&cell.text))
}

/// The rendered text of each cell.
fn cell_texts(cells: &[Inline]) -> Vec<&str> {
    cells.iter().map(|cell| cell.text.as_str()).collect()
}

/// Part file names from the index table, in table order.
fn part_table(index: &Doc, report: &mut Report) -> Vec<String> {
    let headings = index.headings();
    let Some(at) = headings
        .iter()
        .position(|heading| heading.level == 2 && heading.text() == PARTS_HEADING)
    else {
        return Vec::new();
    };
    let start = headings[at].index + 1;
    let end = index.body_end(headings, at);
    let rows: Vec<(usize, &[Inline])> = (start..end)
        .filter_map(|line| Some((line, index.line(line).cells()?)))
        .collect();
    let columns: Vec<&str> = PART_TABLE_HEADER
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect();
    let mut parts = Vec::new();
    if rows.first().map(|(_, cells)| cell_texts(cells)) != Some(columns) {
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
        .is_some_and(|(_, cells)| is_table_delimiter(cells))
    {
        report.add(
            Rule::PartTable,
            &index.rel,
            start,
            "the part table header needs its `|---|---|---|` row, or no table renders".to_owned(),
        );
    }
    for (line, cells) in rows.iter().skip(2) {
        let target = &cells[0].links;
        // A malformed row still names its part, so one bad cell reports one defect.
        parts.extend(target.first().map(|link| percent_decoded(link)));
        let empty = |at: usize| cells.get(at).is_none_or(|cell| cell.text.is_empty());
        if cells.len() != 3 || target.len() != 1 || empty(1) || empty(2) {
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
                section = canonical_section(heading.text());
            }
            let Some((kind, number)) = defined_id(heading.text()) else {
                continue;
            };
            let line = heading.index + 1;
            // A definition ends at the next one, so a nested ID lends its parent no field.
            let body = headings[at + 1..]
                .iter()
                .find(|next| next.level <= heading.level || defined_id(next.text()).is_some())
                .map_or(doc.line_count(), |next| next.index);
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

/// Text the rendered view could not read as GitHub does: a lazy tag, or a block past a bound.
fn check_unread(doc: &Doc, report: &mut Report) {
    if let Some(index) = doc.lazy_tag {
        report.add(
            Rule::Comment,
            &doc.rel,
            index + 1,
            "this tag right after a quote or list opens an HTML block on GitHub, which hides \
             the lines after it. Put a blank line before it"
                .to_owned(),
        );
    }
    if let Some(index) = doc.unread {
        report.add(Rule::Unreadable, &doc.rel, index + 1, past_bound());
    }
}

/// Why a rule cannot read a block past a bound of the rendered view.
fn past_bound() -> String {
    format!(
        "this block nests past {MAX_LIST_DEPTH} levels or holds over {MAX_INLINE_BYTES} bytes of \
         text, so no rule reads it. Split it"
    )
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
        check_unread(doc, report);
        for (index, line) in doc.lines().iter().enumerate() {
            let defines = line
                .code()
                .and_then(atx_heading)
                .is_some_and(|(_, title)| defined_id(&render_text(&title).text).is_some());
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
        for (index, line) in doc.lines().iter().enumerate() {
            let Some(line) = line.prose() else { continue };
            for token in id_tokens(&line.text) {
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
        .position(|heading| heading.level == 2 && heading.text() == BLOCKERS_HEADING)
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
        .filter_map(|line| index.line(line).prose())
        .flat_map(|line| id_tokens(&line.text))
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

/// True when an unresolved reference before the first colon may hide a label: the text is
/// a label once the reference goes, or it was one before it, since `Pin&foo;` may show `:`.
fn may_state_label(line: &Inline) -> bool {
    let colon = line.text.find(':').unwrap_or(line.text.len());
    if line
        .unresolved
        .first()
        .is_none_or(|range| range.start > colon)
    {
        return false;
    }
    let pieces = line.around_unresolved();
    let joined = pieces.concat();
    let head = without_list_markers(&joined)
        .split(':')
        .next()
        .unwrap_or("");
    let before = without_list_markers(pieces[0]);
    [head, before].iter().any(|spelled| {
        let spelled = label_words(spelled);
        ALL_LABELS
            .iter()
            .flat_map(|labels| labels.iter())
            .any(|label| Spelling::Header.matches(&spelled, label))
    })
}

/// An unresolved `&name;` can render as anything, so a heading or a label that holds one is
/// a statement no rule can read. Say the character instead.
fn check_entities(docs: &[&Doc], report: &mut Report) {
    for doc in docs {
        for heading in doc.headings() {
            if !heading.title.unresolved.is_empty() {
                report.add(
                    Rule::Entity,
                    &doc.rel,
                    heading.index + 1,
                    format!(
                        "heading `{}` holds a reference no rule resolves. Write the character",
                        heading.text()
                    ),
                );
            }
        }
        for (index, line) in doc.lines().iter().enumerate() {
            if let Some(line) = line.field()
                && !line.unresolved.is_empty()
                && may_state_label(line)
            {
                report.add(
                    Rule::Entity,
                    &doc.rel,
                    index + 1,
                    format!(
                        "`{}` may state a field through a reference no rule resolves. Write the \
                         character",
                        line.text
                    ),
                );
            }
        }
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

fn check_citations(docs: &[&Doc], pin: &str, history: &dyn History, report: &mut Report) {
    for doc in docs {
        for (index, line) in doc.lines().iter().enumerate() {
            let Some(line) = line.prose() else { continue };
            for code in line.code_spans() {
                // Backticks that wrap a citation inside a wider span are its quoting.
                let span = code.trim_matches('`');
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
    check_entities(&docs, report);
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
        lines: doc.line_count(),
        bytes: doc.source_bytes(),
        overflow: doc
            .source_line_widths()
            .iter()
            .map(|chars| chars.saturating_sub(MAX_LINE_CHARS))
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
    for (index, chars) in doc.source_line_widths().iter().enumerate() {
        if *chars > MAX_LINE_CHARS {
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
                && ALL_LABELS[2..].iter().any(|labels| labels.contains(&label))
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

/// The one `markdown` fence a reader of the template sees, which must itself pass the
/// standard. A comment, an HTML block, a quote or indented code shows no copy source.
fn check_template(root: &Path, report: &mut Report) -> Result<(), String> {
    let doc = read_doc(root, TEMPLATE)?;
    // GitHub may hide a fence this view reads, so the template has no such text.
    check_unread(&doc, report);
    let copies: Vec<_> = doc
        .fences()
        .iter()
        .filter(|fence| fence.language == "markdown" && !fence.quoted)
        .collect();
    let [copy] = copies[..] else {
        report.add(
            Rule::Sections,
            TEMPLATE,
            1,
            "the template needs exactly one visible ```markdown fence".to_owned(),
        );
        return Ok(());
    };
    if copy.body.is_empty() || !copy.closed {
        report.add(
            Rule::Sections,
            TEMPLATE,
            copy.line + 1,
            "the template's ```markdown fence needs a body and a closing marker".to_owned(),
        );
        return Ok(());
    }
    let mut structure = Report::default();
    let copy_source = Doc::parse(TEMPLATE, &copy.body);
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

/// True for rendered text a reader takes for the spec heading, whatever its spacing.
fn marks_spec(text: &str) -> bool {
    let plain = text.to_lowercase();
    let words: Vec<&str> = plain.split_whitespace().collect();
    words.join(" ").contains(SPEC_SIGNATURE)
}

/// True when an unresolved reference could complete the spec heading: all the rest of the
/// title sits inside the signature, in order. Other titles stay ordinary notes.
fn may_mark_spec(title: &Inline) -> bool {
    if title.unresolved.is_empty() {
        return false;
    }
    let signature: String = SPEC_SIGNATURE.split_whitespace().collect();
    let mut at = 0;
    let mut matched = false;
    for piece in title.around_unresolved() {
        let piece: String = piece
            .to_lowercase()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect();
        if piece.is_empty() {
            continue;
        }
        let Some(found) = signature[at..].find(&piece) else {
            return false;
        };
        at += found + piece.len();
        matched = true;
    }
    matched
}

/// The first heading that marks `doc` as a spec. Struck text is not binding in a spec, but
/// here it would hide one, so the title on the page speaks, and so does a quote's heading.
fn spec_mark(doc: &Doc) -> Option<(usize, String)> {
    let marks =
        |heading: &&Heading| marks_spec(&heading.shown.text) || may_mark_spec(&heading.shown);
    doc.headings()
        .iter()
        .find(marks)
        .or_else(|| doc.quoted_headings().iter().find(marks))
        .map(|heading| (heading.index, heading.shown.text.clone()))
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
        let doc = Doc::parse(rel, &raw);
        if let Some(index) = doc.unread {
            report.add(Rule::Unreadable, rel, index + 1, past_bound());
        }
        if let Some((index, title)) = spec_mark(&doc) {
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
