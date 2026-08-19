//! Versioned compatibility evidence records and denominator scoring (KEL-74).
//!
//! Framework-generic: this module does not name Electron APIs, VS Code
//! extensions, or package ecosystems. Those corpora consume the schema later.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};

use serde::Deserialize;

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

/// Score records against a committed denominator.
///
/// Extra records whose cell is not in the denominator are ignored. Records
/// whose `operation.kind` differs from the denominator kind are ignored so a
/// foreign kind cannot fill a cell.
///
/// # Errors
///
/// Returns [`EvidenceError::InvalidDenominator`] when `cells` is empty or
/// contains duplicates, [`EvidenceError::DuplicateCell`] when two records
/// name the same cell, [`EvidenceError::InvalidWaiver`] when a waiver is
/// expired as of `as_of` or paired with a non-[`Verdict::Waived`] result,
/// or [`EvidenceError::NonNormativeEvidence`] when a constructed URI is a
/// lead rather than an immutable pin.
pub fn score(
    denominator: &Denominator,
    records: &[EvidenceRecord],
    as_of: CivilDate,
) -> Result<Scoreboard, EvidenceError> {
    validate_denominator_cells(&denominator.cells)?;
    let required: BTreeSet<&CellKey> = denominator.cells.iter().collect();
    let mut by_cell: BTreeMap<CellKey, &EvidenceRecord> = BTreeMap::new();
    for record in records {
        if record.operation.kind != denominator.kind {
            continue;
        }
        let key = record.cell_key();
        if !required.contains(&key) {
            continue;
        }
        validate_scored_record(record, &key, as_of)?;
        if by_cell.insert(key.clone(), record).is_some() {
            return Err(EvidenceError::DuplicateCell { cell: key.label() });
        }
    }

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut unknown = 0usize;
    let mut waived = 0usize;
    let mut missing = 0usize;
    for cell in &denominator.cells {
        match by_cell.get(cell).map(|record| record.result) {
            Some(Verdict::Pass) => passed += 1,
            Some(Verdict::Fail) => failed += 1,
            Some(Verdict::Unknown) => unknown += 1,
            Some(Verdict::Waived) => waived += 1,
            None => missing += 1,
        }
    }

    let identity_ok = contributing_identity_consistent(by_cell.values().copied());
    let n = denominator.cells.len();
    let corpus_ok =
        product_corpus_is_documented_committed(denominator.panel, &denominator.corpus_id);
    let may_publish_percent = missing == 0 && unknown == 0 && n > 0 && identity_ok && corpus_ok;
    let unweighted_percent = if may_publish_percent {
        let pct = passed.saturating_mul(100) / n;
        u8::try_from(pct).ok()
    } else {
        None
    };
    let complete = n > 0
        && passed == n
        && missing == 0
        && unknown == 0
        && failed == 0
        && waived == 0
        && identity_ok
        && corpus_ok;
    let claim = format!(
        "{passed}/{n} of {} corpus {}@{} ({})",
        denominator.panel.as_str(),
        denominator.corpus_id,
        denominator.corpus_sha256,
        denominator.kind.as_str()
    );

    Ok(Scoreboard {
        panel: denominator.panel,
        corpus_id: denominator.corpus_id.clone(),
        corpus_sha256: denominator.corpus_sha256.clone(),
        kind: denominator.kind,
        denominator: n,
        passed,
        failed,
        unknown,
        waived,
        missing,
        unweighted_percent,
        complete,
        claim,
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

fn parse_evidence_uri(uri: &str) -> Result<String, EvidenceError> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: "empty evidence_uri".to_owned(),
        });
    }
    if is_lead_not_location(trimmed) {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: format!("`{trimmed}` is a sandbox path or opaque turn citation"),
        });
    }
    if trimmed.starts_with("sha256:") {
        parse_digest(trimmed)?;
        return Ok(trimmed.to_owned());
    }
    if let Some(rest) = trimmed.strip_prefix("https://") {
        let (host, path) = parse_https_host_and_path(rest, trimmed)?;
        // Trailing FQDN dots are the same host (RFC 1034); compare once.
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        if host_is_forbidden(&host) {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!("`{trimmed}` is not a public https location"),
            });
        }
        if host.parse::<Ipv4Addr>().is_err()
            && host.parse::<Ipv6Addr>().is_err()
            && !host.contains('.')
        {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!("`{trimmed}` is not a resolvable https URL with a path"),
            });
        }
        if path.is_empty() || path == "/" {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!("`{trimmed}` is not a resolvable https URL with a path"),
            });
        }
        if https_path_is_live_mutable(&host, &path) {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!(
                    "`{trimmed}` has a live branch/tag path; \
                     a mutable URL is not an immutable pin"
                ),
            });
        }
        if !path_has_git_object_id(&path) {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!(
                    "`{trimmed}` has no git object id in the path; \
                     a live-mutable URL is not an immutable pin"
                ),
            });
        }
        return Ok(trimmed.to_owned());
    }
    Err(EvidenceError::NonNormativeEvidence {
        detail: format!("`{trimmed}` is not https:// or sha256:"),
    })
}

/// Turn citations and local temp *paths*. Not a substring search on https URLs.
fn is_lead_not_location(uri: &str) -> bool {
    let lower = uri.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("sha256:") {
        return false;
    }
    lower.starts_with("turn")
        || lower.starts_with("file:")
        || lower.starts_with("sandbox:")
        || lower.starts_with("/tmp/")
        || lower.starts_with("/private/tmp/")
        || lower.starts_with("/var/folders/")
        || lower.starts_with("\\temp\\")
        || lower.contains(":\\users\\") && lower.contains("\\appdata\\local\\temp")
}

