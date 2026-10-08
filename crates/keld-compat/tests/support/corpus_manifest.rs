//! One owner for committed compatibility corpus manifests (gh532 rule 8, X01-T4).
//!
//! Spec: `docs/specs/gh566-corpus-manifest-owner.md` (task T2). Every corpus test in
//! keld-compat includes this module with `#[path]`, and this module holds no test
//! function. It owns the exact-bytes digest, manifest parsing (the frozen v0 shape for
//! `electron-lifecycle-v0`), the admitted-pin table, denominator agreement, cell rules,
//! the code registry of admitted test targets, harness record runs, execution
//! admission and the owner and fixture censuses. Every check returns a typed
//! [`CorpusError`], so a negative control asserts the exact rejection.
// Each including target uses a different subset of this module (keld-ipc precedent).
#![allow(dead_code)]
// Cold test-time checks: typed rejections carry their evidence fields so a negative
// control asserts the exact variant; boxing them would buy nothing on this path.
#![allow(clippy::result_large_err)]

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use keld_compat::evidence::{
    Arch, AuthorityProfile, CellKey, CivilDate, Denominator, EvidenceError, EvidenceRecord,
    OperationKind, Panel, Platform, Scoreboard, Verdict, parse_denominator, parse_evidence, score,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The one exact-bytes digest helper: `sha256:` plus lowercase hex of `bytes` (D2).
pub fn sha256_uri(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// An Electron upstream pin: one tag version and the commit it peels to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin {
    /// Electron version, for example `44.3.0`.
    pub version: &'static str,
    /// Full 40-hex commit the version's tag peels to.
    pub commit: &'static str,
}

impl Pin {
    /// Prefix every cell `oracle_id` carries (gh532 rule 1).
    pub fn oracle_prefix(self) -> String {
        format!("electron-v{}.", self.version)
    }

    /// The `operation.oracle.revision` every record carries (gh532 rule 1).
    pub fn oracle_revision(self) -> String {
        format!("electron-v{}@{}", self.version, self.commit)
    }

    /// Prefix of an immutable Electron source URL at this pin.
    pub fn doc_blob_prefix(self) -> String {
        format!("https://github.com/electron/electron/blob/{}/", self.commit)
    }
}

/// Which manifests an admitted pin may serve (D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinScope {
    /// The frozen v0 shape, for one corpus id only.
    FrozenV0 { corpus_id: &'static str },
}

/// One row of the admitted-pin table.
#[derive(Clone, Copy, Debug)]
pub struct AdmittedPin {
    /// The pin.
    pub pin: Pin,
    /// The manifests it may serve.
    pub scope: PinScope,
}

/// The only corpus id the frozen v0 shape is admitted for (gh532 AC11).
pub const V0_FROZEN_CORPUS_ID: &str = "electron-lifecycle-v0";

/// The one admitted-pin table (D4). A new pin needs a reviewed spec amendment.
pub const ADMITTED_PINS: &[AdmittedPin] = &[AdmittedPin {
    pin: Pin {
        version: "44.3.0",
        commit: "07e460719c75b2ec5ee4893f7d2192ef31c7b8c2",
    },
    scope: PinScope::FrozenV0 {
        corpus_id: V0_FROZEN_CORPUS_ID,
    },
}];

/// Digest of the frozen `electron-lifecycle-v0` manifest bytes (gh532 AC11).
pub const V0_CORPUS_SHA256: &str =
    "sha256:badc0aaf3619168927cf464e2dd0006a599b5614a35b84960c59984b18e0e8b2";

/// Facts the frozen v0 shape does not carry, supplied by code (D3). Removal condition:
/// KEL-237 re-records the lifecycle corpus as v1 with fresh receipts.
#[derive(Debug)]
pub struct FrozenV0 {
    /// Engine identity token of every v0 record (gh532 rule 7).
    pub engine_token: &'static str,
    /// Label every frozen harness record carries (D7). Fixed, unlike v1.
    pub harness_profile: AuthorityProfile,
    /// Date the frozen records are scored as of; they carry no waiver.
    pub records_as_of: CivilDate,
    /// Every file under the fixture directory with its digest at origin/main 6945cdc9.
    pub files: &'static [(&'static str, &'static str)],
}

/// The frozen v0 facade.
pub const V0_FROZEN: FrozenV0 = FrozenV0 {
    engine_token: "headless-lifecycle-conformance",
    harness_profile: AuthorityProfile::LegacySandboxOff,
    records_as_of: CivilDate {
        year: 2026,
        month: 9,
        day: 16,
    },
    files: &[
        ("corpus.json", V0_CORPUS_SHA256),
        (
            "denominator.json",
            "sha256:09149660ce122d95ca6c6322a9f60049d2973ac52cc86d99986263ef09c313db",
        ),
        (
            "evidence/linux-x86_64--app-quit-return-contract.json",
            "sha256:c37df38254cbe5373273b18bb955cc79dd90750383f4fa8380a898c73adaaf94",
        ),
        (
            "evidence/linux-x86_64--app-when-ready-host-ready-gate.json",
            "sha256:a4329e6635985f3b35209440ca7421de72f9a2b49214b68f11d09117c9318573",
        ),
        (
            "evidence/linux-x86_64--app-window-all-closed-policy.json",
            "sha256:5bc041e16887721ef550be032ea4e53b2b0588cc3a4c1986d389967c4739282f",
        ),
        (
            "evidence/macos-aarch64--app-quit-return-contract.json",
            "sha256:cf56ba4f608697854e1c952be8fb3dea58045ee992fe9347337d40e2b8654216",
        ),
        (
            "evidence/macos-aarch64--app-when-ready-host-ready-gate.json",
            "sha256:133dac427b92cb7eb5d53075685bd4ee38f0796a4b664207355c5a05c91248ef",
        ),
        (
            "evidence/macos-aarch64--app-window-all-closed-policy.json",
            "sha256:8a0b721a352b5f8602663753e0d8e58467ad1f7fed6057ffd6d003a11568c13f",
        ),
        (
            "evidence/windows-x86_64--app-quit-return-contract.json",
            "sha256:99b0f9dae86dd1a8c2f092fbf25e0c414335137bc730f342416f7093523a5112",
        ),
        (
            "evidence/windows-x86_64--app-when-ready-host-ready-gate.json",
            "sha256:ad329885fb3d55c34c7106c80eacb5a4ac396b1ce54ca6689b961272950cfb95",
        ),
        (
            "evidence/windows-x86_64--app-window-all-closed-policy.json",
            "sha256:9bddc565741e16b52274e9a1db71d3c86cf3ecb456e7f0d956a4125f9c95f415",
        ),
        (
            "receipts/linux-x86_64.json",
            "sha256:5357cba3cbfe1458b078d815956f15959491a09e95aaa7b03643810c508edc3d",
        ),
        (
            "receipts/macos-aarch64.json",
            "sha256:741490530fde052e312e938d6969d31e151da1f74b86e25067ca2a7632200355",
        ),
        (
            "receipts/windows-x86_64.json",
            "sha256:eab36c9eb94a6d0c9974190b8ca70843473d0e5c175c0b68b4079609a4154ec3",
        ),
        (
            "report.md",
            "sha256:72bf997d571fac33f0881f9b75883ec5e6b9cff572049e3d8689568130d01483",
        ),
    ],
};

/// Manifest shape a registration admits. The registration (code) decides it (D3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// The frozen KEL-237 shape, admitted for [`V0_FROZEN_CORPUS_ID`] only.
    FrozenV0,
}

