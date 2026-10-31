//! Versioned compatibility evidence records and denominator scoring (KEL-74).
//!
//! Framework-generic: this module does not name Electron APIs, VS Code
//! extensions, or package ecosystems. Those corpora consume the schema later.

use std::fmt;

mod parsing;
pub use parsing::{parse_denominator, parse_evidence};
use parsing::{parse_digest, validate_denominator_cells};

mod scoring;
pub use scoring::score;

mod uri;
use uri::parse_evidence_uri;

/// Maximum accepted evidence-record JSON size.
pub const MAX_EVIDENCE_BYTES: usize = 65_536;
/// Maximum accepted denominator JSON size.
pub const MAX_DENOMINATOR_BYTES: usize = 262_144;

const EVIDENCE_SCHEMA: &str = "keld.compat.evidence/v1";
const DENOMINATOR_SCHEMA: &str = "keld.compat.denominator/v1";

/// Typed parse / score failure. Every variant is a `KELD-COMPAT-*` code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceError {
    /// Input exceeded the documented size cap.
    TooLarge {
        /// Observed length in bytes.
        len: usize,
        /// Cap that was exceeded.
        max: usize,
    },
    /// Bytes were not UTF-8.
    NotUtf8,
    /// JSON syntax, trailing junk, or empty input.
    InvalidJson {
        /// Parser detail (safe to show).
        detail: String,
    },
    /// `schema` is missing or not a known v1 id.
    UnsupportedSchema {
        /// Value that was present, or empty.
        found: String,
    },
    /// Closed-set / format violation on a record field.
    InvalidRecord {
        /// Which rule failed.
        detail: String,
    },
    /// Waiver missing, extra, or expired.
    InvalidWaiver {
        /// Which rule failed.
        detail: String,
    },
    /// Evidence URI is a lead, not an immutable location.
    NonNormativeEvidence {
        /// Why the URI was rejected.
        detail: String,
    },
    /// Denominator empty, duplicated, or otherwise unusable.
    InvalidDenominator {
        /// Which rule failed.
        detail: String,
    },
    /// Two records named the same denominator cell.
    DuplicateCell {
        /// `operation_id/oracle_id`.
        cell: String,
    },
}

impl EvidenceError {
    /// Stable `KELD-COMPAT-*` code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::TooLarge { .. } => "KELD-COMPAT-001",
            Self::NotUtf8 => "KELD-COMPAT-002",
            Self::InvalidJson { .. } => "KELD-COMPAT-003",
            Self::UnsupportedSchema { .. } => "KELD-COMPAT-004",
            Self::InvalidRecord { .. } => "KELD-COMPAT-005",
            Self::InvalidWaiver { .. } => "KELD-COMPAT-006",
            Self::NonNormativeEvidence { .. } => "KELD-COMPAT-007",
            Self::InvalidDenominator { .. } => "KELD-COMPAT-008",
            Self::DuplicateCell { .. } => "KELD-COMPAT-009",
        }
    }
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { len, max } => write!(
                f,
                "KELD-COMPAT-001: input is {len} bytes, over the {max}-byte cap. \
                 Split the ledger or shrink the document."
            ),
            Self::NotUtf8 => write!(
                f,
                "KELD-COMPAT-002: evidence bytes are not UTF-8. \
                 Re-encode the document as UTF-8 without a BOM."
            ),
            Self::InvalidJson { detail } => write!(
                f,
                "KELD-COMPAT-003: invalid JSON ({detail}). \
                 Supply a single UTF-8 JSON object with no trailing bytes."
            ),
            Self::UnsupportedSchema { found } => write!(
                f,
                "KELD-COMPAT-004: unsupported schema `{found}`. \
                 Use `{EVIDENCE_SCHEMA}` or `{DENOMINATOR_SCHEMA}`."
            ),
            Self::InvalidRecord { detail } => write!(
                f,
                "KELD-COMPAT-005: invalid evidence record ({detail}). \
                 Use the closed field set in docs/specs/kel74-compat-evidence-schema.md."
            ),
            Self::InvalidWaiver { detail } => write!(
                f,
                "KELD-COMPAT-006: invalid waiver ({detail}). \
                 Waive only with owner, reason, and a future YYYY-MM-DD expiry."
            ),
            Self::NonNormativeEvidence { detail } => write!(
                f,
                "KELD-COMPAT-007: evidence URI is not immutable ({detail}). \
                 Use sha256:<64 lowercase hex> or an https URL with a public \
                 host (not loopback, RFC1918, CGNAT, NAT64/6to4, link-local, \
                 or unique-local; a colon in an unbracketed authority must be \
                 a decimal u16 port) whose blob/tree/raw (or GitHub raw CDN) \
                 ref is itself a 40- or 64-character lowercase-hex git object \
                 id — not a later path segment on a live branch; turn \
                 citations, sandbox paths, and mutable branch URLs are \
                 non-normative leads only."
            ),
            Self::InvalidDenominator { detail } => write!(
                f,
                "KELD-COMPAT-008: invalid denominator ({detail}). \
                 Commit a v1 denominator with unique cells before scoring."
            ),
            Self::DuplicateCell { cell } => write!(
                f,
                "KELD-COMPAT-009: duplicate evidence for cell `{cell}`. \
                 Keep one record per (operation_id, oracle_id)."
            ),
        }
    }
}