fn parse_https_host_and_path(
    rest: &str,
    original: &str,
) -> Result<(String, String), EvidenceError> {
    let without_hash = rest.split_once('#').map_or(rest, |(p, _)| p);
    let without_query = without_hash
        .split_once('?')
        .map_or(without_hash, |(p, _)| p);
    if without_query.is_empty() || authority_has_userinfo(without_query) {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: format!("`{original}` is not a public https location"),
        });
    }
    if without_query.starts_with('[') {
        let end = without_query
            .find(']')
            .ok_or_else(|| EvidenceError::NonNormativeEvidence {
                detail: format!("`{original}` is not a resolvable https URL with a path"),
            })?;
        let host = without_query[1..end].to_owned();
        let after = &without_query[end + 1..];
        let path = match after.strip_prefix(':') {
            Some(port_and_path) => {
                let path_at =
                    port_and_path
                        .find('/')
                        .ok_or_else(|| EvidenceError::NonNormativeEvidence {
                            detail: format!(
                                "`{original}` is not a resolvable https URL with a path"
                            ),
                        })?;
                let port = &port_and_path[..path_at];
                if !https_port_is_valid(port) {
                    return Err(EvidenceError::NonNormativeEvidence {
                        detail: format!("`{original}` is not a public https location"),
                    });
                }
                port_and_path[path_at..].to_owned()
            }
            None => {
                if after.is_empty() {
                    String::new()
                } else if let Some(path) = after.strip_prefix('/') {
                    format!("/{path}")
                } else {
                    return Err(EvidenceError::NonNormativeEvidence {
                        detail: format!("`{original}` is not a resolvable https URL with a path"),
                    });
                }
            }
        };
        return Ok((host, path));
    }
    if without_query.matches(':').count() > 1 {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: format!("`{original}` is not a resolvable https URL with a path"),
        });
    }
    let slash = without_query.find('/');
    let hostport = slash.map_or(without_query, |i| &without_query[..i]);
    if hostport.is_empty() {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: format!("`{original}` is not a public https location"),
        });
    }
    let host = match hostport.rsplit_once(':') {
        Some((h, port)) => {
            if h.is_empty() || !https_port_is_valid(port) {
                return Err(EvidenceError::NonNormativeEvidence {
                    detail: format!("`{original}` is not a public https location"),
                });
            }
            h.to_owned()
        }
        None => hostport.to_owned(),
    };
    let path = slash.map_or(String::new(), |i| without_query[i..].to_owned());
    Ok((host, path))
}

fn authority_has_userinfo(rest: &str) -> bool {
    let mut in_brackets = false;
    for byte in rest.bytes() {
        match byte {
            b'[' => in_brackets = true,
            b']' => in_brackets = false,
            b'/' if !in_brackets => return false,
            b'@' if !in_brackets => return true,
            _ => {}
        }
    }
    false
}

/// Decimal port that fits in `u16`. Non-numeric and `65536` are not public https.
fn https_port_is_valid(port: &str) -> bool {
    !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok()
}

fn host_is_forbidden(host: &str) -> bool {
    // Browsers treat a trailing FQDN dot as the same host; `Ipv4Addr` does not.
    // Literal authority only: no DNS, and no abbreviated-IPv4 parser
    // (`127.1`, `010.0.0.1`, `1.1`) — those plus DNS-to-private (`nip.io`)
    // are T1 residuals (easy to mis-classify a public address).
    let h = host.to_ascii_lowercase();
    let h = h.trim_end_matches('.');
    if h.is_empty() || h == "localhost" || h.ends_with(".localhost") {
        return true;
    }
    if let Ok(v4) = h.parse::<Ipv4Addr>() {
        return ipv4_is_non_public(v4);
    }
    if let Ok(v6) = h.parse::<Ipv6Addr>() {
        return ipv6_is_non_public(v6);
    }
    false
}

fn ipv4_is_non_public(v4: Ipv4Addr) -> bool {
    v4.is_loopback()
        || v4.is_unspecified()
        || v4.is_private()
        || v4.is_link_local()
        || ipv4_is_cgnat(v4)
}

/// RFC 6598 shared address space `100.64.0.0/10` (CGNAT).
/// `Ipv4Addr::is_shared` is still unstable (`feature(ip)`).
fn ipv4_is_cgnat(v4: Ipv4Addr) -> bool {
    let [a, b, ..] = v4.octets();
    a == 100 && (64..128).contains(&b)
}

fn ipv6_is_non_public(v6: Ipv6Addr) -> bool {
    if v6.is_loopback() || v6.is_unspecified() || v6.is_unique_local() || v6.is_unicast_link_local()
    {
        return true;
    }
    if ipv6_is_nat64_well_known(v6) || ipv6_is_6to4(v6) {
        return true;
    }
    ipv6_embedded_ipv4(v6).is_some_and(ipv4_is_non_public)
}

/// RFC 6052 well-known NAT64 prefix `64:ff9b::/96`.
fn ipv6_is_nat64_well_known(v6: Ipv6Addr) -> bool {
    let o = v6.octets();
    o[..12] == [0x00, 0x64, 0xff, 0x9b, 0, 0, 0, 0, 0, 0, 0, 0]
}

/// RFC 3056 6to4 prefix `2002::/16`.
fn ipv6_is_6to4(v6: Ipv6Addr) -> bool {
    let o = v6.octets();
    o[0] == 0x20 && o[1] == 0x02
}

