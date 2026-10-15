//! Versioned compatibility evidence records and denominator scoring (KEL-74).
//!
//! Framework-generic: this module does not name Electron APIs, VS Code
//! extensions, or package ecosystems. Those corpora consume the schema later.

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;

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

/// Parse one `keld.compat.evidence/v1` object.
///
/// # Errors
///
/// Returns [`EvidenceError`] for oversize, encoding, JSON, schema, or field
/// violations, including non-normative evidence URIs.
pub fn parse_evidence(bytes: &[u8]) -> Result<EvidenceRecord, EvidenceError> {
    let raw: RawEvidence = parse_object(bytes, MAX_EVIDENCE_BYTES)?;
    if raw.schema != EVIDENCE_SCHEMA {
        return Err(EvidenceError::UnsupportedSchema { found: raw.schema });
    }
    let artifact = ArtifactIdentity {
        sha256: parse_digest(&raw.artifact.sha256)?,
        platform: parse_platform(&raw.artifact.platform)?,
        arch: parse_arch(&raw.artifact.arch)?,
    };
    let revisions = Revisions {
        keld: pinned_revision("revisions.keld", &raw.revisions.keld)?,
        bun: pinned_revision("revisions.bun", &raw.revisions.bun)?,
        engine: pinned_revision("revisions.engine", &raw.revisions.engine)?,
    };
    let operation = Operation {
        id: closed_token("operation.id", &raw.operation.id)?,
        kind: parse_kind(&raw.operation.kind)?,
        oracle: SemanticOracle {
            id: required_token("operation.oracle.id", &raw.operation.oracle.id)?,
            revision: pinned_revision("operation.oracle.revision", &raw.operation.oracle.revision)?,
        },
    };
    let result = parse_verdict(&raw.result)?;
    let waiver = match (&result, raw.waiver) {
        (Verdict::Waived, Some(w)) => Some(parse_waiver(&w)?),
        (Verdict::Waived, None) => {
            return Err(EvidenceError::InvalidWaiver {
                detail: "result is waived but waiver is missing".to_owned(),
            });
        }
        (_, Some(_)) => {
            return Err(EvidenceError::InvalidWaiver {
                detail: "waiver is only allowed when result is waived".to_owned(),
            });
        }
        (_, None) => None,
    };
    Ok(EvidenceRecord {
        artifact,
        revisions,
        authority_profile: parse_authority(&raw.authority_profile)?,
        operation,
        result,
        waiver,
        evidence_uri: parse_evidence_uri(&raw.evidence_uri)?,
    })
}