impl std::error::Error for EvidenceError {}

/// Calendar date used for waiver expiry (`YYYY-MM-DD`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CivilDate {
    /// Year (e.g. 2026).
    pub year: u16,
    /// Month 1–12.
    pub month: u8,
    /// Day of month.
    pub day: u8,
}

impl CivilDate {
    /// Parse `YYYY-MM-DD` with real month lengths. No timezone.
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError::InvalidWaiver`] when the string is not a real date.
    pub fn parse(text: &str) -> Result<Self, EvidenceError> {
        let mut parts = text.split('-');
        let (Some(ys), Some(ms), Some(ds), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(EvidenceError::InvalidWaiver {
                detail: format!("expires_on `{text}` is not YYYY-MM-DD"),
            });
        };
        if ys.len() != 4
            || ms.len() != 2
            || ds.len() != 2
            || !ascii_digits(ys)
            || !ascii_digits(ms)
            || !ascii_digits(ds)
        {
            return Err(EvidenceError::InvalidWaiver {
                detail: format!("expires_on `{text}` is not YYYY-MM-DD"),
            });
        }
        // Digit check above rejects signs; `parse` cannot fail for 4/2/2 ASCII digits.
        let year: u16 = ys.parse().map_err(|_| EvidenceError::InvalidWaiver {
            detail: format!("expires_on `{text}` is not YYYY-MM-DD"),
        })?;
        let month: u8 = ms.parse().map_err(|_| EvidenceError::InvalidWaiver {
            detail: format!("expires_on `{text}` is not YYYY-MM-DD"),
        })?;
        let day: u8 = ds.parse().map_err(|_| EvidenceError::InvalidWaiver {
            detail: format!("expires_on `{text}` is not YYYY-MM-DD"),
        })?;
        let parsed = Self { year, month, day };
        if !parsed.valid() {
            return Err(EvidenceError::InvalidWaiver {
                detail: format!("expires_on `{text}` is not a real calendar date"),
            });
        }
        Ok(parsed)
    }

    fn valid(self) -> bool {
        let max = days_in_month(self.year, self.month);
        self.month >= 1 && max > 0 && self.day >= 1 && self.day <= max
    }
}

impl fmt::Display for CivilDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

fn ascii_digits(chunk: &str) -> bool {
    !chunk.is_empty() && chunk.bytes().all(|b| b.is_ascii_digit())
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap(year: u16) -> bool {
    let y = u32::from(year);
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

/// Host OS family recorded with the artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// macOS.
    Macos,
    /// Windows.
    Windows,
    /// Linux.
    Linux,
}

/// Instruction-set recorded with the artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    /// 64-bit ARM.
    Aarch64,
    /// 64-bit x86.
    X86_64,
}

/// Authority profile under which the operation ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityProfile {
    /// Strict Bun child (zero ambient OS authority).
    StrictBun,
    /// Native addon in a sandboxed worker.
    SandboxedAddonWorker,
    /// Explicit legacy sandbox-off.
    LegacySandboxOff,
    /// User-approved tool child.
    UserApprovedToolChild,
    /// Run without verified containment; never strict or legacy evidence.
    Unverified,
}

impl AuthorityProfile {
    /// The `authority_profile` string for this variant: the one
    /// variant-to-string map for records, reports and scoreboard rendering.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StrictBun => "strict_bun",
            Self::SandboxedAddonWorker => "sandboxed_addon_worker",
            Self::LegacySandboxOff => "legacy_sandbox_off",
            Self::UserApprovedToolChild => "user_approved_tool_child",
            Self::Unverified => "unverified",
        }
    }
}

/// Which denominator this cell belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    /// Install / unpack.
    Install,
    /// Startup / activation.
    Activation,
    /// Primary user workflow.
    PrimaryWorkflow,
    /// Full-feature conformance.
    FullFeature,
}

impl OperationKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Activation => "activation",
            Self::PrimaryWorkflow => "primary_workflow",
            Self::FullFeature => "full_feature",
        }
    }
}

/// Product panel versus a named showcase / north-star corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// Median product corpus. Showcase results MUST NOT redefine these tiers.
    Product,
    /// Named stress corpus (VS Code and others later).
    Showcase,
}

impl Panel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Showcase => "showcase",
        }
    }
}

/// Pass / fail / unknown / waived. No silent fourth state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Semantic oracle passed.
    Pass,
    /// Semantic oracle failed.
    Fail,
    /// Not yet measured or inconclusive.
    Unknown,
    /// Explicit waiver with owner, reason, and expiry.
    Waived,
}

/// Content-addressed artifact identity. The digest is not computed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactIdentity {
    /// Canonical `sha256:` + 64 lowercase hex.
    pub sha256: String,
    /// OS family.
    pub platform: Platform,
    /// Architecture.
    pub arch: Arch,
}

/// Pinned runtime revisions. `latest` is rejected at parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revisions {
    /// Keld revision (usually a git SHA).
    pub keld: String,
    /// Bun version string.
    pub bun: String,
    /// Engine id (`wkwebview`, `webview2`, `webkitgtk`, `cef`, …).
    pub engine: String,
}

/// Semantic oracle identity. Import success is never an oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticOracle {
    /// Stable oracle id.
    pub id: String,
    /// Pinned oracle revision. Not `latest`.
    pub revision: String,
}

/// Operation cell inside a denominator kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    /// Closed-token operation id.
    pub id: String,
    /// Denominator kind this operation is scored under.
    pub kind: OperationKind,
    /// Semantic oracle that decides pass/fail.
    pub oracle: SemanticOracle,
}

/// Required metadata when [`Verdict::Waived`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiver {
    /// Human or team that owns the waiver.
    pub owner: String,
    /// Why the cell is waived.
    pub reason: String,
    /// Inclusive last valid date.
    pub expires_on: CivilDate,
}

/// One versioned evidence record.
///
/// Fields are private: only [`parse_evidence`] constructs a record for callers
/// outside this module. [`score`] still re-validates pairing, URI, and identity
/// so a same-crate hand-built value cannot become a published percentage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRecord {
    artifact: ArtifactIdentity,
    revisions: Revisions,
    authority_profile: AuthorityProfile,
    operation: Operation,
    result: Verdict,
    waiver: Option<Waiver>,
    evidence_uri: String,
}

impl EvidenceRecord {
    /// Shipped artifact identity.
    #[must_use]
    pub fn artifact(&self) -> &ArtifactIdentity {
        &self.artifact
    }

    /// Keld / Bun / engine pins.
    #[must_use]
    pub fn revisions(&self) -> &Revisions {
        &self.revisions
    }

    /// Authority profile used for the run.
    #[must_use]
    pub fn authority_profile(&self) -> AuthorityProfile {
        self.authority_profile
    }

    /// Operation and oracle.
    #[must_use]
    pub fn operation(&self) -> &Operation {
        &self.operation
    }

    /// Cell verdict.
    #[must_use]
    pub fn result(&self) -> Verdict {
        self.result
    }

    /// Present only for [`Verdict::Waived`].
    #[must_use]
    pub fn waiver(&self) -> Option<&Waiver> {
        self.waiver.as_ref()
    }

    /// Immutable `sha256:` digest or https URL with a git object id in the path.
    #[must_use]
    pub fn evidence_uri(&self) -> &str {
        &self.evidence_uri
    }

    fn cell_key(&self) -> CellKey {
        CellKey {
            operation_id: self.operation.id.clone(),
            oracle_id: self.operation.oracle.id.clone(),
        }
    }
}

/// One required scoreboard cell.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellKey {
    /// Operation id.
    pub operation_id: String,
    /// Oracle id.
    pub oracle_id: String,
}

impl CellKey {
    fn label(&self) -> String {
        format!("{}/{}", self.operation_id, self.oracle_id)
    }
}

/// Committed corpus denominator. Required before any percentage.
///
/// Fields are private: only [`parse_denominator`] constructs a value for
/// callers outside this module. [`score`] still rejects empty or duplicate
/// `cells` so a hand-built list cannot claim `0/0` or double-count one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denominator {
    panel: Panel,
    corpus_id: String,
    corpus_sha256: String,
    kind: OperationKind,
    cells: Vec<CellKey>,
}

impl Denominator {
    /// Product vs showcase panel.
    #[must_use]
    pub fn panel(&self) -> Panel {
        self.panel
    }

    /// Stable corpus name.
    #[must_use]
    pub fn corpus_id(&self) -> &str {
        &self.corpus_id
    }

    /// Digest of the committed corpus manifest.
    #[must_use]
    pub fn corpus_sha256(&self) -> &str {
        &self.corpus_sha256
    }

    /// Which of the four denominators this list is.
    #[must_use]
    pub fn kind(&self) -> OperationKind {
        self.kind
    }

    /// Required cells, unique.
    #[must_use]
    pub fn cells(&self) -> &[CellKey] {
        &self.cells
    }
}

/// Honest scoreboard row. Percentages never hide a missing denominator.
///
/// Fields are private: only [`score`] constructs a value for callers
/// outside this module. A same-crate (or downstream) struct literal MUST NOT
/// mint `complete: true` or `unweighted_percent: Some(100)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scoreboard {
    panel: Panel,
    corpus_id: String,
    corpus_sha256: String,
    kind: OperationKind,
    denominator: usize,
    passed: usize,
    failed: usize,
    unknown: usize,
    waived: usize,
    missing: usize,
    authority_profile: Option<AuthorityProfile>,
    unweighted_percent: Option<u8>,
    complete: bool,
    claim: String,
}

impl Scoreboard {
    /// Echo of the denominator panel.
    #[must_use]
    pub fn panel(&self) -> Panel {
        self.panel
    }

