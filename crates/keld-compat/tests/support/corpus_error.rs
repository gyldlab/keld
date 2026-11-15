//! The typed rejection vocabulary of the corpus owner (gh566 D1). A child module of
//! `corpus_manifest.rs`, split out under the gh566 D1 review condition so the owner stays
//! under 1,500 lines. Each `Display` names the corpus, the cell, the rule and the fix.

use std::fmt;

use keld_compat::evidence::EvidenceError;

use super::{OWNER_PATH, SNAPSHOT_DIR, SNAPSHOT_ROOT, V0_FROZEN_CORPUS_ID};

/// Typed rejection. Each `Display` names the corpus, the cell, the rule and the fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorpusError {
    /// A file could not be read or listed.
    Io { path: String, error: String },
    /// KEL-74 rejected a denominator, record or score (for example `DuplicateCell`).
    Evidence(EvidenceError),
    /// The manifest JSON did not parse (unknown or repeated field, wrong type).
    Parse { corpus_id: String, message: String },
    /// The denominator's `corpus_sha256` is not the digest of the manifest bytes.
    DigestMismatch {
        corpus_id: String,
        declared: String,
        computed: String,
    },
    /// The one-byte mutation control did not change the digest.
    DigestMutationUndetected { corpus_id: String },
    /// The frozen v0 shape was registered for another corpus id.
    V0ShapeNotAdmitted { corpus_id: String },
    /// The manifest's `corpus_id` differs from its registration.
    CorpusIdMismatch {
        registered: String,
        manifest: String,
    },
    /// No admitted-pin row matches the upstream pair for this registration.
    UnadmittedPin {
        corpus_id: String,
        version: String,
        commit: String,
    },
    /// An `oracle_id`, `app_docs` URL or record revision is not at the corpus pin.
    PinMismatch {
        corpus_id: String,
        cell: String,
        found: String,
        expected: String,
    },
    /// The denominator does not agree with the manifest (C8).
    DenominatorMismatch { corpus_id: String, detail: String },
    /// A cell breaks the verdict and key rules (gh532 rule 3; KEL-237 v0 rules).
    VerdictRule {
        corpus_id: String,
        cell: String,
        detail: String,
    },
    /// A registered target has the wrong path form, is missing or would recurse (C3).
    InvalidTarget {
        corpus_id: String,
        path: String,
        reason: String,
    },
    /// A cell names a target that is not registered for its corpus (C3).
    UnregisteredTarget {
        corpus_id: String,
        cell: String,
        test_path: String,
    },
    /// A harness run was requested for a non-showcase corpus.
    NotHarnessPanel { corpus_id: String },
    /// A run has no records.
    EmptyRun { corpus_id: String },
    /// A run mixes records of two `(platform, arch)` pairs.
    MixedRun { corpus_id: String },
    /// A record disagrees with the corpus or its cell (C5, gh532 AC7, AC9).
    RecordMismatch {
        corpus_id: String,
        cell: String,
        platform: &'static str,
        field: &'static str,
        found: String,
        expected: String,
    },
    /// A record's authority label does not match its run (gh532 AC8, AC12).
    LabelMismatch {
        corpus_id: String,
        cell: String,
        receipt_state: &'static str,
        label: &'static str,
    },
    /// A runner could not start or exited unsuccessfully.
    RunnerFailed { target: String, detail: String },
    /// A mapped cell lacks exactly one passing case in its target's output.
    CaseNotAdmitted {
        corpus_id: String,
        cell: String,
        target: String,
        test_name: String,
    },
    /// The owner census found a second owner or a missing invariant (gh532 AC10).
    CensusViolation {
        rule: u8,
        file: String,
        line: usize,
        detail: String,
    },
    /// Committed fixtures and the registry disagree, or frozen bytes drifted.
    FixtureCensus { detail: String },
    /// Corpus directories that hold their own `doc-snapshots/` copy (gh566 D5 A5).
    CorpusLocalSnapshot { dirs: Vec<String> },
    /// A v1 manifest names another schema (gh566 D3).
    UnknownSchema { corpus_id: String, schema: String },
    /// `artifact_digest` is not the one v1 meaning, `manifest_bytes` (gh532 rule 5).
    UnsupportedArtifactDigest { corpus_id: String, value: String },
    /// An `engine` or `doc_snapshots` object repeats a key (gh566 C4).
    DuplicateKey {
        corpus_id: String,
        field: &'static str,
        key: String,
    },
    /// The `engine` map is empty or holds an invalid platform key or token (gh532 rule 7).
    InvalidEngine { corpus_id: String, detail: String },
    /// A v1 cell declares no platform (gh566 C10).
    EmptyPlatforms { corpus_id: String, cell: String },
    /// A v1 cell repeats, misspells or widens its platforms (gh566 C10).
    InvalidPlatforms {
        corpus_id: String,
        cell: String,
        detail: String,
    },
    /// A citation URL is not an Electron docs page at the corpus pin (gh532 AC3).
    CitationUrl {
        corpus_id: String,
        cell: String,
        url: String,
        reason: String,
    },
    /// `quote_sha256` is not the digest of `quote` (gh532 AC3).
    QuoteDigestMismatch {
        corpus_id: String,
        cell: String,
        declared: String,
        computed: String,
    },
    /// A cited page has no `doc_snapshots` entry (gh532 AC16).
    MissingSnapshotEntry {
        corpus_id: String,
        cell: String,
        page: String,
    },
    /// A cited page has no snapshot file under the pinned commit (gh532 AC16).
    MissingSnapshotFile {
        corpus_id: String,
        page: String,
        path: String,
    },
    /// A snapshot's bytes differ from its `doc_snapshots` digest (gh532 AC16).
    SnapshotDigestMismatch {
        corpus_id: String,
        page: String,
        declared: String,
        computed: String,
    },
    /// A cell's `quote` is not a byte-exact substring of its pinned page (gh532 AC16).
    QuoteAbsent {
        corpus_id: String,
        cell: String,
        page: String,
    },
    /// A `doc_snapshots` entry no cell cites (gh532 rule 2).
    UncitedSnapshotEntry { corpus_id: String, page: String },
    /// Git would normalise a snapshot on commit, so CI would read other bytes (gh566 D5).
    SnapshotWouldBeNormalised { page: String, path: String },
    /// A snapshot path's attributes transform bytes at checkout (gh566 D5).
    SnapshotCheckoutFilter {
        path: String,
        attribute: String,
        value: String,
    },
    /// A product run was requested for a non-product corpus.
    NotProductPanel { corpus_id: String },
    /// A product receipt cell has no record, so the run would be dropped (gh532 AC8).
    MissingProductRecord {
        corpus_id: String,
        cell: String,
        receipt_state: &'static str,
    },
    /// A product record names a cell its receipt did not run (gh566 D7).
    UncoveredProductRecord { corpus_id: String, cell: String },
    /// Committed product records need X02-T5's receipt reader (gh566 C6).
    ProductRecordsNeedReceipt { corpus_id: String },
    /// A report counts fails or writes a fail label itself (gh566 C9).
    ReportBypassesFailSplit {
        file: String,
        line: usize,
        detail: String,
    },
}