/// How a registered target runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runner {
    /// A keld-compat integration target run by Cargo and libtest.
    Libtest { target: &'static str },
    /// A Bun test file under `packages/`.
    Bun,
}

/// One admitted test target of a corpus (gh532 rule 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TestTarget {
    /// Workspace-relative path that cells name in `test_path`.
    pub path: &'static str,
    /// The runner that executes it.
    pub runner: Runner,
}

/// One registered corpus. Code, not the manifest, admits its shape and targets.
#[derive(Clone, Copy, Debug)]
pub struct Registration {
    /// Corpus id the manifest must carry.
    pub corpus_id: &'static str,
    /// Fixture directory relative to `crates/keld-compat`.
    pub fixture_dir: &'static str,
    /// Admitted manifest shape.
    pub shape: Shape,
    /// Admitted test targets.
    pub targets: &'static [TestTarget],
}

/// The frozen KEL-237 lifecycle corpus.
pub const LIFECYCLE_V0: Registration = Registration {
    corpus_id: V0_FROZEN_CORPUS_ID,
    fixture_dir: "fixtures/lifecycle-corpus",
    shape: Shape::FrozenV0,
    targets: &[
        TestTarget {
            path: "crates/keld-compat/tests/electron_lifecycle.rs",
            runner: Runner::Libtest {
                target: "electron_lifecycle",
            },
        },
        TestTarget {
            path: "packages/@keld/electron/src/app.test.ts",
            runner: Runner::Bun,
        },
    ],
};

/// Every committed corpus. A consumer appends one entry (gh566 §4.4).
pub const REGISTRY: &[Registration] = &[LIFECYCLE_V0];

/// Fixed file names inside a fixture directory.
const MANIFEST_FILE: &str = "corpus.json";
const DENOMINATOR_FILE: &str = "denominator.json";
const EVIDENCE_DIR: &str = "evidence";
/// Repository path of this module, relative to `crates/keld-compat`.
pub const OWNER_PATH: &str = "tests/support/corpus_manifest.rs";
/// Text a target must not contain to be an oracle: including it would recurse (C3).
const OWNER_INCLUDE: &str = "support/corpus_manifest.rs";

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
        }
    }
}

impl std::error::Error for CorpusError {}