    /// Echo of the corpus id.
    #[must_use]
    pub fn corpus_id(&self) -> &str {
        &self.corpus_id
    }

    /// Echo of the corpus digest.
    #[must_use]
    pub fn corpus_sha256(&self) -> &str {
        &self.corpus_sha256
    }

    /// Echo of the denominator kind.
    #[must_use]
    pub fn kind(&self) -> OperationKind {
        self.kind
    }

    /// Committed cell count (N).
    #[must_use]
    pub fn denominator(&self) -> usize {
        self.denominator
    }

    /// Cells with [`Verdict::Pass`].
    #[must_use]
    pub fn passed(&self) -> usize {
        self.passed
    }

    /// Cells with [`Verdict::Fail`].
    #[must_use]
    pub fn failed(&self) -> usize {
        self.failed
    }

    /// Cells with [`Verdict::Unknown`].
    #[must_use]
    pub fn unknown(&self) -> usize {
        self.unknown
    }

    /// Cells with [`Verdict::Waived`].
    #[must_use]
    pub fn waived(&self) -> usize {
        self.waived
    }

    /// Denominator cells with no record.
    #[must_use]
    pub fn missing(&self) -> usize {
        self.missing
    }

    /// The authority profile shared by every contributing record.
    ///
    /// `None` when no record contributes or when two contributing records
    /// differ in profile, so a mixed board is never reported under one
    /// record's profile. Render it with [`AuthorityProfile::as_str`].
    #[must_use]
    pub fn authority_profile(&self) -> Option<AuthorityProfile> {
        self.authority_profile
    }

