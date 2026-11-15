//! One owner for committed compatibility corpus manifests (gh532 rule 8, X01-T4).
//!
//! Spec: `docs/specs/gh566-corpus-manifest-owner.md` (task T2). Every corpus test in
//! keld-compat includes this module with `#[path]`, and this module holds no test
//! function. It owns the exact-bytes digest, manifest parsing (the frozen v0 shape for
//! `electron-lifecycle-v0`), the admitted-pin table, denominator agreement, cell rules,
//! the code registry of admitted test targets, harness record runs and the frozen-file
//! table. Two sibling support modules, split out under the gh566 D1 review condition,
//! hold execution admission (`corpus_admission.rs`) and the censuses
//! (`corpus_census.rs`). Three child modules, which this module declares itself, hold
//! the `CorpusError` vocabulary (`corpus_error.rs`), citations and snapshots
//! (`corpus_citation.rs`), and record runs with `FailSplit` (`corpus_runs.rs`). Every
//! check returns a typed [`CorpusError`], so a negative control asserts the exact
//! rejection.
// Each including target uses a different subset of this module (keld-ipc precedent).
#![allow(dead_code)]
// Cold test-time checks: typed rejections carry their evidence fields so a negative
// control asserts the exact variant; boxing them would buy nothing on this path.
#![allow(clippy::result_large_err)]

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use keld_compat::evidence::{
    Arch, AuthorityProfile, CellKey, CivilDate, Denominator, EvidenceRecord, OperationKind, Panel,
    Platform, Verdict, parse_denominator, parse_evidence,
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

    /// Directory of this pin's doc snapshots, relative to [`SNAPSHOT_ROOT`] (gh532
    /// rule 2; one store for every corpus, gh566 D5 A5).
    pub fn snapshot_dir(self) -> String {
        format!("{SNAPSHOT_DIR}/{}/", self.commit)
    }
}

/// Which manifests an admitted pin may serve (D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinScope {
    /// The frozen v0 shape, for one corpus id only.
    FrozenV0 { corpus_id: &'static str },
    /// Every v1 manifest.
    V1,
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
pub const ADMITTED_PINS: &[AdmittedPin] = &[
    AdmittedPin {
        pin: Pin {
            version: "44.3.0",
            commit: "07e460719c75b2ec5ee4893f7d2192ef31c7b8c2",
        },
        scope: PinScope::FrozenV0 {
            corpus_id: V0_FROZEN_CORPUS_ID,
        },
    },
    AdmittedPin {
        pin: Pin {
            version: "44.4.5",
            commit: "694f45852a0f1726cd23bfd379854de489cccb65",
        },
        scope: PinScope::V1,
    },
];

/// Label every v1 conformance-harness record carries (gh532 rule 6, §10 Q2; gh566 D7).
/// The frozen v0 label is `V0_FROZEN.harness_profile` and does not follow this one.
pub const HARNESS_PROFILE: AuthorityProfile = AuthorityProfile::LegacySandboxOff;

/// Date committed v1 records are scored as of. v1 records carry no waiver (gh566 D6),
/// so the date changes no result (gh566 D7).
pub const V1_RECORDS_AS_OF: CivilDate = CivilDate {
    year: 2026,
    month: 10,
    day: 8,
};

/// The one v1 `schema` id (gh566 D3).
pub const V1_SCHEMA: &str = "keld.compat.corpus/v1";

/// Report label of red-until-implemented `fail` cells (gh532 AC17, gh566 C9).
pub const PENDING_LABEL: &str = "Pending implementation";
/// Report label of permanent-divergence `fail` cells (gh532 AC17, gh566 C9).
pub const DIVERGENCE_LABEL: &str = "Intentional divergence";

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
    /// The `keld.compat.corpus/v1` shape (gh532 §4.2).
    V1,
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
    /// Platforms its cells may declare; v0 cells take all of them (gh566 D13).
    pub platforms: &'static [Platform],
    /// Admitted test targets.
    pub targets: &'static [TestTarget],
}