/// The frozen v0 manifest shape, moved unchanged from `lifecycle_corpus.rs`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestV0 {
    corpus_id: String,
    scope: String,
    panel: String,
    kind: String,
    upstream: UpstreamV0,
    cells: Vec<CellV0>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpstreamV0 {
    electron_version: String,
    electron_commit: String,
    app_docs: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CellV0 {
    operation_id: String,
    oracle_id: String,
    expected_verdict: String,
    test_path: String,
    test_name: String,
    negative_control: String,
    #[serde(default)]
    intentional_divergence: Option<String>,
}

/// One validated corpus cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The KEL-74 cell identity.
    pub key: CellKey,
    /// The verdict every run lane must record (or `unknown` when unrun).
    pub expected: Verdict,
    /// The permanent divergence a `fail` cell records, when it has one.
    pub divergence: Option<String>,
    /// Registered target path the cell maps.
    pub test_path: String,
    /// Exact case name in that target.
    pub test_name: String,
    /// The one mutation that must fail the mapped test.
    pub negative_control: String,
}

/// A manifest that passed every static rule. Later steps consume it and never re-parse.
#[derive(Debug, Clone)]
pub struct Corpus {
    id: String,
    scope: String,
    panel: Panel,
    kind: OperationKind,
    pin: Pin,
    digest: String,
    engine_token: &'static str,
    harness_profile: AuthorityProfile,
    records_as_of: CivilDate,
    cells: Vec<Cell>,
    registration: Registration,
    denominator: Denominator,
}

/// A validated harness run: one `(platform, arch)` scored once by KEL-74.
#[derive(Debug, Clone)]
pub struct Run {
    board: Scoreboard,
}

impl Run {
    /// The KEL-74 scoreboard of this run.
    pub fn board(&self) -> &Scoreboard {
        &self.board
    }

    /// Takes the scoreboard.
    pub fn into_board(self) -> Scoreboard {
        self.board
    }
}

/// Absolute path of `crates/keld-compat`.
pub fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Locate the source workspace independently of the test runner's working directory.
pub fn workspace_root() -> PathBuf {
    crate_root()
        .parent()
        .and_then(Path::parent)
        .map_or_else(crate_root, Path::to_path_buf)
}

/// Joins a `/`-separated relative path one component at a time (Windows-safe).
fn join_rel(base: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

/// Maps an I/O failure on `path` to the typed rejection.
fn io_error(path: &Path) -> impl Fn(std::io::Error) -> CorpusError + '_ {
    move |error| CorpusError::Io {
        path: path.display().to_string(),
        error: error.to_string(),
    }
}

fn read(path: &Path) -> Result<Vec<u8>, CorpusError> {
    fs::read(path).map_err(io_error(path))
}

/// Maps the KEL-74 platform enum to its record token (the one test-side copy).
pub const fn platform_token(platform: Platform) -> &'static str {
    match platform {
        Platform::Macos => "macos",
        Platform::Windows => "windows",
        Platform::Linux => "linux",
    }
}

/// Maps the KEL-74 architecture enum to its record token.
pub const fn arch_token(arch: Arch) -> &'static str {
    match arch {
        Arch::Aarch64 => "aarch64",
        Arch::X86_64 => "x86_64",
    }
}

/// Maps a KEL-74 verdict to its record token.
pub const fn verdict_token(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Pass => "pass",
        Verdict::Fail => "fail",
        Verdict::Unknown => "unknown",
        Verdict::Waived => "waived",
    }
}

/// Maps a KEL-74 panel to its manifest token. Owned here because KEL-74's map is
/// private; removal tracked on #566 with the other four maps (D3).
pub const fn panel_token(panel: Panel) -> &'static str {
    match panel {
        Panel::Product => "product",
        Panel::Showcase => "showcase",
    }
}

/// Maps a KEL-74 operation kind to its manifest token (D3).
pub const fn kind_token(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Install => "install",
        OperationKind::Activation => "activation",
        OperationKind::PrimaryWorkflow => "primary_workflow",
        OperationKind::FullFeature => "full_feature",
    }
}

/// The platform this test binary was built for (receipt selection, C1).
pub const fn host_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::Macos
    } else if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    }
}

/// Asserts the denominator digest binds the exact manifest bytes, then runs the live
/// one-byte mutation control: the first `e` becomes `E` and the digest must change.
pub fn check_manifest_digest(
    corpus_id: &str,
    manifest: &[u8],
    declared: &str,
) -> Result<(), CorpusError> {
    let computed = sha256_uri(manifest);
    if computed != declared {
        return Err(CorpusError::DigestMismatch {
            corpus_id: corpus_id.to_owned(),
            declared: declared.to_owned(),
            computed,
        });
    }
    let mut mutated = manifest.to_vec();
    let index = mutated
        .iter()
        .position(|byte| *byte == b'e')
        .ok_or_else(|| CorpusError::DigestMutationUndetected {
            corpus_id: corpus_id.to_owned(),
        })?;
    mutated[index] = b'E';
    if sha256_uri(&mutated) == declared {
        return Err(CorpusError::DigestMutationUndetected {
            corpus_id: corpus_id.to_owned(),
        });
    }
    Ok(())
}

impl Corpus {
    /// Reads `corpus.json` and `denominator.json` from the registration's fixture dir.
    pub fn fixture_bytes(reg: &Registration) -> Result<(Vec<u8>, Vec<u8>), CorpusError> {
        let dir = join_rel(&crate_root(), reg.fixture_dir);
        Ok((
            read(&dir.join(MANIFEST_FILE))?,
            read(&dir.join(DENOMINATOR_FILE))?,
        ))
    }