/// Parse one `keld.compat.denominator/v1` object.
///
/// # Errors
///
/// Returns [`EvidenceError`] for oversize, encoding, JSON, schema, empty
/// cell lists, or duplicate cells.
pub fn parse_denominator(bytes: &[u8]) -> Result<Denominator, EvidenceError> {
    let raw: RawDenominator = parse_object(bytes, MAX_DENOMINATOR_BYTES)?;
    if raw.schema != DENOMINATOR_SCHEMA {
        return Err(EvidenceError::UnsupportedSchema { found: raw.schema });
    }
    let mut cells = Vec::with_capacity(raw.cells.len());
    for cell in raw.cells {
        cells.push(CellKey {
            operation_id: closed_token("cells.operation_id", &cell.operation_id)?,
            oracle_id: required_token("cells.oracle_id", &cell.oracle_id)?,
        });
    }
    validate_denominator_cells(&cells)?;
    Ok(Denominator {
        panel: parse_panel(&raw.panel)?,
        corpus_id: closed_token("corpus_id", &raw.corpus_id)?,
        corpus_sha256: parse_digest(&raw.corpus_sha256)?,
        kind: parse_kind(&raw.kind)?,
        cells,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEvidence {
    schema: String,
    artifact: RawArtifact,
    revisions: RawRevisions,
    authority_profile: String,
    operation: RawOperation,
    result: String,
    #[serde(default)]
    waiver: Option<RawWaiver>,
    evidence_uri: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawArtifact {
    sha256: String,
    platform: String,
    arch: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRevisions {
    keld: String,
    bun: String,
    engine: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOperation {
    id: String,
    kind: String,
    oracle: RawOracle,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOracle {
    id: String,
    revision: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWaiver {
    owner: String,
    reason: String,
    expires_on: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDenominator {
    schema: String,
    panel: String,
    corpus_id: String,
    corpus_sha256: String,
    kind: String,
    cells: Vec<RawCell>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCell {
    operation_id: String,
    oracle_id: String,
}

fn parse_object<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
    max: usize,
) -> Result<T, EvidenceError> {
    if bytes.len() > max {
        return Err(EvidenceError::TooLarge {
            len: bytes.len(),
            max,
        });
    }
    if bytes.is_empty() {
        return Err(EvidenceError::InvalidJson {
            detail: "empty input".to_owned(),
        });
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Err(EvidenceError::NotUtf8);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| EvidenceError::NotUtf8)?;
    serde_json::from_str(text).map_err(|err| {
        let msg = err.to_string();
        if msg.contains("unknown field") || msg.contains("missing field") {
            EvidenceError::InvalidRecord { detail: msg }
        } else {
            EvidenceError::InvalidJson { detail: msg }
        }
    })
}

fn parse_digest(value: &str) -> Result<String, EvidenceError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(EvidenceError::InvalidRecord {
            detail: format!("digest `{value}` must start with sha256:"),
        });
    };
    if hex.len() != 64 || !hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(EvidenceError::InvalidRecord {
            detail: format!("digest `{value}` must be sha256: plus 64 lowercase hex chars"),
        });
    }
    Ok(value.to_owned())
}

fn parse_platform(value: &str) -> Result<Platform, EvidenceError> {
    match value {
        "macos" => Ok(Platform::Macos),
        "windows" => Ok(Platform::Windows),
        "linux" => Ok(Platform::Linux),
        other => Err(EvidenceError::InvalidRecord {
            detail: format!("platform `{other}` is not macos|windows|linux"),
        }),
    }
}

fn parse_arch(value: &str) -> Result<Arch, EvidenceError> {
    match value {
        "aarch64" => Ok(Arch::Aarch64),
        "x86_64" => Ok(Arch::X86_64),
        other => Err(EvidenceError::InvalidRecord {
            detail: format!("arch `{other}` is not aarch64|x86_64"),
        }),
    }
}

fn parse_authority(value: &str) -> Result<AuthorityProfile, EvidenceError> {
    match value {
        "strict_bun" => Ok(AuthorityProfile::StrictBun),
        "sandboxed_addon_worker" => Ok(AuthorityProfile::SandboxedAddonWorker),
        "legacy_sandbox_off" => Ok(AuthorityProfile::LegacySandboxOff),
        "user_approved_tool_child" => Ok(AuthorityProfile::UserApprovedToolChild),
        "unverified" => Ok(AuthorityProfile::Unverified),
        other => Err(EvidenceError::InvalidRecord {
            detail: format!("unknown authority_profile `{other}`"),
        }),
    }
}

fn parse_kind(value: &str) -> Result<OperationKind, EvidenceError> {
    match value {
        "install" => Ok(OperationKind::Install),
        "activation" => Ok(OperationKind::Activation),
        "primary_workflow" => Ok(OperationKind::PrimaryWorkflow),
        "full_feature" => Ok(OperationKind::FullFeature),
        other => Err(EvidenceError::InvalidRecord {
            detail: format!("kind `{other}` is not a v1 denominator kind"),
        }),
    }
}

fn parse_panel(value: &str) -> Result<Panel, EvidenceError> {
    match value {
        "product" => Ok(Panel::Product),
        "showcase" => Ok(Panel::Showcase),
        other => Err(EvidenceError::InvalidRecord {
            detail: format!("panel `{other}` is not product|showcase"),
        }),
    }
}

fn parse_verdict(value: &str) -> Result<Verdict, EvidenceError> {
    match value {
        "pass" => Ok(Verdict::Pass),
        "fail" => Ok(Verdict::Fail),
        "unknown" => Ok(Verdict::Unknown),
        "waived" => Ok(Verdict::Waived),
        other => Err(EvidenceError::InvalidRecord {
            detail: format!("result `{other}` is not pass|fail|unknown|waived"),
        }),
    }
}

fn parse_waiver(raw: &RawWaiver) -> Result<Waiver, EvidenceError> {
    let owner = raw.owner.trim();
    let reason = raw.reason.trim();
    if owner.is_empty() || reason.is_empty() {
        return Err(EvidenceError::InvalidWaiver {
            detail: "owner and reason must be non-empty".to_owned(),
        });
    }
    Ok(Waiver {
        owner: owner.to_owned(),
        reason: reason.to_owned(),
        expires_on: CivilDate::parse(&raw.expires_on)?,
    })
}

fn pinned_revision(field: &str, value: &str) -> Result<String, EvidenceError> {
    let trimmed = required_token(field, value)?;
    if trimmed.eq_ignore_ascii_case("latest") {
        return Err(EvidenceError::InvalidRecord {
            detail: format!("{field} must be a pinned revision, not `latest`"),
        });
    }
    Ok(trimmed)
}

fn required_token(field: &str, value: &str) -> Result<String, EvidenceError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EvidenceError::InvalidRecord {
            detail: format!("{field} must be non-empty"),
        });
    }
    Ok(trimmed.to_owned())
}

fn closed_token(field: &str, value: &str) -> Result<String, EvidenceError> {
    let trimmed = required_token(field, value)?;
    if !trimmed
        .bytes()
        .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
    {
        return Err(EvidenceError::InvalidRecord {
            detail: format!("{field} `{trimmed}` must match [a-z0-9._-]"),
        });
    }
    Ok(trimmed)
}

fn validate_denominator_cells(cells: &[CellKey]) -> Result<(), EvidenceError> {
    if cells.is_empty() {
        return Err(EvidenceError::InvalidDenominator {
            detail: "cells must contain at least one entry".to_owned(),
        });
    }
    let mut seen = BTreeSet::new();
    for cell in cells {
        if !seen.insert(cell) {
            return Err(EvidenceError::InvalidDenominator {
                detail: format!("duplicate cell `{}`", cell.label()),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