/// The frozen KEL-237 lifecycle corpus.
pub const LIFECYCLE_V0: Registration = Registration {
    corpus_id: V0_FROZEN_CORPUS_ID,
    fixture_dir: "fixtures/lifecycle-corpus",
    shape: Shape::FrozenV0,
    platforms: &[Platform::Macos, Platform::Linux, Platform::Windows],
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

/// The pinned v44.4.5 draw.io window-path cells (gh448, F02-T1). A macOS first proof:
/// every cell declares `macos`, so other hosts list its cells `unknown` (gh566 D13).
pub const WINDOW_V1: Registration = Registration {
    corpus_id: "electron-window-v1",
    fixture_dir: "fixtures/window-corpus",
    shape: Shape::V1,
    platforms: &[Platform::Macos],
    targets: &[TestTarget {
        path: "packages/@keld/electron/src/browser-window.test.ts",
        runner: Runner::Bun,
    }],
};

/// Every committed corpus. A consumer appends one entry (gh566 §4.4).
pub const REGISTRY: &[Registration] = &[LIFECYCLE_V0, WINDOW_V1];

/// Parent of the one doc-snapshot store, relative to `crates/keld-compat`. Every v1
/// corpus reads its cited pages from `<SNAPSHOT_ROOT>/<SNAPSHOT_DIR>/<commit>/<page>`
/// ([`Pin::snapshot_dir`]), so a page cited by two corpora at one pin is committed once
/// (gh566 D5 A5).
pub const SNAPSHOT_ROOT: &str = "fixtures";
/// Name of the snapshot store directory under [`SNAPSHOT_ROOT`] (gh532 rule 2).
pub const SNAPSHOT_DIR: &str = "doc-snapshots";

/// Workspace-relative path of a snapshot keyed by `rel` (`doc-snapshots/<commit>/<page>`).
/// The one path builder: `Corpus::load` reads the bytes and runs both Git checks on it.
pub fn snapshot_repo_path(rel: &str) -> String {
    format!("crates/keld-compat/{SNAPSHOT_ROOT}/{rel}")
}

/// The owner's store reader: the bytes of the snapshot keyed by `rel`, read only from the
/// one store under `workspace` (gh566 D5 A5). A corpus-local copy is never consulted.
pub fn read_store_snapshot(workspace: &Path, rel: &str) -> std::io::Result<Vec<u8>> {
    fs::read(join_rel(workspace, &snapshot_repo_path(rel)))
}

/// Fixed file names inside a fixture directory.
pub const MANIFEST_FILE: &str = "corpus.json";
const DENOMINATOR_FILE: &str = "denominator.json";
const EVIDENCE_DIR: &str = "evidence";
/// Repository path of this module, relative to `crates/keld-compat`.
pub const OWNER_PATH: &str = "tests/support/corpus_manifest.rs";
/// Text a target must not contain to be an oracle: including it would recurse (C3).
const OWNER_INCLUDE: &str = "support/corpus_manifest.rs";

#[path = "corpus_error.rs"]
mod error;
pub use error::CorpusError;
#[path = "corpus_citation.rs"]
mod citation;
// The child modules' public items; each including target uses a different subset.
#[allow(unused_imports)]
pub use citation::{DocCitation, SnapshotReader, check_checkout_attributes, check_normalisation};
#[path = "corpus_runs.rs"]
mod runs;
#[allow(unused_imports)]
pub use runs::{FailSplit, ProductReceipt, ProfileState, Run, validate_committed_runs};

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

/// The `keld.compat.corpus/v1` shape (gh532 §4.2, gh566 D3).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestV1 {
    schema: String,
    corpus_id: String,
    scope: String,
    panel: String,
    kind: String,
    artifact_digest: String,
    engine: UniqueMap,
    doc_snapshots: UniqueMap,
    upstream: UpstreamV1,
    cells: Vec<CellV1>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpstreamV1 {
    electron_version: String,
    electron_commit: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CellV1 {
    operation_id: String,
    oracle_id: String,
    #[serde(default)]
    doc_citation: Option<DocCitation>,
    expected_verdict: String,
    #[serde(default)]
    intentional_divergence: Option<String>,
    #[serde(default)]
    implementing_ticket: Option<String>,
    platforms: Vec<String>,
    test_path: String,
    test_name: String,
    negative_control: String,
}

/// A JSON object's string entries in order, repeats included, so the owner rejects a
/// repeated key (C4) instead of letting serde keep the last value (F3).
#[derive(Debug, Default)]
struct UniqueMap(Vec<(String, String)>);

impl<'de> Deserialize<'de> for UniqueMap {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries;
        impl<'de> serde::de::Visitor<'de> for Entries {
            type Value = UniqueMap;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object of string values")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<UniqueMap, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry::<String, String>()? {
                    entries.push(entry);
                }
                Ok(UniqueMap(entries))
            }
        }
        deserializer.deserialize_map(Entries)
    }
}