    /// Loads and validates a committed corpus.
    pub fn load(reg: &Registration) -> Result<Self, CorpusError> {
        let (manifest, denominator) = Self::fixture_bytes(reg)?;
        Self::parse(reg, &manifest, &denominator)
    }

    /// Validates manifest and denominator bytes in the D2 order: digest, parse and
    /// shape, pin, denominator agreement, cell rules, then registry and targets.
    pub fn parse(
        reg: &Registration,
        manifest: &[u8],
        denominator: &[u8],
    ) -> Result<Self, CorpusError> {
        let denominator = parse_denominator(denominator).map_err(CorpusError::Evidence)?;
        check_manifest_digest(reg.corpus_id, manifest, denominator.corpus_sha256())?;
        let digest = sha256_uri(manifest);

        let Shape::FrozenV0 = reg.shape;
        if reg.corpus_id != V0_FROZEN_CORPUS_ID {
            return Err(CorpusError::V0ShapeNotAdmitted {
                corpus_id: reg.corpus_id.to_owned(),
            });
        }
        let parsed =
            serde_json::from_slice::<ManifestV0>(manifest).map_err(|error| CorpusError::Parse {
                corpus_id: reg.corpus_id.to_owned(),
                message: error.to_string(),
            })?;
        if parsed.corpus_id != reg.corpus_id {
            return Err(CorpusError::CorpusIdMismatch {
                registered: reg.corpus_id.to_owned(),
                manifest: parsed.corpus_id,
            });
        }
        let id = parsed.corpus_id.clone();

        let pin = admitted_pin(reg, &parsed.upstream)?;
        check_v0_pin_use(&id, pin, &parsed)?;
        check_denominator(&id, &parsed, &denominator)?;
        let cells = parsed
            .cells
            .iter()
            .map(|cell| v0_cell(&id, cell))
            .collect::<Result<Vec<_>, _>>()?;
        check_targets(reg)?;
        for cell in &cells {
            if !reg
                .targets
                .iter()
                .any(|target| target.path == cell.test_path)
            {
                return Err(CorpusError::UnregisteredTarget {
                    corpus_id: id.clone(),
                    cell: cell.key.operation_id.clone(),
                    test_path: cell.test_path.clone(),
                });
            }
        }

        Ok(Self {
            id,
            scope: parsed.scope,
            panel: denominator.panel(),
            kind: denominator.kind(),
            pin,
            digest,
            engine_token: V0_FROZEN.engine_token,
            harness_profile: V0_FROZEN.harness_profile,
            records_as_of: V0_FROZEN.records_as_of,
            cells,
            registration: *reg,
            denominator,
        })
    }

    /// Corpus id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Declared scope sentence.
    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// Scoreboard panel.
    pub fn panel(&self) -> Panel {
        self.panel
    }

    /// Operation kind of every cell.
    pub fn kind(&self) -> OperationKind {
        self.kind
    }

    /// The admitted pin, read from the parsed manifest (D4).
    pub fn pin(&self) -> Pin {
        self.pin
    }

    /// Digest of the exact manifest bytes.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Validated cells in manifest order.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// The registration this corpus was loaded through.
    pub fn registration(&self) -> &Registration {
        &self.registration
    }

    /// The KEL-74 denominator, agreeing with the manifest (C8).
    pub fn denominator(&self) -> &Denominator {
        &self.denominator
    }

    /// Date this corpus's committed records are scored as of.
    pub fn records_as_of(&self) -> CivilDate {
        self.records_as_of
    }

    /// Validates one harness run and scores it once with KEL-74 (D7). `score()`'s
    /// `DuplicateCell` is the uniqueness check (C5).
    pub fn validate_harness_run(
        &self,
        records: &[EvidenceRecord],
        as_of: CivilDate,
    ) -> Result<Run, CorpusError> {
        if self.panel != Panel::Showcase {
            return Err(CorpusError::NotHarnessPanel {
                corpus_id: self.id.clone(),
            });
        }
        let first = records.first().ok_or_else(|| CorpusError::EmptyRun {
            corpus_id: self.id.clone(),
        })?;
        let (platform, arch) = (first.artifact().platform, first.artifact().arch);
        if records
            .iter()
            .any(|record| record.artifact().platform != platform || record.artifact().arch != arch)
        {
            return Err(CorpusError::MixedRun {
                corpus_id: self.id.clone(),
            });
        }
        for record in records {
            self.check_record(record)?;
        }
        let board = score(&self.denominator, records, as_of).map_err(CorpusError::Evidence)?;
        Ok(Run { board })
    }