    /// `None` when incomplete, mixed-identity, or product with no committed corpus.
    #[must_use]
    pub fn unweighted_percent(&self) -> Option<u8> {
        self.unweighted_percent
    }

    /// True only when `N > 0`, every committed cell passed, identities match,
    /// and a product panel names a documented committed corpus (T1: never).
    #[must_use]
    pub fn complete(&self) -> bool {
        self.complete
    }

    /// `{passed}/{N} of {panel} corpus {id}@{digest} ({kind})`.
    #[must_use]
    pub fn claim(&self) -> &str {
        &self.claim
    }
}

/// 32-bit Mach-O fat (`FAT_MAGIC`).
const FAT_MAGIC: [u8; 4] = [0xCA, 0xFE, 0xBA, 0xBE];
/// 32-bit Mach-O fat, opposite endian (`FAT_CIGAM`).
const FAT_CIGAM: [u8; 4] = [0xBE, 0xBA, 0xFE, 0xCA];
/// 64-bit Mach-O fat (`FAT_MAGIC_64`).
const FAT_MAGIC_64: [u8; 4] = [0xCA, 0xFE, 0xBA, 0xBF];
/// 64-bit Mach-O fat, opposite endian (`FAT_CIGAM_64`).
const FAT_CIGAM_64: [u8; 4] = [0xBF, 0xBA, 0xFE, 0xCA];

/// Magic-byte class of a shipped file prefix. Not a compatibility verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactClass {
    /// Mach-O thin or fat.
    MachO,
    /// PE (`MZ`).
    Pe,
    /// ELF.
    Elf,
    /// WebAssembly (`\0asm`).
    Wasm,
    /// Unrecognized or too short.
    Unknown,
}

/// Classify a shipped artifact by magic bytes. Import success is not returned.
#[must_use]
pub fn classify_artifact(prefix: &[u8]) -> ArtifactClass {
    if prefix.len() >= 4 {
        let mag = [prefix[0], prefix[1], prefix[2], prefix[3]];
        if mag == [0xFE, 0xED, 0xFA, 0xCE]
            || mag == [0xFE, 0xED, 0xFA, 0xCF]
            || mag == [0xCE, 0xFA, 0xED, 0xFE]
            || mag == [0xCF, 0xFA, 0xED, 0xFE]
            || mag == FAT_MAGIC
            || mag == FAT_CIGAM
            || mag == FAT_MAGIC_64
            || mag == FAT_CIGAM_64
        {
            return ArtifactClass::MachO;
        }
        if mag == [0x7F, b'E', b'L', b'F'] {
            return ArtifactClass::Elf;
        }
        if mag == [0x00, b'a', b's', b'm'] {
            return ArtifactClass::Wasm;
        }
    }
    if prefix.len() >= 2 && prefix[0] == b'M' && prefix[1] == b'Z' {
        return ArtifactClass::Pe;
    }
    ArtifactClass::Unknown
}

#[cfg(test)]
mod tests;