impl UniqueMap {
    /// The entries, or `DuplicateKey` for the first repeated key (C4).
    fn into_unique(
        self,
        corpus_id: &str,
        field: &'static str,
    ) -> Result<Vec<(String, String)>, CorpusError> {
        for (index, (key, _)) in self.0.iter().enumerate() {
            if self.0[..index].iter().any(|(earlier, _)| earlier == key) {
                return Err(CorpusError::DuplicateKey {
                    corpus_id: corpus_id.to_owned(),
                    field,
                    key: key.clone(),
                });
            }
        }
        Ok(self.0)
    }
}

/// One manifest cell before the shared rules run; v0 cells leave the v1 fields empty.
struct RawCell {
    operation_id: String,
    oracle_id: String,
    expected_verdict: String,
    test_path: String,
    test_name: String,
    negative_control: String,
    intentional_divergence: Option<String>,
    implementing_ticket: Option<String>,
    doc_citation: Option<DocCitation>,
    platforms: Option<Vec<String>>,
}

/// Both shapes lowered to one view, so the shared rules are written once (D3).
struct Lowered {
    id: String,
    scope: String,
    panel: String,
    kind: String,
    version: String,
    commit: String,
    app_docs: Option<String>,
    engine: Option<Vec<(String, String)>>,
    doc_snapshots: Vec<(String, String)>,
    cells: Vec<RawCell>,
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
    /// The ticket whose change flips a red-until-implemented cell (gh532 rule 3).
    pub implementing_ticket: Option<String>,
    /// The pinned doc citation, when the cell has one (gh532 rule 2).
    pub citation: Option<DocCitation>,
    /// Lanes the mapped test runs on; elsewhere the cell is `unknown` (gh566 D13).
    pub platforms: Vec<Platform>,
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
    engine: Vec<(Platform, String)>,
    harness_profile: AuthorityProfile,
    records_as_of: CivilDate,
    cells: Vec<Cell>,
    registration: Registration,
    denominator: Denominator,
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
pub fn join_rel(base: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

/// Maps an I/O failure on `path` to the typed rejection.
pub fn io_error(path: &Path) -> impl Fn(std::io::Error) -> CorpusError + '_ {
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

    /// Loads and validates a committed corpus. Snapshots are read from the one store
    /// at [`SNAPSHOT_ROOT`], and each must be stored by Git byte-for-byte (D5).
    pub fn load(reg: &Registration) -> Result<Self, CorpusError> {
        let (manifest, denominator) = Self::fixture_bytes(reg)?;
        let workspace = workspace_root();
        let reader = |rel: &str| read_store_snapshot(&workspace, rel);
        let corpus = Self::parse_with_snapshots(reg, &manifest, &denominator, &reader)?;
        let mut pages: Vec<String> = Vec::new();
        for cell in &corpus.cells {
            if let Some(citation) = &cell.citation {
                let page = citation::page_path(&corpus.id, cell, corpus.pin, &citation.url)?;
                if !pages.contains(&page) {
                    pages.push(page);
                }
            }
        }
        for page in pages {
            let rel = format!("{}{page}", corpus.pin.snapshot_dir());
            let repo_rel = snapshot_repo_path(&rel);
            citation::check_normalisation(&repo_rel, &join_rel(&workspace, &repo_rel))?;
            citation::check_checkout_attributes(&workspace, &repo_rel)?;
        }
        Ok(corpus)
    }

    /// Validates bytes with no snapshot store: cited v1 cells fail closed.
    pub fn parse(
        reg: &Registration,
        manifest: &[u8],
        denominator: &[u8],
    ) -> Result<Self, CorpusError> {
        let none = |_: &str| Err(std::io::Error::from(std::io::ErrorKind::NotFound));
        Self::parse_with_snapshots(reg, manifest, denominator, &none)
    }

    /// Validates manifest and denominator bytes in the D2 order: digest, parse and
    /// shape, pin, denominator agreement, cell rules and platforms, citations and
    /// snapshots, then registry and targets. `read_snapshot` is keyed by the path
    /// relative to the snapshot store, [`SNAPSHOT_ROOT`] (D5).
    pub fn parse_with_snapshots(
        reg: &Registration,
        manifest: &[u8],
        denominator: &[u8],
        read_snapshot: SnapshotReader<'_>,
    ) -> Result<Self, CorpusError> {
        let denominator = parse_denominator(denominator).map_err(CorpusError::Evidence)?;
        check_manifest_digest(reg.corpus_id, manifest, denominator.corpus_sha256())?;
        let digest = sha256_uri(manifest);

        let lowered = match reg.shape {
            Shape::FrozenV0 => lower_v0(reg, manifest)?,
            Shape::V1 => lower_v1(reg, manifest)?,
        };
        if lowered.id != reg.corpus_id {
            return Err(CorpusError::CorpusIdMismatch {
                registered: reg.corpus_id.to_owned(),
                manifest: lowered.id,
            });
        }
        let id = lowered.id.clone();
        let engine = match &lowered.engine {
            Some(entries) => engine_map(&id, entries)?,
            None => reg
                .platforms
                .iter()
                .map(|platform| (*platform, V0_FROZEN.engine_token.to_owned()))
                .collect(),
        };

        let pin = admitted_pin(reg, &lowered.version, &lowered.commit)?;
        check_pin_use(&id, pin, &lowered)?;
        check_denominator(&id, &lowered, &denominator)?;
        let cells = lowered
            .cells
            .iter()
            .map(|raw| lower_cell(&id, reg, raw))
            .collect::<Result<Vec<_>, _>>()?;
        if reg.shape == Shape::V1 {
            citation::verify(&id, pin, &cells, &lowered.doc_snapshots, read_snapshot)?;
        }
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

        let (harness_profile, records_as_of) = match reg.shape {
            Shape::FrozenV0 => (V0_FROZEN.harness_profile, V0_FROZEN.records_as_of),
            Shape::V1 => (HARNESS_PROFILE, V1_RECORDS_AS_OF),
        };
        Ok(Self {
            id,
            scope: lowered.scope,
            panel: denominator.panel(),
            kind: denominator.kind(),
            pin,
            digest,
            engine,
            harness_profile,
            records_as_of,
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

    /// The engine identity token of `platform`, if the corpus declares one (rule 7).
    pub fn engine_token(&self, platform: Platform) -> Option<&str> {
        self.engine
            .iter()
            .find(|(declared, _)| *declared == platform)
            .map(|(_, token)| token.as_str())
    }
}

fn parse_error(corpus_id: &str, error: &serde_json::Error) -> CorpusError {
    CorpusError::Parse {
        corpus_id: corpus_id.to_owned(),
        message: error.to_string(),
    }
}

fn lower_v0(reg: &Registration, manifest: &[u8]) -> Result<Lowered, CorpusError> {
    if reg.corpus_id != V0_FROZEN_CORPUS_ID {
        return Err(CorpusError::V0ShapeNotAdmitted {
            corpus_id: reg.corpus_id.to_owned(),
        });
    }
    let parsed = serde_json::from_slice::<ManifestV0>(manifest)
        .map_err(|error| parse_error(reg.corpus_id, &error))?;
    Ok(Lowered {
        id: parsed.corpus_id,
        scope: parsed.scope,
        panel: parsed.panel,
        kind: parsed.kind,
        version: parsed.upstream.electron_version,
        commit: parsed.upstream.electron_commit,
        app_docs: Some(parsed.upstream.app_docs),
        engine: None,
        doc_snapshots: Vec::new(),
        cells: parsed
            .cells
            .into_iter()
            .map(|cell| RawCell {
                operation_id: cell.operation_id,
                oracle_id: cell.oracle_id,
                expected_verdict: cell.expected_verdict,
                test_path: cell.test_path,
                test_name: cell.test_name,
                negative_control: cell.negative_control,
                intentional_divergence: cell.intentional_divergence,
                implementing_ticket: None,
                doc_citation: None,
                platforms: None,
            })
            .collect(),
    })
}

fn lower_v1(reg: &Registration, manifest: &[u8]) -> Result<Lowered, CorpusError> {
    let parsed = serde_json::from_slice::<ManifestV1>(manifest)
        .map_err(|error| parse_error(reg.corpus_id, &error))?;
    if parsed.schema != V1_SCHEMA {
        return Err(CorpusError::UnknownSchema {
            corpus_id: reg.corpus_id.to_owned(),
            schema: parsed.schema,
        });
    }
    if parsed.artifact_digest != "manifest_bytes" {
        return Err(CorpusError::UnsupportedArtifactDigest {
            corpus_id: reg.corpus_id.to_owned(),
            value: parsed.artifact_digest,
        });
    }
    Ok(Lowered {
        engine: Some(parsed.engine.into_unique(reg.corpus_id, "engine")?),
        doc_snapshots: parsed
            .doc_snapshots
            .into_unique(reg.corpus_id, "doc_snapshots")?,
        id: parsed.corpus_id,
        scope: parsed.scope,
        panel: parsed.panel,
        kind: parsed.kind,
        version: parsed.upstream.electron_version,
        commit: parsed.upstream.electron_commit,
        app_docs: None,
        cells: parsed
            .cells
            .into_iter()
            .map(|cell| RawCell {
                operation_id: cell.operation_id,
                oracle_id: cell.oracle_id,
                expected_verdict: cell.expected_verdict,
                test_path: cell.test_path,
                test_name: cell.test_name,
                negative_control: cell.negative_control,
                intentional_divergence: cell.intentional_divergence,
                implementing_ticket: cell.implementing_ticket,
                doc_citation: cell.doc_citation,
                platforms: Some(cell.platforms),
            })
            .collect(),
    })
}

/// The platform whose record token is `token`, if any.
fn platform_of(token: &str) -> Option<Platform> {
    [Platform::Macos, Platform::Linux, Platform::Windows]
        .into_iter()
        .find(|platform| platform_token(*platform) == token)
}

/// Validates the v1 `engine` map (gh532 rule 7): platform-token keys, one identity
/// token each, with no `@` and no whitespace.
fn engine_map(
    id: &str,
    entries: &[(String, String)],
) -> Result<Vec<(Platform, String)>, CorpusError> {
    let invalid = |detail: String| CorpusError::InvalidEngine {
        corpus_id: id.to_owned(),
        detail,
    };
    if entries.is_empty() {
        return Err(invalid("is empty".to_owned()));
    }
    entries
        .iter()
        .map(|(key, token)| {
            let platform =
                platform_of(key).ok_or_else(|| invalid(format!("key {key} is not a platform")))?;
            if token.is_empty() || token.contains('@') || token.chars().any(char::is_whitespace) {
                return Err(invalid(format!(
                    "token {token:?} for {key} is not one identity"
                )));
            }
            Ok((platform, token.clone()))
        })
        .collect()
}

fn admitted_pin(reg: &Registration, version: &str, commit: &str) -> Result<Pin, CorpusError> {
    ADMITTED_PINS
        .iter()
        .find(|row| {
            row.pin.version == version
                && row.pin.commit == commit
                && match row.scope {
                    PinScope::FrozenV0 { corpus_id } => {
                        reg.shape == Shape::FrozenV0 && reg.corpus_id == corpus_id
                    }
                    PinScope::V1 => reg.shape == Shape::V1,
                }
        })
        .map(|row| row.pin)
        .ok_or_else(|| CorpusError::UnadmittedPin {
            corpus_id: reg.corpus_id.to_owned(),
            version: version.to_owned(),
            commit: commit.to_owned(),
        })
}

fn check_pin_use(id: &str, pin: Pin, lowered: &Lowered) -> Result<(), CorpusError> {
    let docs = pin.doc_blob_prefix();
    if let Some(app_docs) = &lowered.app_docs
        && !app_docs.starts_with(&docs)
    {
        return Err(CorpusError::PinMismatch {
            corpus_id: id.to_owned(),
            cell: "upstream.app_docs".to_owned(),
            found: app_docs.clone(),
            expected: docs,
        });
    }
    let prefix = pin.oracle_prefix();
    for cell in &lowered.cells {
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
    lowered: &Lowered,
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
    if panel_token(denominator.panel()) != lowered.panel {
        return Err(mismatch(format!(
            "panel {} differs from the manifest's {}",
            panel_token(denominator.panel()),
            lowered.panel
        )));
    }
    if kind_token(denominator.kind()) != lowered.kind {
        return Err(mismatch(format!(
            "kind {} differs from the manifest's {}",
            kind_token(denominator.kind()),
            lowered.kind
        )));
    }
    let mut manifest_cells = Vec::with_capacity(lowered.cells.len());
    for cell in &lowered.cells {
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

/// `GH-` or `KEL-` plus decimal digits without a leading zero (gh566 D6).
fn ticket_is_canonical(ticket: &str) -> bool {
    let digits = ticket
        .strip_prefix("GH-")
        .or_else(|| ticket.strip_prefix("KEL-"))
        .unwrap_or("");
    !digits.is_empty()
        && !digits.starts_with('0')
        && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Applies the cell rules of the registration's shape (gh532 rules 3–4, gh566 D6, D13).
fn lower_cell(id: &str, reg: &Registration, raw: &RawCell) -> Result<Cell, CorpusError> {
    let rule = |detail: &str| CorpusError::VerdictRule {
        corpus_id: id.to_owned(),
        cell: raw.operation_id.clone(),
        detail: detail.to_owned(),
    };
    if raw.negative_control.trim().is_empty() {
        return Err(rule("must name a falsifier in negative_control"));
    }
    if raw.test_name.trim().is_empty() || raw.test_name.contains(['\r', '\n']) {
        return Err(rule("must name one non-empty test case on one line"));
    }
    let divergence = raw
        .intentional_divergence
        .as_deref()
        .filter(|reason| !reason.trim().is_empty());
    let expected = match reg.shape {
        Shape::FrozenV0 => match (raw.expected_verdict.as_str(), divergence) {
            ("pass", None) if raw.intentional_divergence.is_none() => Verdict::Pass,
            ("pass", _) => return Err(rule("a passing cell must not hide a divergence")),
            ("fail", Some(_)) => Verdict::Fail,
            ("fail", None) => {
                return Err(rule("a failing cell must name the intentional divergence"));
            }
            (other, _) => {
                return Err(rule(&format!(
                    "uses unsupported bounded-corpus verdict {other}"
                )));
            }
        },
        Shape::V1 => v1_verdict(raw, divergence).map_err(|detail| rule(&detail))?,
    };
    let platforms = match &raw.platforms {
        None => reg.platforms.to_vec(),
        Some(tokens) => cell_platforms(id, reg, &raw.operation_id, tokens)?,
    };
    Ok(Cell {
        key: CellKey {
            operation_id: raw.operation_id.clone(),
            oracle_id: raw.oracle_id.clone(),
        },
        expected,
        divergence: divergence.map(str::to_owned),
        implementing_ticket: raw.implementing_ticket.clone(),
        citation: raw.doc_citation.clone(),
        platforms,
        test_path: raw.test_path.clone(),
        test_name: raw.test_name.clone(),
        negative_control: raw.negative_control.clone(),
    })
}

/// The v1 verdict and key rules (gh532 rules 3–4 and AC3–AC6, gh566 D6).
fn v1_verdict(raw: &RawCell, divergence: Option<&str>) -> Result<Verdict, String> {
    if raw.intentional_divergence.is_some() && divergence.is_none() {
        return Err("intentional_divergence must not be empty".to_owned());
    }
    if let Some(ticket) = &raw.implementing_ticket
        && !ticket_is_canonical(ticket)
    {
        return Err(format!(
            "implementing_ticket {ticket} must be GH- or KEL- plus digits without a leading zero"
        ));
    }
    let expected = [Verdict::Pass, Verdict::Fail, Verdict::Unknown]
        .into_iter()
        .find(|verdict| verdict_token(*verdict) == raw.expected_verdict)
        .ok_or_else(|| {
            format!(
                "expected_verdict {} is not pass, fail or unknown",
                raw.expected_verdict
            )
        })?;
    let cited = raw.doc_citation.is_some();
    let ticket = raw.implementing_ticket.is_some();
    match expected {
        Verdict::Pass if !cited => {
            Err("a pass cell needs a doc_citation (gh532 rule 4)".to_owned())
        }
        Verdict::Pass if divergence.is_some() || ticket => Err(
            "a pass cell carries neither intentional_divergence nor implementing_ticket".to_owned(),
        ),
        Verdict::Fail if !cited => {
            Err("a fail cell needs a doc_citation (gh532 rule 4)".to_owned())
        }
        Verdict::Fail if divergence.is_some() == ticket => Err(
            "a fail cell carries exactly one of intentional_divergence and implementing_ticket"
                .to_owned(),
        ),
        Verdict::Unknown if divergence.is_some() || ticket => Err(
            "an unknown cell carries neither intentional_divergence nor implementing_ticket"
                .to_owned(),
        ),
        _ => Ok(expected),
    }
}

/// A v1 cell's `platforms`: non-empty, distinct, known and registered (gh566 C10).
fn cell_platforms(
    id: &str,
    reg: &Registration,
    cell: &str,
    tokens: &[String],
) -> Result<Vec<Platform>, CorpusError> {
    if tokens.is_empty() {
        return Err(CorpusError::EmptyPlatforms {
            corpus_id: id.to_owned(),
            cell: cell.to_owned(),
        });
    }
    let invalid = |detail: String| CorpusError::InvalidPlatforms {
        corpus_id: id.to_owned(),
        cell: cell.to_owned(),
        detail,
    };
    let mut platforms = Vec::with_capacity(tokens.len());
    for token in tokens {
        let platform =
            platform_of(token).ok_or_else(|| invalid(format!("{token} is not a platform")))?;
        if platforms.contains(&platform) {
            return Err(invalid(format!("{token} is listed twice")));
        }
        if !reg.platforms.contains(&platform) {
            return Err(invalid(format!(
                "{token} is not registered for this corpus"
            )));
        }
        platforms.push(platform);
    }
    Ok(platforms)
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

/// Recursively appends each file's bytes and `/`-separated path to `out`. Returned
/// paths start with `prefix`; an empty prefix makes them relative to `dir`.
pub fn collect_files(
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