/// IPv4-mapped (`::ffff:a.b.c.d`), deprecated IPv4-compatible (`::a.b.c.d`),
/// and IPv4-translated (`::ffff:0:a.b.c.d`, RFC 2765). `::` / `::1` are
/// classified before this runs.
fn ipv6_embedded_ipv4(v6: Ipv6Addr) -> Option<Ipv4Addr> {
    if let Some(v4) = v6.to_ipv4_mapped() {
        return Some(v4);
    }
    let o = v6.octets();
    let tail = Ipv4Addr::new(o[12], o[13], o[14], o[15]);
    let ipv4_compatible = o[..12] == [0; 12];
    let ipv4_translated =
        o[..8] == [0; 8] && o[8] == 0xff && o[9] == 0xff && o[10] == 0 && o[11] == 0;
    (ipv4_compatible || ipv4_translated).then_some(tail)
}

fn path_has_git_object_id(path: &str) -> bool {
    path.split('/').any(is_git_object_id)
}

/// `/blob/<branch>/`, `/tree/<branch>/`, `/raw/<branch>/`, and GitHub raw CDN
/// `/{owner}/{repo}/{branch}/…` stay live-mutable even when a later segment is
/// 40- or 64-hex. Commit-pinned `/blob/<object-id>/` is not this case.
fn https_path_is_live_mutable(host: &str, path: &str) -> bool {
    if path_has_live_git_ref(path) {
        return true;
    }
    let host = host.to_ascii_lowercase();
    let host = host.trim_end_matches('.');
    if host == "raw.githubusercontent.com" {
        let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        return segs
            .get(2)
            .is_some_and(|git_ref| !is_git_object_id(git_ref));
    }
    false
}

fn path_has_live_git_ref(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segments.windows(2).any(|pair| {
        let kind = pair[0].to_ascii_lowercase();
        matches!(kind.as_str(), "blob" | "tree" | "raw") && !is_git_object_id(pair[1])
    })
}

fn is_git_object_id(segment: &str) -> bool {
    (segment.len() == 40 || segment.len() == 64)
        && segment
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
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

fn validate_scored_record(
    record: &EvidenceRecord,
    key: &CellKey,
    as_of: CivilDate,
) -> Result<(), EvidenceError> {
    validate_scored_waiver(record, key, as_of)?;
    parse_evidence_uri(&record.evidence_uri)?;
    Ok(())
}

fn validate_scored_waiver(
    record: &EvidenceRecord,
    key: &CellKey,
    as_of: CivilDate,
) -> Result<(), EvidenceError> {
    match (record.result, record.waiver.as_ref()) {
        (Verdict::Waived, Some(waiver)) => {
            if waiver.expires_on < as_of {
                return Err(EvidenceError::InvalidWaiver {
                    detail: format!(
                        "waiver for `{}` expired on {} (as_of {as_of})",
                        key.label(),
                        waiver.expires_on
                    ),
                });
            }
            Ok(())
        }
        (Verdict::Waived, None) => Err(EvidenceError::InvalidWaiver {
            detail: "result is waived but waiver is missing".to_owned(),
        }),
        (_, Some(_)) => Err(EvidenceError::InvalidWaiver {
            detail: "waiver is only allowed when result is waived".to_owned(),
        }),
        (_, None) => Ok(()),
    }
}

fn contributing_identity_consistent<'a>(
    mut records: impl Iterator<Item = &'a EvidenceRecord>,
) -> bool {
    let Some(first) = records.next() else {
        return true;
    };
    records.all(|record| {
        record.artifact.sha256 == first.artifact.sha256
            && record.authority_profile == first.authority_profile
            && record.revisions.engine == first.revisions.engine
    })
}

/// Documented committed product corpus ids. T1: none, so product panels
/// never publish `unweighted_percent` or `complete`.
const DOCUMENTED_COMMITTED_PRODUCT_CORPORA: &[&str] = &[];