    fn check_record(&self, record: &EvidenceRecord) -> Result<(), CorpusError> {
        let operation = record.operation();
        let platform = platform_token(record.artifact().platform);
        let cell_label = format!("{}/{}", operation.id, operation.oracle.id);
        let mismatch =
            |field: &'static str, found: String, expected: String| CorpusError::RecordMismatch {
                corpus_id: self.id.clone(),
                cell: cell_label.clone(),
                platform,
                field,
                found,
                expected,
            };
        let cell = self
            .cells
            .iter()
            .find(|cell| {
                cell.key.operation_id == operation.id && cell.key.oracle_id == operation.oracle.id
            })
            .ok_or_else(|| {
                mismatch(
                    "cell",
                    cell_label.clone(),
                    "a manifest cell (operation_id, oracle_id)".to_owned(),
                )
            })?;
        if operation.kind != self.kind {
            return Err(mismatch(
                "operation.kind",
                kind_token(operation.kind).to_owned(),
                kind_token(self.kind).to_owned(),
            ));
        }
        if record.artifact().sha256 != self.digest {
            return Err(mismatch(
                "artifact.sha256",
                record.artifact().sha256.clone(),
                self.digest.clone(),
            ));
        }
        let revision = self.pin.oracle_revision();
        if operation.oracle.revision != revision {
            return Err(CorpusError::PinMismatch {
                corpus_id: self.id.clone(),
                cell: format!("record {cell_label} on {platform}"),
                found: operation.oracle.revision.clone(),
                expected: revision,
            });
        }
        let result = record.result();
        if result != cell.expected && result != Verdict::Unknown {
            return Err(mismatch(
                "result",
                verdict_token(result).to_owned(),
                format!("{} or unknown", verdict_token(cell.expected)),
            ));
        }
        if record.waiver().is_some() {
            return Err(mismatch(
                "waiver",
                "a waiver".to_owned(),
                "no waiver".to_owned(),
            ));
        }
        let engine = &record.revisions().engine;
        let engine_ok = engine.split_once('@').is_some_and(|(token, rev)| {
            token == self.engine_token
                && !rev.is_empty()
                && !rev.contains('@')
                && !rev.chars().any(char::is_whitespace)
        });
        if !engine_ok {
            return Err(mismatch(
                "revisions.engine",
                engine.clone(),
                format!("{}@<pinned revision>", self.engine_token),
            ));
        }
        if record.authority_profile() != self.harness_profile {
            return Err(CorpusError::LabelMismatch {
                corpus_id: self.id.clone(),
                cell: cell_label,
                receipt_state: "conformance-harness",
                label: record.authority_profile().as_str(),
            });
        }
        Ok(())
    }
}

fn admitted_pin(reg: &Registration, upstream: &UpstreamV0) -> Result<Pin, CorpusError> {
    ADMITTED_PINS
        .iter()
        .find(|row| {
            row.pin.version == upstream.electron_version
                && row.pin.commit == upstream.electron_commit
                && match row.scope {
                    PinScope::FrozenV0 { corpus_id } => {
                        reg.shape == Shape::FrozenV0 && reg.corpus_id == corpus_id
                    }
                }
        })
        .map(|row| row.pin)
        .ok_or_else(|| CorpusError::UnadmittedPin {
            corpus_id: reg.corpus_id.to_owned(),
            version: upstream.electron_version.clone(),
            commit: upstream.electron_commit.clone(),
        })
}

fn check_v0_pin_use(id: &str, pin: Pin, parsed: &ManifestV0) -> Result<(), CorpusError> {
    let docs = pin.doc_blob_prefix();
    if !parsed.upstream.app_docs.starts_with(&docs) {
        return Err(CorpusError::PinMismatch {
            corpus_id: id.to_owned(),
            cell: "upstream.app_docs".to_owned(),
            found: parsed.upstream.app_docs.clone(),
            expected: docs,
        });
    }
    let prefix = pin.oracle_prefix();
    for cell in &parsed.cells {
        let suffix_ok = cell
            .oracle_id
            .strip_prefix(&prefix)
            .is_some_and(|rest| !rest.is_empty());
        if !suffix_ok {
            return Err(CorpusError::PinMismatch {
                corpus_id: id.to_owned(),
                cell: cell.operation_id.clone(),
                found: cell.oracle_id.clone(),
                expected: format!("{prefix}<oracle>"),
            });
        }
    }
    Ok(())
}

fn check_denominator(
    id: &str,
    parsed: &ManifestV0,
    denominator: &Denominator,
) -> Result<(), CorpusError> {
    let mismatch = |detail: String| CorpusError::DenominatorMismatch {
        corpus_id: id.to_owned(),
        detail,
    };
    if denominator.corpus_id() != id {
        return Err(mismatch(format!(
            "corpus_id {} differs from the manifest's {id}",
            denominator.corpus_id()
        )));
    }
    if panel_token(denominator.panel()) != parsed.panel {
        return Err(mismatch(format!(
            "panel {} differs from the manifest's {}",
            panel_token(denominator.panel()),
            parsed.panel
        )));
    }
    if kind_token(denominator.kind()) != parsed.kind {
        return Err(mismatch(format!(
            "kind {} differs from the manifest's {}",
            kind_token(denominator.kind()),
            parsed.kind
        )));
    }
    let mut manifest_cells = Vec::with_capacity(parsed.cells.len());
    for cell in &parsed.cells {
        let key = CellKey {
            operation_id: cell.operation_id.clone(),
            oracle_id: cell.oracle_id.clone(),
        };
        if manifest_cells.contains(&key) {
            return Err(mismatch(format!(
                "the manifest repeats cell {}/{}",
                key.operation_id, key.oracle_id
            )));
        }
        manifest_cells.push(key);
    }
    let mut expected = manifest_cells;
    expected.sort();
    let mut found = denominator.cells().to_vec();
    found.sort();
    if found != expected {
        return Err(mismatch(format!(
            "cells {} differ from the manifest's {}",
            cell_list(&found),
            cell_list(&expected)
        )));
    }
    Ok(())
}

