//! Real retained Windows graph cases with independently pinned inert fixtures.

use super::windows_boot::bind_with_qualification;
use super::{
    AdmittedStageGraph, ApprovedGraphCaptureInput, GRAPH_FILE, SelectedRendererSource,
    WindowsDistributionQualification, make_graph_leaf_from_retained_root,
    qualified_windows_distribution, read_windows_bound_graph_from_retained_root,
    verify_provider_trees_from_retained_root, windows_graph_target,
};
use crate::app_session::{ORIGINAL_RUNTIME_ENTRY_PATH, windows_original_entry_bootstrap_source};
use sha2::{Digest as _, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

const PACKAGE: &str = concat!(
    "{\"name\":\"graph-test\",\"type\":\"module\",\"version\":\"0.0.1\",",
    "\"dependencies\":{\"electron\":\"npm:@keld/electron@0.0.1\",",
    "\"@keld/api\":\"0.0.1\",\"@keld/kipc\":\"0.0.1\"}}\n"
);
const LOCK: &str = concat!(
    "{\"name\":\"graph-test\",\"version\":\"0.0.1\",\"lockfileVersion\":3,\"packages\":{",
    "\"\":{\"name\":\"graph-test\",\"version\":\"0.0.1\",\"dependencies\":{",
    "\"electron\":\"npm:@keld/electron@0.0.1\",\"@keld/api\":\"0.0.1\",",
    "\"@keld/kipc\":\"0.0.1\"}},",
    "\"node_modules/electron\":{\"name\":\"@keld/electron\",\"version\":\"0.0.1\"},",
    "\"node_modules/@keld/api\":{\"name\":\"@keld/api\",\"version\":\"0.0.1\"},",
    "\"node_modules/@keld/kipc\":{\"name\":\"@keld/kipc\",\"version\":\"0.0.1\"}}}\n"
);
const ELECTRON_PACKAGE: &str = concat!(
    "{\"name\":\"@keld/electron\",\"version\":\"0.0.1\",\"type\":\"module\",",
    "\"exports\":{\".\":\"./dist/index.js\",\"./main\":\"./dist/index.js\",",
    "\"./renderer\":\"./dist/index.js\",\"./common\":\"./dist/index.js\",",
    "\"./utility\":\"./dist/index.js\"},",
    "\"dependencies\":{\"@keld/api\":\"0.0.1\",\"@keld/kipc\":\"0.0.1\"}}\n"
);
const MAIN: &str = "main/app.mjs";
const PRELOAD: &str = "bootstrap/app-preload.cjs";
const PRELOAD_BYTES: &[u8] = b"// Independent non-executed selected preload fixture.\n";

fn fixture_digest(value: &str) -> [u8; 32] {
    assert_eq!(value.len(), 64, "independent fixture receipt width");
    let mut digest = [0_u8; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        digest[index] = u8::from_str_radix(std::str::from_utf8(pair).expect("literal hex"), 16)
            .expect("independent literal receipt");
    }
    digest
}

fn approved() -> ApprovedGraphCaptureInput {
    // Frozen independent fixture receipts from GH499 before this producer and
    // serializer. Never derive expected provider hashes from an observed graph.
    ApprovedGraphCaptureInput {
        expected_package_sha256: fixture_digest(
            "36f0819d6ec665ae79fc6fd13256fed97a93c6497b05c8e5e27f51c31c163dee",
        ),
        expected_lock_sha256: fixture_digest(
            "fa43830d726d04578607c5d9566be4198718db3bbe914b73091c288617c3476b",
        ),
        expected_package_trees: [
            "7e57f5030f61b455e6c7aecfd1554bdf1334cf9c5a98024ea388bc729b95646f",
            "24cb3148684ef325d0f0488f677f7d79c01be1ae881ab72cd758da75028fcec1",
            "e8893aeac32cb4828dce4360dd0fb432b51bf4d403a3f6b3e03b787017ff7e01",
        ]
        .map(fixture_digest),
    }
}

fn retained_directory(path: &Path) -> File {
    #[cfg(target_os = "macos")]
    {
        File::open(path).expect("retained fixture directory")
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .expect("retained fixture directory")
    }
}

struct Fixture {
    _owned: tempfile::TempDir,
    root: PathBuf,
    held: File,
    selected: SelectedRendererSource,
}

impl Fixture {
    fn new() -> Self {
        let owned = tempfile::tempdir().expect("private graph fixture");
        let root = owned.path().join("copied-app");
        fs::create_dir(&root).expect("owned copied app");
        for (name, bytes) in [
            (MAIN, b"// Inert app source; never execute.\n".as_slice()),
            (
                "client/assets/nested/editor.html",
                b"<html>different editor</html>\n",
            ),
            ("client/assets/styles.css", b"body { margin: 0; }\n"),
            (PRELOAD, PRELOAD_BYTES),
            ("package.json", PACKAGE.as_bytes()),
            ("package-lock.json", LOCK.as_bytes()),
            (
                "node_modules/electron/package.json",
                ELECTRON_PACKAGE.as_bytes(),
            ),
            (
                "node_modules/electron/dist/index.js",
                b"export const neverEvaluate = \"FIRST_PARTY_FIXTURE_ONLY\";\n",
            ),
            (
                "node_modules/@keld/api/package.json",
                b"{\"name\":\"@keld/api\",\"version\":\"0.0.1\"}\n",
            ),
            (
                "node_modules/@keld/kipc/package.json",
                b"{\"name\":\"@keld/kipc\",\"version\":\"0.0.1\"}\n",
            ),
            (
                "keld-launch-facts/owner.mjs",
                b"// exact generated bootstrap is graph-bound\n",
            ),
        ] {
            Self::write(&root, name, bytes);
        }
        let held = retained_directory(&root);
        let selected = SelectedRendererSource::new(
            "client/assets",
            "nested/editor.html",
            PRELOAD,
            Sha256::digest(PRELOAD_BYTES).into(),
        )
        .expect("independent selected source data");
        Self {
            _owned: owned,
            root,
            held,
            selected,
        }
    }

    fn write(root: &Path, name: &str, bytes: &[u8]) {
        let path = root.join(name);
        fs::create_dir_all(path.parent().expect("parent")).expect("owned fixture directories");
        fs::write(path, bytes).expect("owned fixture bytes");
    }

    fn compile(&self) -> Result<Vec<u8>, crate::app_session::HostAppError> {
        make_graph_leaf_from_retained_root(
            &self.root,
            &self.held,
            &approved(),
            &self.selected,
            MAIN,
        )
    }
}

fn qualification() -> WindowsDistributionQualification {
    WindowsDistributionQualification {
        target: windows_graph_target(),
        provider_trees: approved().expected_package_trees,
    }
}

fn fixture_with_entry() -> Fixture {
    let f = Fixture::new();
    Fixture::write(
        &f.root,
        ORIGINAL_RUNTIME_ENTRY_PATH,
        &windows_original_entry_bootstrap_source(MAIN).expect("canonical Windows source"),
    );
    f
}

fn bind(
    f: &Fixture,
    q: Option<&WindowsDistributionQualification>,
) -> Result<AdmittedStageGraph, crate::app_session::HostAppError> {
    let bytes = f.compile()?;
    fs::write(f.root.join(GRAPH_FILE), &bytes).expect("same captured leaf");
    bind_with_qualification(
        &f.root,
        &f.held,
        Sha256::digest(&bytes).into(),
        Path::new(MAIN),
        &f.selected,
        q,
    )
}

#[test]
fn production_boot8_refuses_even_a_self_consistent_real_graph_without_receipt() {
    let f = fixture_with_entry();
    let bytes = f.compile().expect("fixture graph");
    fs::write(f.root.join(GRAPH_FILE), &bytes).expect("same graph");
    let missing = bind_with_qualification(
        &f.root,
        &f.held,
        Sha256::digest(&bytes).into(),
        Path::new(MAIN),
        &f.selected,
        None,
    )
    .expect_err("an explicit missing qualification always refuses");
    assert!(
        missing
            .to_string()
            .contains("no independently qualified Windows target/provider"),
        "{missing}"
    );
    let qualified = qualified_windows_distribution();
    #[cfg(all(target_arch = "x86_64", target_env = "msvc"))]
    {
        let qualified = qualified.expect("the reviewed Windows target has a compiled receipt");
        assert_eq!(qualified.target(), "x86_64-pc-windows-msvc");
        assert_ne!(
            qualified.provider_trees(),
            &approved().expected_package_trees
        );
    }
    #[cfg(not(all(target_arch = "x86_64", target_env = "msvc")))]
    assert!(
        qualified.is_none(),
        "other Windows targets remain unqualified"
    );
    let error = read_windows_bound_graph_from_retained_root(
        &f.root,
        &f.held,
        Sha256::digest(&bytes).into(),
        Path::new(MAIN),
        &f.selected,
    )
    .expect_err("inert fixture providers cannot use the production receipt");
    #[cfg(all(target_arch = "x86_64", target_env = "msvc"))]
    assert!(
        error
            .to_string()
            .contains("external provider identity differs"),
        "{error}"
    );
    #[cfg(not(all(target_arch = "x86_64", target_env = "msvc")))]
    assert!(
        error
            .to_string()
            .contains("no independently qualified Windows target/provider"),
        "{error}"
    );
}

#[test]
fn target_and_provider_receipts_cannot_be_replaced_by_matching_project_data() {
    let f = fixture_with_entry();
    let mut foreign = qualification();
    foreign.target = "foreign-target";
    let error = bind(&f, Some(&foreign)).expect_err("foreign target");
    assert!(
        error.to_string().contains("foreign build target"),
        "{error}"
    );
    let mut wrong_provider = qualification();
    wrong_provider.provider_trees[1][0] ^= 1;
    let error = bind(&f, Some(&wrong_provider)).expect_err("independent provider mismatch");
    assert!(
        error
            .to_string()
            .contains("external provider identity differs"),
        "{error}"
    );
}

#[test]
fn exact_generated_entry_file_is_pinned_until_the_actual_graph_owner_drops() {
    let f = fixture_with_entry();
    let graph = bind(&f, Some(&qualification())).expect("inert internal fixture receipt");
    let path = f.root.join(ORIGINAL_RUNTIME_ENTRY_PATH);
    assert_eq!(
        graph
            .windows_runtime_entry(&f.root, Path::new(MAIN))
            .expect("same owner"),
        path
    );
    assert!(
        graph
            .windows_runtime_entry(&f.root.join("other"), Path::new(MAIN))
            .is_err()
    );
    assert!(
        graph
            .windows_runtime_entry(&f.root, Path::new("other.mjs"))
            .is_err()
    );
    assert!(
        fs::write(&path, b"replacement").is_err(),
        "actual FILE_SHARE_READ denies writes"
    );
    assert!(
        fs::remove_file(&path).is_err(),
        "actual held file denies deletion"
    );
    drop(graph);
    fs::write(&path, b"replacement").expect("last entry owner actually released");
}

#[test]
fn a_self_consistent_graph_cannot_change_or_omit_the_canonical_entry() {
    let f = fixture_with_entry();
    Fixture::write(
        &f.root,
        ORIGINAL_RUNTIME_ENTRY_PATH,
        b"// foreign self-consistent source\n",
    );
    let error = bind(&f, Some(&qualification())).expect_err("graph hash is not template authority");
    assert!(
        error
            .to_string()
            .contains("graph does not pin canonical bytes"),
        "{error}"
    );
    fs::remove_file(f.root.join(ORIGINAL_RUNTIME_ENTRY_PATH)).expect("no retained admitted owner");
    let error = bind(&f, Some(&qualification())).expect_err("entry missing");
    assert!(
        error.to_string().contains("canonical entry is absent"),
        "{error}"
    );
}

#[test]
fn retained_root_and_original_digest_are_checked_before_graph_admission() {
    let f = fixture_with_entry();
    let bytes = f.compile().expect("fixture graph");
    fs::write(f.root.join(GRAPH_FILE), &bytes).expect("same graph");
    let error = bind_with_qualification(
        &f.root,
        &f.held,
        [0; 32],
        Path::new(MAIN),
        &f.selected,
        Some(&qualification()),
    )
    .expect_err("wrong compiled digest");
    assert!(error.to_string().contains("graph hash differs"), "{error}");
    let other = Fixture::new();
    let error = bind_with_qualification(
        &other.root,
        &f.held,
        Sha256::digest(&bytes).into(),
        Path::new(MAIN),
        &f.selected,
        Some(&qualification()),
    )
    .expect_err("a foreign retained root cannot select this graph");
    assert!(
        error.to_string().contains("root was substituted"),
        "{error}"
    );
}

#[test]
fn retained_graph_rechecks_file_bytes_and_same_byte_preload_replacements() {
    use keld_runtime::primary::RoleSourceRecheck;

    let fixture = fixture_with_entry();
    let graph = bind(&fixture, Some(&qualification())).expect("retained inert source");
    RoleSourceRecheck::recheck(&graph).expect("unchanged graph before the role attempt");
    let preload = fixture.root.join(PRELOAD);
    assert_eq!(
        graph
            .read_selected_preload_bytes(&preload)
            .expect("selected bytes"),
        PRELOAD_BYTES
    );
    assert!(
        graph
            .read_selected_preload_bytes(&fixture.root.join(MAIN))
            .is_err()
    );

    fs::write(fixture.root.join(MAIN), b"// changed source").expect("change the actual fixture");
    let error = RoleSourceRecheck::recheck(&graph).expect_err("the original entry changed");
    assert!(error.to_string().contains("file size"), "{error}");
    fs::write(
        fixture.root.join(MAIN),
        b"// Inert app source; never execute.\n",
    )
    .expect("restore exact original bytes");
    graph.recheck().expect("restored source bytes");

    let previous = fixture.root.join("preload-before.cjs");
    fs::rename(&preload, &previous).expect("preserve the original file identity");
    fs::write(&preload, PRELOAD_BYTES).expect("different file with identical bytes");
    fs::remove_file(previous).expect("restore graph membership");
    let error = graph
        .read_selected_preload_bytes(&preload)
        .expect_err("different file identity");
    assert!(
        error.to_string().contains("selected file replaced"),
        "{error}"
    );
}

#[test]
fn provider_preflight_uses_independent_pinned_subtree_receipts() {
    let fixture = fixture_with_entry();
    let pins = approved().expected_package_trees;
    verify_provider_trees_from_retained_root(&fixture.root, &fixture.held, &pins)
        .expect("unchanged fixture providers");
    fs::write(
        fixture.root.join("node_modules/electron/dist/index.js"),
        b"// changed provider",
    )
    .expect("change one physical provider member");
    let error = verify_provider_trees_from_retained_root(&fixture.root, &fixture.held, &pins)
        .expect_err("provider content differs");
    assert!(
        error
            .to_string()
            .contains("external provider identity differs"),
        "{error}"
    );
}