impl fmt::Display for CorpusError {
    // One arm per typed rejection, each with its fix guidance; splitting would scatter it.
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } => write!(
                f,
                "cannot read `{path}`: {error}. Restore the file or fix the registered path."
            ),
            Self::Evidence(error) => write!(f, "KEL-74 rejected corpus input: {error}"),
            Self::Parse { corpus_id, message } => write!(
                f,
                "{corpus_id}: manifest does not parse ({message}). Remove unknown or repeated fields; the shape is closed."
            ),
            Self::DigestMismatch {
                corpus_id,
                declared,
                computed,
            } => write!(
                f,
                "{corpus_id}: denominator corpus_sha256 {declared} is not the digest {computed} of the exact manifest bytes (gh532 rule 5). Regenerate the denominator and every record from the committed bytes."
            ),
            Self::DigestMutationUndetected { corpus_id } => write!(
                f,
                "{corpus_id}: a one-byte manifest mutation kept the committed digest. The digest helper is not hashing the bytes."
            ),
            Self::V0ShapeNotAdmitted { corpus_id } => write!(
                f,
                "{corpus_id}: the frozen v0 shape is admitted only for {V0_FROZEN_CORPUS_ID} (gh532 AC11). Write a v1 manifest."
            ),
            Self::CorpusIdMismatch {
                registered,
                manifest,
            } => write!(
                f,
                "registration {registered} points at a manifest whose corpus_id is {manifest}. Make them equal."
            ),
            Self::UnadmittedPin {
                corpus_id,
                version,
                commit,
            } => write!(
                f,
                "{corpus_id}: upstream {version} @ {commit} is not an admitted pin for this registration (gh532 rule 1). A new pin needs a reviewed spec amendment."
            ),
            Self::PinMismatch {
                corpus_id,
                cell,
                found,
                expected,
            } => write!(
                f,
                "{corpus_id}: {cell} has {found}, not the corpus pin {expected} (gh532 rule 1). Re-pin the whole corpus in one change."
            ),
            Self::DenominatorMismatch { corpus_id, detail } => write!(
                f,
                "{corpus_id}: denominator does not agree with the manifest: {detail} (gh566 C8). Regenerate the denominator from the manifest."
            ),
            Self::VerdictRule {
                corpus_id,
                cell,
                detail,
            } => write!(f, "{corpus_id}: cell {cell}: {detail}."),
            Self::InvalidTarget {
                corpus_id,
                path,
                reason,
            } => write!(
                f,
                "{corpus_id}: registered target {path} is not admissible: {reason} (gh566 C3)."
            ),
            Self::UnregisteredTarget {
                corpus_id,
                cell,
                test_path,
            } => write!(
                f,
                "{corpus_id}: cell {cell} names {test_path}, which is not registered for this corpus. Register the target in REGISTRY (gh532 rule 8)."
            ),
            Self::NotHarnessPanel { corpus_id } => write!(
                f,
                "{corpus_id}: harness runs exist only for showcase corpora. Validate product runs with a receipt."
            ),
            Self::EmptyRun { corpus_id } => {
                write!(f, "{corpus_id}: a run needs at least one record.")
            }
            Self::MixedRun { corpus_id } => write!(
                f,
                "{corpus_id}: a run mixes platforms or architectures. Validate one (platform, arch) at a time."
            ),
            Self::RecordMismatch {
                corpus_id,
                cell,
                platform,
                field,
                found,
                expected,
            } => write!(
                f,
                "{corpus_id}: {platform} record for {cell} has {field} {found}, expected {expected}. Regenerate the record from the run that produced it."
            ),
            Self::LabelMismatch {
                corpus_id,
                cell,
                receipt_state,
                label,
            } => write!(
                f,
                "{corpus_id}: record for {cell} is labelled {label}, which a {receipt_state} run cannot carry (gh532 rule 6)."
            ),
            Self::RunnerFailed { target, detail } => {
                write!(f, "runner for {target} failed: {detail}")
            }
            Self::CaseNotAdmitted {
                corpus_id,
                cell,
                target,
                test_name,
            } => write!(
                f,
                "{corpus_id}: cell {cell} needs exactly one executed, passing case `{test_name}` in {target}. A removed, skipped, ignored or source-only test is not evidence."
            ),
            Self::CensusViolation {
                rule,
                file,
                line,
                detail,
            } => write!(
                f,
                "owner census rule {rule}: {file}:{line}: {detail} (gh532 AC10). Use the one owner in {OWNER_PATH}."
            ),
            Self::FixtureCensus { detail } => write!(
                f,
                "fixture census: {detail}. Register every committed corpus once in REGISTRY (gh566 C2)."
            ),
            Self::CorpusLocalSnapshot { dirs } => write!(
                f,
                "corpus directories with their own {SNAPSHOT_DIR}/: {} (gh566 D5 A5). Cited pages live once in crates/keld-compat/{SNAPSHOT_ROOT}/{SNAPSHOT_DIR}/<commit>/<page>: move each page there, or delete it when the store already holds the same bytes.",
                dirs.join(", ")
            ),
            Self::UnknownSchema { corpus_id, schema } => write!(
                f,
                "{corpus_id}: schema {schema} is not keld.compat.corpus/v1 (gh566 D3)."
            ),
            Self::UnsupportedArtifactDigest { corpus_id, value } => write!(
                f,
                "{corpus_id}: artifact_digest {value} is not manifest_bytes, the one v1 meaning (gh532 rule 5)."
            ),
            Self::DuplicateKey {
                corpus_id,
                field,
                key,
            } => write!(
                f,
                "{corpus_id}: {field} repeats key {key}; JSON would keep only one value (gh566 C4). Remove the duplicate."
            ),
            Self::InvalidEngine { corpus_id, detail } => write!(
                f,
                "{corpus_id}: engine map {detail} (gh532 rule 7). Map each platform token to one identity token."
            ),
            Self::EmptyPlatforms { corpus_id, cell } => write!(
                f,
                "{corpus_id}: cell {cell} declares no platform (gh566 C10). List the lanes its mapped test runs on."
            ),
            Self::InvalidPlatforms {
                corpus_id,
                cell,
                detail,
            } => write!(
                f,
                "{corpus_id}: cell {cell} platforms: {detail} (gh566 C10)."
            ),
            Self::CitationUrl {
                corpus_id,
                cell,
                url,
                reason,
            } => write!(
                f,
                "{corpus_id}: cell {cell} cites {url}: {reason} (gh532 AC3). Cite a docs/ page at the corpus pin."
            ),
            Self::QuoteDigestMismatch {
                corpus_id,
                cell,
                declared,
                computed,
            } => write!(
                f,
                "{corpus_id}: cell {cell} quote_sha256 {declared} is not {computed}, the digest of its quote (gh532 AC3)."
            ),
            Self::MissingSnapshotEntry {
                corpus_id,
                cell,
                page,
            } => write!(
                f,
                "{corpus_id}: cell {cell} cites {page}, which has no doc_snapshots entry (gh532 AC16)."
            ),
            Self::MissingSnapshotFile {
                corpus_id,
                page,
                path,
            } => write!(
                f,
                "{corpus_id}: {page} has no snapshot at crates/keld-compat/{SNAPSHOT_ROOT}/{path} (gh532 AC16, gh566 D5 A5). Commit the raw upstream page there once, under the pinned commit; every corpus reads that one store."
            ),
            Self::SnapshotDigestMismatch {
                corpus_id,
                page,
                declared,
                computed,
            } => write!(
                f,
                "{corpus_id}: snapshot of {page} has {computed}, not its doc_snapshots digest {declared} (gh532 AC16)."
            ),
            Self::QuoteAbsent {
                corpus_id,
                cell,
                page,
            } => write!(
                f,
                "{corpus_id}: cell {cell} quotes text absent from {page} at the pin (gh532 AC16). Copy the quote byte-for-byte from the snapshot."
            ),
            Self::UncitedSnapshotEntry { corpus_id, page } => write!(
                f,
                "{corpus_id}: doc_snapshots lists {page}, which no cell cites (gh532 rule 2). Remove the entry."
            ),
            Self::SnapshotWouldBeNormalised { page, path } => write!(
                f,
                "snapshot {path} of {page} would be normalised by git on commit, so CI would read other bytes (gh566 D5). Add a path-scoped `-text` attribute."
            ),
            Self::SnapshotCheckoutFilter {
                path,
                attribute,
                value,
            } => write!(
                f,
                "snapshot {path} has checkout attribute {attribute}={value}, so another clone would check out other bytes (gh566 D5). Give doc-snapshots/ `-text` and no filter."
            ),
            Self::NotProductPanel { corpus_id } => write!(
                f,
                "{corpus_id}: product runs exist only for product corpora. Validate harness runs instead."
            ),
            Self::MissingProductRecord {
                corpus_id,
                cell,
                receipt_state,
            } => write!(
                f,
                "{corpus_id}: the {receipt_state} receipt ran {cell} but no record exists; the run must not be dropped (gh532 AC8)."
            ),
            Self::UncoveredProductRecord { corpus_id, cell } => write!(
                f,
                "{corpus_id}: record for {cell} names a cell its receipt did not run (gh566 D7)."
            ),
            Self::ProductRecordsNeedReceipt { corpus_id } => write!(
                f,
                "{corpus_id}: committed product records need X02-T5's receipt reader before they can be validated (gh566 C6)."
            ),
            Self::ReportBypassesFailSplit { file, line, detail } => write!(
                f,
                "report census: {file}:{line}: {detail} (gh566 C9). Render fail counts through FailSplit."
            ),
        }
    }
}

impl std::error::Error for CorpusError {}