fn cell_list(cells: &[CellKey]) -> String {
    cells
        .iter()
        .map(|cell| format!("{}/{}", cell.operation_id, cell.oracle_id))
        .collect::<Vec<_>>()
        .join(", ")
}

fn v0_cell(id: &str, cell: &CellV0) -> Result<Cell, CorpusError> {
    let rule = |detail: &str| CorpusError::VerdictRule {
        corpus_id: id.to_owned(),
        cell: cell.operation_id.clone(),
        detail: detail.to_owned(),
    };
    if cell.negative_control.trim().is_empty() {
        return Err(rule("must name a falsifier in negative_control"));
    }
    if cell.test_name.trim().is_empty() || cell.test_name.contains(['\r', '\n']) {
        return Err(rule("must name one non-empty test case on one line"));
    }
    let divergence = cell
        .intentional_divergence
        .as_deref()
        .filter(|reason| !reason.trim().is_empty());
    let expected = match (cell.expected_verdict.as_str(), divergence) {
        ("pass", None) if cell.intentional_divergence.is_none() => Verdict::Pass,
        ("pass", _) => return Err(rule("a passing cell must not hide a divergence")),
        ("fail", Some(_)) => Verdict::Fail,
        ("fail", None) => return Err(rule("a failing cell must name the intentional divergence")),
        (other, _) => {
            return Err(rule(&format!(
                "uses unsupported bounded-corpus verdict {other}"
            )));
        }
    };
    Ok(Cell {
        key: CellKey {
            operation_id: cell.operation_id.clone(),
            oracle_id: cell.oracle_id.clone(),
        },
        expected,
        divergence: divergence.map(str::to_owned),
        test_path: cell.test_path.clone(),
        test_name: cell.test_name.clone(),
        negative_control: cell.negative_control.clone(),
    })
}

/// Validates every registered target of `reg` (C3): a libtest target is a keld-compat
/// integration file that exists and does not include this owner; a Bun target is an
/// existing `*.test.ts` under `packages/`.
pub fn check_targets(reg: &Registration) -> Result<(), CorpusError> {
    let invalid = |path: &str, reason: String| CorpusError::InvalidTarget {
        corpus_id: reg.corpus_id.to_owned(),
        path: path.to_owned(),
        reason,
    };
    for target in reg.targets {
        let file = join_rel(&workspace_root(), target.path);
        match target.runner {
            Runner::Libtest { target: name } => {
                let expected = format!("crates/keld-compat/tests/{name}.rs");
                if target.path != expected {
                    return Err(invalid(
                        target.path,
                        format!(
                            "a libtest target must be {expected}, a keld-compat integration file"
                        ),
                    ));
                }
                let text = fs::read_to_string(&file)
                    .map_err(|error| invalid(target.path, format!("cannot read it: {error}")))?;
                if text.contains(OWNER_INCLUDE) {
                    return Err(invalid(
                        target.path,
                        "it includes the corpus owner, so admitting it would recurse".to_owned(),
                    ));
                }
            }
            Runner::Bun => {
                if !target.path.starts_with("packages/") || !target.path.ends_with(".test.ts") {
                    return Err(invalid(
                        target.path,
                        "a Bun target must be a packages/**/*.test.ts file".to_owned(),
                    ));
                }
                if !file.is_file() {
                    return Err(invalid(target.path, "the file does not exist".to_owned()));
                }
            }
        }
    }
    Ok(())
}