/// Showcase may publish. Product may publish only when `corpus_id` is on
/// `DOCUMENTED_COMMITTED_PRODUCT_CORPORA` (empty today).
fn product_corpus_is_documented_committed(panel: Panel, corpus_id: &str) -> bool {
    match panel {
        Panel::Showcase => true,
        Panel::Product => DOCUMENTED_COMMITTED_PRODUCT_CORPORA.contains(&corpus_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST_A: &str =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DIGEST_B: &str =
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const AS_OF: CivilDate = CivilDate {
        year: 2026,
        month: 8,
        day: 19,
    };

    fn valid_evidence_json() -> String {
        format!(
            r#"{{
  "schema": "keld.compat.evidence/v1",
  "artifact": {{
    "sha256": "{DIGEST_A}",
    "platform": "macos",
    "arch": "aarch64"
  }},
  "revisions": {{
    "keld": "67f39cdc898254f1e0c9cd50800f242ae7a4c493",
    "bun": "1.3.14",
    "engine": "wkwebview"
  }},
  "authority_profile": "strict_bun",
  "operation": {{
    "id": "hello.window.open",
    "kind": "primary_workflow",
    "oracle": {{
      "id": "hello-window-visible",
      "revision": "kel26-macos"
    }}
  }},
  "result": "pass",
  "evidence_uri": "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493"
}}"#
        )
    }

    fn valid_denominator_json() -> String {
        format!(
            r#"{{
  "schema": "keld.compat.denominator/v1",
  "panel": "product",
  "corpus_id": "phase2-hello",
  "corpus_sha256": "{DIGEST_B}",
  "kind": "primary_workflow",
  "cells": [
    {{ "operation_id": "hello.window.open", "oracle_id": "hello-window-visible" }},
    {{ "operation_id": "hello.ipc.echo", "oracle_id": "kipc-echo-roundtrip" }}
  ]
}}"#
        )
    }

    fn parse_ok() -> EvidenceRecord {
        parse_evidence(valid_evidence_json().as_bytes()).expect("valid evidence")
    }

    fn second_pass() -> EvidenceRecord {
        let mut json = valid_evidence_json();
        json = json.replace("hello.window.open", "hello.ipc.echo");
        json = json.replace("hello-window-visible", "kipc-echo-roundtrip");
        parse_evidence(json.as_bytes()).expect("second cell")
    }

    #[test]
    fn parse_evidence_accepts_closed_v1_record() {
        let record = parse_ok();
        assert_eq!(record.artifact.sha256, DIGEST_A);
        assert_eq!(record.artifact.platform, Platform::Macos);
        assert_eq!(record.artifact.arch, Arch::Aarch64);
        assert_eq!(record.revisions.bun, "1.3.14");
        assert_eq!(record.authority_profile, AuthorityProfile::StrictBun);
        assert_eq!(record.operation.kind, OperationKind::PrimaryWorkflow);
        assert_eq!(record.result, Verdict::Pass);
        assert!(record.waiver.is_none());
    }

    #[test]
    fn parse_rejects_oversize() {
        let mut bytes = valid_evidence_json().into_bytes();
        bytes.resize(MAX_EVIDENCE_BYTES + 1, b' ');
        let err = parse_evidence(&bytes).expect_err("oversize");
        assert_eq!(err.code(), "KELD-COMPAT-001");
        assert!(err.to_string().contains("Split the ledger"));
    }

    #[test]
    fn parse_rejects_non_utf8() {
        let err = parse_evidence(&[0xFF, 0xFE, 0x00]).expect_err("latin1");
        assert_eq!(err.code(), "KELD-COMPAT-002");
        assert!(err.to_string().contains("Re-encode"));
    }

    #[test]
    fn parse_rejects_trailing_junk() {
        let mut json = valid_evidence_json();
        json.push_str(" true");
        let err = parse_evidence(json.as_bytes()).expect_err("junk");
        assert_eq!(err.code(), "KELD-COMPAT-003");
        assert!(err.to_string().contains("no trailing bytes"));
    }

    #[test]
    fn parse_rejects_unknown_schema() {
        let json = valid_evidence_json().replace(EVIDENCE_SCHEMA, "keld.compat.evidence/v0");
        let err = parse_evidence(json.as_bytes()).expect_err("schema");
        assert_eq!(err.code(), "KELD-COMPAT-004");
        assert!(err.to_string().contains(EVIDENCE_SCHEMA));
    }

    #[test]
    fn parse_rejects_unknown_field_and_bad_digest() {
        let extra = valid_evidence_json().replacen('{', r#"{"extra":1,"#, 1);
        let err = parse_evidence(extra.as_bytes()).expect_err("unknown field");
        assert_eq!(err.code(), "KELD-COMPAT-005");
        let bad = valid_evidence_json().replace(DIGEST_A, "sha256:DEADBEEF");
        let err = parse_evidence(bad.as_bytes()).expect_err("digest");
        assert_eq!(err.code(), "KELD-COMPAT-005");
        assert!(err.to_string().contains("closed field set"));
    }

    #[test]
    fn waived_requires_owner_reason_expiry() {
        let json = valid_evidence_json().replace(r#""result": "pass""#, r#""result": "waived""#);
        let err = parse_evidence(json.as_bytes()).expect_err("missing waiver");
        assert_eq!(err.code(), "KELD-COMPAT-006");
        assert!(err.to_string().contains("owner, reason"));
    }

    #[test]
    fn waiver_forbidden_on_pass() {
        let mut json = valid_evidence_json();
        json = json.replacen(
            r#""result": "pass""#,
            r#""result": "pass",
  "waiver": {"owner":"a","reason":"b","expires_on":"2026-12-31"}"#,
            1,
        );
        let err = parse_evidence(json.as_bytes()).expect_err("extra waiver");
        assert_eq!(err.code(), "KELD-COMPAT-006");
    }

    #[test]
    fn civil_date_rejects_signed_chunks() {
        // Rust integer FromStr accepts a leading '+'; YYYY-MM-DD must not.
        let err = CivilDate::parse("+026-+1-+1").expect_err("signed chunks");
        assert_eq!(err.code(), "KELD-COMPAT-006");
        assert!(err.to_string().contains("YYYY-MM-DD"), "{err}");
        assert!(CivilDate::parse("2026-08-19").is_ok());
    }

    #[test]
    fn parse_accepts_waived_with_complete_waiver() {
        let mut json = valid_evidence_json();
        json = json.replace(r#""result": "pass""#, r#""result": "waived""#);
        json = json.replacen(
            r#""evidence_uri""#,
            r#""waiver": {"owner":"release","reason":"engine gap","expires_on":"2026-12-31"},
  "evidence_uri""#,
            1,
        );
        let record = parse_evidence(json.as_bytes()).expect("waived");
        assert_eq!(record.result, Verdict::Waived);
        assert_eq!(record.waiver.expect("waiver").owner, "release");
    }

    #[test]
    fn rejects_turn_citation_and_tmp_path() {
        for uri in ["turn0file3", "file:///tmp/out.json", "/tmp/keld-out.json"] {
            let json = valid_evidence_json().replace(
                "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
                uri,
            );
            let err = parse_evidence(json.as_bytes()).expect_err(uri);
            assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
            assert!(err.to_string().contains("non-normative leads"), "{uri}");
        }
    }

    #[test]
    fn denominator_rejects_empty_and_duplicate_cells() {
        let empty = valid_denominator_json().replace(
            r#"cells": [
    { "operation_id": "hello.window.open", "oracle_id": "hello-window-visible" },
    { "operation_id": "hello.ipc.echo", "oracle_id": "kipc-echo-roundtrip" }
  ]"#,
            r#"cells": []"#,
        );
        let err = parse_denominator(empty.as_bytes()).expect_err("empty");
        assert_eq!(err.code(), "KELD-COMPAT-008");
        let dup = valid_denominator_json().replace("hello.ipc.echo", "hello.window.open");
        let dup = dup.replace("kipc-echo-roundtrip", "hello-window-visible");
        let err = parse_denominator(dup.as_bytes()).expect_err("dup");
        assert_eq!(err.code(), "KELD-COMPAT-008");
        assert!(err.to_string().contains("unique cells"));
    }

    #[test]
    fn partial_corpus_refuses_percent_and_complete() {
        let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        let board = score(&denom, &[parse_ok()], AS_OF).expect("score");
        assert_eq!(board.passed, 1);
        assert_eq!(board.missing, 1);
        assert_eq!(board.denominator, 2);
        assert_eq!(board.unweighted_percent, None);
        assert!(!board.complete);
        assert_eq!(
            board.claim,
            format!("1/2 of product corpus phase2-hello@{DIGEST_B} (primary_workflow)")
        );
        assert!(!board.claim.contains("100% compatible"));
        assert!(!board.claim.contains("fully compatible"));
    }

    #[test]
    fn full_pass_is_complete_and_still_names_denominator() {
        let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        denom.panel = Panel::Showcase;
        let board = score(&denom, &[parse_ok(), second_pass()], AS_OF).expect("score");
        assert_eq!(board.unweighted_percent, Some(100));
        assert!(board.complete);
        assert_eq!(
            board.claim,
            format!("2/2 of showcase corpus phase2-hello@{DIGEST_B} (primary_workflow)")
        );
        assert!(!board.claim.contains("100% compatible"));
    }

    #[test]
    fn extra_records_cannot_shrink_denominator() {
        let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        let mut stray = valid_evidence_json();
        stray = stray.replace("hello.window.open", "other.op");
        stray = stray.replace("hello-window-visible", "other-oracle");
        let stray = parse_evidence(stray.as_bytes()).expect("stray");
        let board = score(&denom, &[parse_ok(), stray], AS_OF).expect("score");
        assert_eq!(board.missing, 1);
        assert_eq!(board.unweighted_percent, None);
        assert!(!board.complete);
    }

    #[test]
    fn unknown_cell_refuses_percent() {
        let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        let mut unknown = valid_evidence_json();
        unknown = unknown.replace(r#""result": "pass""#, r#""result": "unknown""#);
        let unknown = parse_evidence(unknown.as_bytes()).expect("unknown");
        let board = score(&denom, &[unknown, second_pass()], AS_OF).expect("score");
        assert_eq!(board.unknown, 1);
        assert_eq!(board.passed, 1);
        assert_eq!(board.unweighted_percent, None);
        assert!(!board.complete);
    }

    #[test]
    fn expired_waiver_is_not_silent() {
        let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        let mut json = valid_evidence_json();
        json = json.replace(r#""result": "pass""#, r#""result": "waived""#);
        json = json.replacen(
            r#""evidence_uri""#,
            r#""waiver": {"owner":"release","reason":"gap","expires_on":"2026-01-01"},
  "evidence_uri""#,
            1,
        );
        let waived = parse_evidence(json.as_bytes()).expect("parse waiver");
        let err = score(&denom, &[waived], AS_OF).expect_err("expired");
        assert_eq!(err.code(), "KELD-COMPAT-006");
    }

    #[test]
    fn empty_cells_score_is_not_complete_and_not_one_hundred_percent() {
        // parse_denominator already rejects `cells: []`. score must too: a
        // hand-built empty Vec used to yield passed==n==0, complete=true, "0/0".
        let denom = Denominator {
            panel: Panel::Product,
            corpus_id: "phase2-hello".to_owned(),
            corpus_sha256: DIGEST_B.to_owned(),
            kind: OperationKind::PrimaryWorkflow,
            cells: Vec::new(),
        };
        match score(&denom, &[], AS_OF) {
            Ok(board) => panic!(
                "empty cells must not score: complete={} percent={:?} claim={}",
                board.complete, board.unweighted_percent, board.claim
            ),
            Err(err) => {
                assert_eq!(err.code(), "KELD-COMPAT-008");
                assert!(err.to_string().contains("unique cells"), "{err}");
            }
        }
    }

    #[test]
    fn duplicate_denom_cells_cannot_double_count_one_pass() {
        // parse_denominator rejects duplicate cells; a hand-built vec used to
        // count one Pass twice (2/2 complete from a single record).
        let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        let cell = denom.cells[0].clone();
        denom.cells = vec![cell.clone(), cell];
        match score(&denom, &[parse_ok()], AS_OF) {
            Ok(board) => panic!(
                "duplicate cells must not score: passed={} n={} complete={} percent={:?}",
                board.passed, board.denominator, board.complete, board.unweighted_percent
            ),
            Err(err) => {
                assert_eq!(err.code(), "KELD-COMPAT-008");
                assert!(err.to_string().contains("unique cells"), "{err}");
            }
        }
    }

    #[test]
    fn tmp_and_turn_uri_rejected_at_score() {
        let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        denom.cells.truncate(1);
        for uri in ["turn0file3", "/tmp/keld-out.json", "file:///tmp/out.json"] {
            let mut record = parse_ok();
            record.evidence_uri = uri.to_owned();
            let err = score(&denom, &[record], AS_OF).expect_err(uri);
            assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
        }
    }

    #[test]
    fn mixed_artifact_identity_refuses_complete_and_percent() {
        let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        denom.panel = Panel::Showcase;
        let first = parse_ok();
        let mut second = second_pass();
        second.artifact.sha256 = DIGEST_B.to_owned();
        second.authority_profile = AuthorityProfile::LegacySandboxOff;
        second.revisions.engine = "cef".to_owned();
        let board = score(&denom, &[first, second], AS_OF).expect("mixed stitch still counts");
        assert_eq!(board.passed, 2);
        assert_eq!(board.denominator, 2);
        assert!(
            !board.complete,
            "mixed digest/profile/engine must not be complete"
        );
        assert_eq!(board.unweighted_percent, None);
        assert!(!board.claim.contains("100%"));
    }

    #[test]
    fn product_panel_omits_percent_for_uncommitted_corpus() {
        let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        denom.cells.truncate(1);
        denom.corpus_id = "toy-uncommitted".to_owned();
        let board = score(&denom, &[parse_ok()], AS_OF).expect("score");
        assert_eq!(board.passed, 1);
        assert_eq!(board.denominator, 1);
        assert_eq!(board.unweighted_percent, None);
        assert!(
            !board.complete,
            "uncommitted product 1/1 must not be complete; consumers key off complete"
        );
        assert!(!board.claim.contains("100%"));
        assert!(!board.claim.contains("100% compatible"));
        assert!(!board.claim.contains("fully compatible"));
        assert!(
            board
                .claim
                .contains("1/1 of product corpus toy-uncommitted")
        );
    }

    #[test]
    fn showcase_one_cell_pass_publishes_percent_and_complete() {
        let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        denom.panel = Panel::Showcase;
        denom.cells.truncate(1);
        let board = score(&denom, &[parse_ok()], AS_OF).expect("score");
        assert_eq!(board.unweighted_percent, Some(100));
        assert!(board.complete);
        assert!(board.claim.contains("1/1 of showcase corpus"));
        assert!(!board.claim.contains("100% compatible"));
    }

    #[test]
    fn scoreboard_struct_fields_are_not_public() {
        // Independent of score(): a `pub complete` / `pub unweighted_percent`
        // field is the same minting hole the Denominator list had. rustc
        // privacy is the seal; this test fails if those fields are re-exported.
        let src = include_str!("evidence.rs");
        let marker = "pub struct Scoreboard {";
        let start = src.find(marker).expect("Scoreboard struct");
        let after = &src[start + marker.len()..];
        let end = after.find('}').expect("struct close");
        let fields = &after[..end];
        assert!(
            !fields.contains("pub "),
            "Scoreboard fields must stay private so callers cannot mint 100%/complete:\n{fields}"
        );
        assert!(
            fields.contains("unweighted_percent: Option<u8>"),
            "{fields}"
        );
        assert!(fields.contains("complete: bool"), "{fields}");
    }

    #[test]
    fn constructed_pass_with_unexpired_waiver_is_not_a_pass() {
        // parse_evidence rejects Pass+waiver JSON; public fields let a caller
        // construct it anyway. score must not treat that as a pass.
        let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        denom.cells.truncate(1);
        let mut record = parse_ok();
        record.result = Verdict::Pass;
        record.waiver = Some(Waiver {
            owner: "release".to_owned(),
            reason: "gap".to_owned(),
            expires_on: CivilDate {
                year: 2026,
                month: 12,
                day: 31,
            },
        });
        match score(&denom, &[record], AS_OF) {
            Ok(board) => panic!(
                "Pass+waiver must not score as pass: complete={} passed={} percent={:?}",
                board.complete, board.passed, board.unweighted_percent
            ),
            Err(err) => {
                assert_eq!(err.code(), "KELD-COMPAT-006");
            }
        }
    }

    #[test]
    fn duplicate_cell_records_error() {
        let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        let err = score(&denom, &[parse_ok(), parse_ok()], AS_OF).expect_err("dup");
        assert_eq!(err.code(), "KELD-COMPAT-009");
        assert!(err.to_string().contains("one record"));
    }

    #[test]
    fn classify_artifact_magic_classes() {
        assert_eq!(
            classify_artifact(&[0xCF, 0xFA, 0xED, 0xFE, 0x00]),
            ArtifactClass::MachO
        );
        // FAT_MAGIC / FAT_CIGAM (32-bit fat) and FAT_MAGIC_64 / FAT_CIGAM_64.
        assert_eq!(classify_artifact(&FAT_MAGIC), ArtifactClass::MachO);
        assert_eq!(classify_artifact(&FAT_CIGAM), ArtifactClass::MachO);
        assert_eq!(classify_artifact(&FAT_MAGIC_64), ArtifactClass::MachO);
        assert_eq!(classify_artifact(&FAT_CIGAM_64), ArtifactClass::MachO);
        assert_eq!(classify_artifact(b"MZ\x90\x00"), ArtifactClass::Pe);
        assert_eq!(classify_artifact(b"\x7FELF...."), ArtifactClass::Elf);
        assert_eq!(classify_artifact(b"\0asm\x01\x00"), ArtifactClass::Wasm);
        assert_eq!(classify_artifact(b""), ArtifactClass::Unknown);
        assert_eq!(classify_artifact(b"PK\x03\x04"), ArtifactClass::Unknown);
    }

    #[test]
    fn rejects_mutable_https_without_content_address() {
        for uri in [
            "https://example.com/foo",
            "https://example.com/tmp/not-this",
            "https://github.com/gyldlab/keld/blob/main/README.md",
        ] {
            let json = valid_evidence_json().replace(
                "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
                uri,
            );
            let err = parse_evidence(json.as_bytes()).expect_err(uri);
            assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
            assert!(err.to_string().contains("non-normative leads"), "{uri}");
        }
    }

    #[test]
    fn rejects_loopback_userinfo_and_unspecified_https_hosts() {
        let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
        for uri in [
            format!("https://git@127.0.0.1/org/repo/commit/{sha}"),
            format!("https://[::ffff:127.0.0.1]/org/repo/commit/{sha}"),
            format!("https://[::ffff:7f00:1]/org/repo/commit/{sha}"),
            format!("https://0.0.0.0/org/repo/commit/{sha}"),
            format!("https://127.0.0.1/org/repo/commit/{sha}"),
            format!("https://[::1]/org/repo/commit/{sha}"),
            format!("https://user@github.com/gyldlab/keld/commit/{sha}"),
            format!("https://[::]/org/repo/commit/{sha}"),
        ] {
            let json = valid_evidence_json().replace(
                "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
                &uri,
            );
            let err = parse_evidence(json.as_bytes()).expect_err(&uri);
            assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
        }
    }

    #[test]
    fn rejects_rfc1918_link_local_and_unique_local_https_hosts() {
        let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
        for uri in [
            format!("https://10.0.0.1/org/repo/commit/{sha}"),
            format!("https://192.168.1.1/org/repo/commit/{sha}"),
            format!("https://172.16.0.1/org/repo/commit/{sha}"),
            format!("https://169.254.0.1/org/repo/commit/{sha}"),
            format!("https://[fc00::1]/org/repo/commit/{sha}"),
            format!("https://[fd12:3456:789a::1]/org/repo/commit/{sha}"),
            format!("https://[fe80::1]/org/repo/commit/{sha}"),
            format!("https://[::ffff:10.0.0.1]/org/repo/commit/{sha}"),
            format!("https://[::ffff:c0a8:101]/org/repo/commit/{sha}"),
            format!("https://10.0.0.1./org/repo/commit/{sha}"),
            format!("https://192.168.1.1./org/repo/commit/{sha}"),
            format!("https://169.254.0.1./org/repo/commit/{sha}"),
            format!("https://[::10.0.0.1]/org/repo/commit/{sha}"),
            format!("https://[::a00:1]/org/repo/commit/{sha}"),
            format!("https://[0:0:0:0:0:0:10.0.0.1]/org/repo/commit/{sha}"),
            format!("https://[::169.254.0.1]/org/repo/commit/{sha}"),
            format!("https://[::a9fe:1]/org/repo/commit/{sha}"),
            format!("https://[::ffff:0:10.0.0.1]/org/repo/commit/{sha}"),
            format!("https://[::ffff:0:a00:1]/org/repo/commit/{sha}"),
            format!("https://localhost./org/repo/commit/{sha}"),
        ] {
            let json = valid_evidence_json().replace(
                "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
                &uri,
            );
            let err = parse_evidence(json.as_bytes()).expect_err(&uri);
            assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
            assert!(
                err.to_string().contains("not a public https location"),
                "{uri}: {err}"
            );
        }
        let public_ipv4 = format!("https://1.1.1.1/org/repo/commit/{sha}");
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            &public_ipv4,
        );
        parse_evidence(json.as_bytes()).expect("public unicast IPv4 is a public https host");
        let public_fqdn_dot = format!("https://1.1.1.1./org/repo/commit/{sha}");
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            &public_fqdn_dot,
        );
        parse_evidence(json.as_bytes())
            .expect("trailing FQDN dot on a public IPv4 is still a public host");
        let pinned = format!("https://github.com/gyldlab/keld/blob/{sha}/README.md");
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            &pinned,
        );
        parse_evidence(json.as_bytes()).expect("commit-pinned blob is still a pin");
    }

    #[test]
    fn rejects_nat64_6to4_and_cgnat_https_hosts() {
        let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
        for uri in [
            format!("https://[64:ff9b::10.0.0.1]/org/repo/commit/{sha}"),
            format!("https://[2002:a00:1::1]/org/repo/commit/{sha}"),
            format!("https://100.64.0.1/org/repo/commit/{sha}"),
            format!("https://100.64.0.1./org/repo/commit/{sha}"),
            format!("https://[::ffff:100.64.0.1]/org/repo/commit/{sha}"),
            format!("https://[64:ff9b::1.1.1.1]/org/repo/commit/{sha}"),
            format!("https://[2002:808:808::1]/org/repo/commit/{sha}"),
        ] {
            let json = valid_evidence_json().replace(
                "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
                &uri,
            );
            let err = parse_evidence(json.as_bytes()).expect_err(&uri);
            assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
            assert!(
                err.to_string().contains("not a public https location"),
                "{uri}: {err}"
            );
        }
    }

    #[test]
    fn accepts_sha256_evidence_uri() {
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            DIGEST_A,
        );
        let record = parse_evidence(json.as_bytes()).expect("sha256 uri");
        assert_eq!(record.evidence_uri, DIGEST_A);
    }

    #[test]
    fn rejects_blob_main_even_when_a_hex_path_segment_is_present() {
        let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
        let hexfile = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        for uri in [
            format!("https://github.com/gyldlab/keld/blob/main/{hexfile}"),
            format!("https://github.com/gyldlab/keld/tree/main/{hexfile}"),
            format!("https://github.com/gyldlab/keld/BLOB/MAIN/{hexfile}"),
            format!("https://raw.githubusercontent.com/gyldlab/keld/main/{hexfile}"),
        ] {
            let json = valid_evidence_json().replace(
                "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
                &uri,
            );
            let err = parse_evidence(json.as_bytes()).expect_err(&uri);
            assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
        }
        let pinned = format!("https://github.com/gyldlab/keld/blob/{sha}/README.md");
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            &pinned,
        );
        parse_evidence(json.as_bytes()).expect("commit-pinned blob is a pin");
    }

    #[test]
    fn score_rejects_constructed_loopback_and_blob_main_uris() {
        let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        denom.cells.truncate(1);
        let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
        let hexfile = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        for uri in [
            format!("https://git@127.0.0.1/org/repo/commit/{sha}"),
            format!("https://[::ffff:127.0.0.1]/org/repo/commit/{sha}"),
            format!("https://0.0.0.0/org/repo/commit/{sha}"),
            "https://github.com/gyldlab/keld/blob/main/README.md".to_owned(),
            format!("https://github.com/gyldlab/keld/blob/main/{hexfile}"),
            format!("https://10.0.0.1/org/repo/commit/{sha}"),
            format!("https://192.168.1.1/org/repo/commit/{sha}"),
            format!("https://172.16.0.1/org/repo/commit/{sha}"),
            format!("https://169.254.0.1/org/repo/commit/{sha}"),
            format!("https://[fc00::1]/org/repo/commit/{sha}"),
            format!("https://10.0.0.1./org/repo/commit/{sha}"),
            format!("https://[::10.0.0.1]/org/repo/commit/{sha}"),
            format!("https://[::ffff:0:10.0.0.1]/org/repo/commit/{sha}"),
            format!("https://localhost./org/repo/commit/{sha}"),
            format!("https://[64:ff9b::10.0.0.1]/org/repo/commit/{sha}"),
            format!("https://[2002:a00:1::1]/org/repo/commit/{sha}"),
            format!("https://100.64.0.1/org/repo/commit/{sha}"),
            format!("https://github.com:abc/gyldlab/keld/commit/{sha}"),
            format!("https://github.com:65536/gyldlab/keld/commit/{sha}"),
            format!("https://raw.githubusercontent.com./gyldlab/keld/main/{hexfile}"),
        ] {
            let mut record = parse_ok();
            record.evidence_uri = uri.clone();
            let err = score(&denom, &[record], AS_OF).expect_err(&uri);
            assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
        }
    }

    #[test]
    fn error_display_table_has_code_and_fix() {
        let cases: [(EvidenceError, &str, &str); 9] = [
            (
                EvidenceError::TooLarge { len: 9, max: 8 },
                "KELD-COMPAT-001",
                "Split the ledger",
            ),
            (EvidenceError::NotUtf8, "KELD-COMPAT-002", "Re-encode"),
            (
                EvidenceError::InvalidJson {
                    detail: "x".to_owned(),
                },
                "KELD-COMPAT-003",
                "no trailing bytes",
            ),
            (
                EvidenceError::UnsupportedSchema {
                    found: "v0".to_owned(),
                },
                "KELD-COMPAT-004",
                EVIDENCE_SCHEMA,
            ),
            (
                EvidenceError::InvalidRecord {
                    detail: "x".to_owned(),
                },
                "KELD-COMPAT-005",
                "closed field set",
            ),
            (
                EvidenceError::InvalidWaiver {
                    detail: "x".to_owned(),
                },
                "KELD-COMPAT-006",
                "owner, reason",
            ),
            (
                EvidenceError::NonNormativeEvidence {
                    detail: "x".to_owned(),
                },
                "KELD-COMPAT-007",
                "non-normative leads",
            ),
            (
                EvidenceError::InvalidDenominator {
                    detail: "x".to_owned(),
                },
                "KELD-COMPAT-008",
                "unique cells",
            ),
            (
                EvidenceError::DuplicateCell {
                    cell: "a/b".to_owned(),
                },
                "KELD-COMPAT-009",
                "one record",
            ),
        ];
        for (err, code, fix) in cases {
            let text = err.to_string();
            assert_eq!(err.code(), code);
            assert!(text.contains(code), "{text}");
            assert!(text.contains(fix), "{text}");
        }
    }

    #[test]
    fn foreign_kind_record_cannot_fill_committed_cell() {
        let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
        denom.cells.truncate(1);
        let json =
            valid_evidence_json().replace(r#""kind": "primary_workflow""#, r#""kind": "install""#);
        let foreign = parse_evidence(json.as_bytes()).expect("install record");
        assert_eq!(foreign.operation.kind, OperationKind::Install);
        let board = score(&denom, &[foreign], AS_OF).expect("score");
        assert_eq!(board.missing, 1);
        assert_eq!(board.passed, 0);
        assert_eq!(board.unweighted_percent, None);
        assert!(!board.complete);
    }

    #[test]
    fn rejects_malformed_https_ports() {
        let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
        for uri in [
            format!("https://github.com:abc/gyldlab/keld/commit/{sha}"),
            format!("https://github.com:65536/gyldlab/keld/commit/{sha}"),
            format!("https://github.com:/gyldlab/keld/commit/{sha}"),
            format!("https://[2001:db8::1]:65536/org/repo/commit/{sha}"),
        ] {
            let json = valid_evidence_json().replace(
                "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
                &uri,
            );
            let err = parse_evidence(json.as_bytes()).expect_err(&uri);
            assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
        }
        let ok = format!("https://github.com:443/gyldlab/keld/commit/{sha}");
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            &ok,
        );
        parse_evidence(json.as_bytes()).expect("decimal u16 port is still a public https location");
    }

    #[test]
    fn rejects_trailing_dot_github_raw_cdn_live_ref() {
        let hexfile = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
        let live = format!("https://raw.githubusercontent.com./gyldlab/keld/main/{hexfile}");
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            &live,
        );
        let err = parse_evidence(json.as_bytes()).expect_err(&live);
        assert_eq!(err.code(), "KELD-COMPAT-007");
        let pinned = format!("https://raw.githubusercontent.com./gyldlab/keld/{sha}/README.md");
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            &pinned,
        );
        parse_evidence(json.as_bytes())
            .expect("trailing FQDN dot on a pinned GitHub raw CDN ref is still a pin");
    }
}