/// Every file under a fixture directory as (`/`-separated relative path, bytes).
pub fn fixture_files(reg: &Registration) -> Result<Vec<(String, Vec<u8>)>, CorpusError> {
    let root = join_rel(&crate_root(), reg.fixture_dir);
    let mut files = Vec::new();
    collect_files(&root, "", &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files(
    dir: &Path,
    prefix: &str,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), CorpusError> {
    let entries = fs::read_dir(dir).map_err(io_error(dir))?;
    for entry in entries {
        let entry = entry.map_err(io_error(dir))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if path.is_dir() {
            collect_files(&path, &rel, out)?;
        } else {
            out.push((rel, read(&path)?));
        }
    }
    Ok(())
}

/// The frozen v0 file census (gh532 AC11): exactly the table's files, each at its
/// origin/main digest.
pub fn check_frozen_files(files: &[(String, Vec<u8>)]) -> Result<(), CorpusError> {
    let found: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
    let mut expected: Vec<&str> = V0_FROZEN.files.iter().map(|(path, _)| *path).collect();
    expected.sort_unstable();
    let mut sorted = found.clone();
    sorted.sort_unstable();
    if sorted != expected {
        return Err(CorpusError::FixtureCensus {
            detail: format!("frozen files {sorted:?} differ from the table {expected:?}"),
        });
    }
    for (path, bytes) in files {
        let digest = sha256_uri(bytes);
        let pinned = V0_FROZEN
            .files
            .iter()
            .find(|(name, _)| name == path)
            .map(|(_, digest)| *digest);
        if pinned != Some(digest.as_str()) {
            return Err(CorpusError::FixtureCensus {
                detail: format!("{path} has {digest}, not its frozen origin/main digest"),
            });
        }
    }
    Ok(())
}

/// Committed KEL-74 records under `evidence/`, as (relative path, bytes).
pub fn committed_record_files(reg: &Registration) -> Result<Vec<(String, Vec<u8>)>, CorpusError> {
    let prefix = format!("{EVIDENCE_DIR}/");
    Ok(fixture_files(reg)?
        .into_iter()
        .filter(|(path, _)| {
            path.starts_with(&prefix)
                && Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension == "json")
        })
        .collect())
}

/// Parses committed records and groups them into runs by `(platform, arch)`.
pub fn committed_runs(reg: &Registration) -> Result<Vec<Vec<EvidenceRecord>>, CorpusError> {
    let mut runs: Vec<Vec<EvidenceRecord>> = Vec::new();
    for (_, bytes) in committed_record_files(reg)? {
        let record = parse_evidence(&bytes).map_err(CorpusError::Evidence)?;
        let identity = (record.artifact().platform, record.artifact().arch);
        match runs.iter_mut().find(|run| {
            run.first()
                .is_some_and(|first| (first.artifact().platform, first.artifact().arch) == identity)
        }) {
            Some(run) => run.push(record),
            None => runs.push(vec![record]),
        }
    }
    Ok(runs)
}

/// Directories under `crates/keld-compat/fixtures/` that hold a `corpus.json`.
pub fn committed_corpus_dirs() -> Result<Vec<String>, CorpusError> {
    let fixtures = crate_root().join("fixtures");
    let entries = fs::read_dir(&fixtures).map_err(io_error(&fixtures))?;
    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io_error(&fixtures))?;
        if entry.path().join(MANIFEST_FILE).is_file() {
            dirs.push(format!("fixtures/{}", entry.file_name().to_string_lossy()));
        }
    }
    dirs.sort();
    Ok(dirs)
}

/// Fixture census (C2): the committed corpus directories equal the registry's, and
/// corpus ids and directories are each unique.
pub fn fixture_census(found: &[String], registry: &[Registration]) -> Result<(), CorpusError> {
    let fail = |detail: String| Err(CorpusError::FixtureCensus { detail });
    for (index, reg) in registry.iter().enumerate() {
        if registry[..index]
            .iter()
            .any(|other| other.corpus_id == reg.corpus_id)
        {
            return fail(format!("corpus id {} is registered twice", reg.corpus_id));
        }
        if registry[..index]
            .iter()
            .any(|other| other.fixture_dir == reg.fixture_dir)
        {
            return fail(format!(
                "fixture dir {} is registered twice",
                reg.fixture_dir
            ));
        }
        if !found.iter().any(|dir| dir == reg.fixture_dir) {
            return fail(format!(
                "{} is registered at {}, which holds no committed corpus",
                reg.corpus_id, reg.fixture_dir
            ));
        }
    }
    for dir in found {
        if !registry.iter().any(|reg| reg.fixture_dir == dir) {
            return fail(format!(
                "{dir} holds a corpus that no registration validates"
            ));
        }
    }
    Ok(())
}

/// keld-compat test sources as (`/`-separated path relative to the crate, text).
pub type Sources = Vec<(String, String)>;

/// Reads every `tests/**/*.rs` file of keld-compat.
pub fn load_test_sources() -> Result<Sources, CorpusError> {
    let root = crate_root();
    let mut files = Vec::new();
    collect_files(&root.join("tests"), "tests", &mut files)?;
    let mut sources: Sources = files
        .into_iter()
        .filter(|(path, _)| {
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "rs")
        })
        .map(|(path, bytes)| (path, String::from_utf8_lossy(&bytes).into_owned()))
        .collect();
    sources.sort();
    Ok(sources)
}

/// `sha2` dependency kinds of keld-compat from `cargo metadata` (census rule 5).
pub fn sha2_dependency_kinds() -> Result<Vec<Option<String>>, CorpusError> {
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(workspace_root())
        .output()
        .map_err(|error| CorpusError::RunnerFailed {
            target: "cargo metadata".to_owned(),
            detail: error.to_string(),
        })?;
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|error| CorpusError::RunnerFailed {
            target: "cargo metadata".to_owned(),
            detail: error.to_string(),
        })?;
    let kinds = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|package| package["name"] == "keld-compat")
        .flat_map(|package| package["dependencies"].as_array().into_iter().flatten())
        .filter(|dependency| dependency["name"] == "sha2")
        .map(|dependency| dependency["kind"].as_str().map(str::to_owned))
        .collect();
    Ok(kinds)
}

/// Field names that mark a manifest parser (census rule 2).
const MANIFEST_FIELDS: &[&str] = &[
    "corpus_id",
    "cells",
    "upstream",
    "oracle_id",
    "expected_verdict",
    "test_path",
];

/// The owner census (gh532 AC10, gh566 D10 rules 1–5). Patterns are assembled with
/// `concat!` so this file's own text matches only its real definitions.
pub fn owner_census(
    sources: &Sources,
    lib_rs: &str,
    sha2_kinds: &[Option<String>],
) -> Result<(), CorpusError> {
    let violation = |rule: u8, file: &str, line: usize, detail: String| {
        Err(CorpusError::CensusViolation {
            rule,
            file: file.to_owned(),
            line,
            detail,
        })
    };
    let forbidden = [
        concat!("fn sha", "256_uri"),
        concat!("Sha", "256"),
        concat!("corpus", ".json\""),
        concat!("denominator", ".json\""),
    ];
    let test_attribute = concat!("#[", "test]");
    let mut owner_seen = false;
    for (path, text) in sources {
        if path.starts_with("tests/support/") && text.contains(test_attribute) {
            return violation(
                3,
                path,
                0,
                "a support module holds a test, which would run in every including target"
                    .to_owned(),
            );
        }
        if path == OWNER_PATH {
            owner_seen = true;
            census_owner(path, text)?;
            continue;
        }
        for (index, line) in text.lines().enumerate() {
            if let Some(token) = forbidden.iter().find(|token| line.contains(**token)) {
                return violation(1, path, index + 1, format!("`{token}` outside the owner"));
            }
        }
        if let Some((line, field)) = deserialize_manifest_field(text) {
            return violation(
                2,
                path,
                line,
                format!("a Deserialize struct declares manifest field `{field}`"),
            );
        }
    }
    if !owner_seen {
        return violation(3, OWNER_PATH, 0, "the owner is missing".to_owned());
    }
    let public: Vec<&str> = lib_rs
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pub mod ") || line.starts_with("pub use "))
        .collect();
    if public != ["pub mod evidence;"] {
        return violation(
            4,
            "src/lib.rs",
            0,
            format!("public items {public:?}; only `pub mod evidence;` is allowed"),
        );
    }
    if sha2_kinds != [Some("dev".to_owned())] {
        return violation(
            5,
            "Cargo.toml",
            0,
            format!("sha2 dependency kinds {sha2_kinds:?}; it must be a dev-dependency only"),
        );
    }
    Ok(())
}

/// Census rule 3: inside the owner every definition appears exactly once.
fn census_owner(path: &str, text: &str) -> Result<(), CorpusError> {
    let checks = [
        (concat!("pub fn sha", "256_uri("), 1, true),
        (concat!("Sha256", "::digest("), 1, false),
        (
            concat!("serde_json::from_slice::<", "ManifestV0>"),
            1,
            false,
        ),
    ];
    for (pattern, expected, line_start) in checks {
        let count = text
            .lines()
            .filter(|line| {
                if line_start {
                    line.trim_start().starts_with(pattern)
                } else {
                    line.contains(pattern)
                }
            })
            .count();
        if count != expected {
            return Err(CorpusError::CensusViolation {
                rule: 3,
                file: path.to_owned(),
                line: 0,
                detail: format!("`{pattern}` appears {count} times; expected {expected}"),
            });
        }
    }
    Ok(())
}

/// Finds a field named in [`MANIFEST_FIELDS`] inside a struct that derives `Deserialize`.
/// A derive may span lines (rustfmt splits long ones) and may share a line with the
/// `struct` it decorates; any `pub` or `pub(...)` visibility is ignored.
fn deserialize_manifest_field(text: &str) -> Option<(usize, &'static str)> {
    let mut in_derive = false;
    let mut armed = false;
    let mut depth: i64 = 0;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if depth == 0 {
            let mut rest = trimmed;
            if in_derive || rest.starts_with("#[derive(") {
                armed |= rest.contains("Deserialize");
                if let Some((_, after)) = rest.split_once(")]") {
                    in_derive = false;
                    rest = after.trim();
                } else {
                    in_derive = true;
                    continue;
                }
            }
            if armed && rest.contains("struct ") && rest.ends_with('{') {
                depth = 1;
                armed = false;
            } else if !rest.is_empty() && !rest.starts_with("#[") && !rest.starts_with("//") {
                armed = false;
            }
            continue;
        }
        let field = strip_visibility(trimmed);
        if depth == 1
            && let Some(name) = MANIFEST_FIELDS.iter().find(|name| {
                field
                    .strip_prefix(**name)
                    .is_some_and(|rest| rest.trim_start().starts_with(':'))
            })
        {
            return Some((index + 1, name));
        }
        for character in trimmed.chars() {
            match character {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
    }
    None
}

/// Drops a leading `pub` or `pub(...)` visibility from a field line.
fn strip_visibility(line: &str) -> &str {
    if let Some(rest) = line.strip_prefix("pub(") {
        return rest
            .split_once(')')
            .map_or(line, |(_, after)| after.trim_start());
    }
    line.strip_prefix("pub ").unwrap_or(line)
}
