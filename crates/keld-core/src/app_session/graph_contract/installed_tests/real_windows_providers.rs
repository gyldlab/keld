//! Real independently reviewed Windows providers on the existing graph owner.
//! Package/lock/main/preload are the existing inert fixture, never draw.io.
//! The ignored controls require preserved GH502 inputs; neither production
//! qualification getter is changed or populated from the supplied directory.

use super::super::windows_boot::{
    WindowsDistributionQualification, bind_with_qualification, qualified_windows_distribution,
    read_windows_bound_graph_from_retained_root,
};
use super::super::{
    borrow_retained_root, make_graph_leaf_from_retained_root, make_graph_leaf_on_retained_root,
    verify_provider_trees_from_retained_root,
};
use super::{
    AdmittedStageGraph, ApprovedGraphCaptureInput, Fixture, GRAPH_FILE, MAIN, PRELOAD,
    PRELOAD_BYTES, approved, fixture_digest,
};
use crate::app_session::{
    HostAppError, ORIGINAL_RUNTIME_ENTRY_PATH, SelectedRendererSource,
    original_entry_bootstrap_source,
};
use sha2::{Digest as _, Sha256};
use std::fs::{self, File};
use std::os::windows::fs::MetadataExt as _;
use std::path::{Component, Path, PathBuf, Prefix};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

const INPUT_ENV: &str = "KELD_TEST_WINDOWS_PROVIDER_INPUT";
const PROVIDERS: [(&str, &str); 3] = [
    ("electron", "node_modules/electron"),
    ("api", "node_modules/@keld/api"),
    ("kipc", "node_modules/@keld/kipc"),
];

fn reviewed_inputs() -> ApprovedGraphCaptureInput {
    let mut input = approved();
    // Independent receipt c5fcc01bb5fd421dafd42637897e49af7d8e29a54645003227597519d26aa22c.
    // Source metadata remains the existing frozen inert fixture, not observed
    // package/lock bytes. These provider expectations never come from this copy.
    input.expected_package_trees = [
        "b4c8e8f14fba45d8f3ef5ea044db903993b257c6f3ecb89722c0b76438380118",
        "10bab86f55ddba99968f93520a6dbd90bc153d1a149b3af84cf1b7fe5ca48fa7",
        "81c253df16a3c527dcaf72845df5aa9605efbeef1273e8a454f4b83e43c50562",
    ]
    .map(fixture_digest);
    input
}

fn qualification() -> WindowsDistributionQualification {
    WindowsDistributionQualification {
        target: "x86_64-pc-windows-msvc",
        provider_trees: reviewed_inputs().expected_package_trees,
    }
}

fn require_d_path(path: &Path) {
    let on_d = matches!(
        path.components().next(),
        Some(Component::Prefix(prefix))
            if matches!(
                prefix.kind(),
                Prefix::Disk(b'D' | b'd') | Prefix::VerbatimDisk(b'D' | b'd')
            )
    );
    assert!(
        path.is_absolute() && on_d,
        "this explicit acceptance fixture requires an absolute D: path: {}",
        path.display()
    );
}

fn copy_provider(source: &Path, target: &Path, depth: usize, files: &mut usize) {
    assert!(depth <= 16, "preserved provider fixture depth");
    let metadata = fs::symlink_metadata(source).expect("preserved input metadata");
    assert_eq!(
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT,
        0,
        "a fixture input cannot substitute a reparse target"
    );
    if metadata.is_dir() {
        fs::create_dir(target).expect("new owned provider directory");
        for entry in fs::read_dir(source).expect("preserved provider directory") {
            let entry = entry.expect("preserved provider member");
            copy_provider(
                &entry.path(),
                &target.join(entry.file_name()),
                depth + 1,
                files,
            );
        }
    } else {
        assert!(metadata.is_file(), "preserved input must be regular");
        assert!(metadata.len() <= 512 * 1024, "known provider file bound");
        *files += 1;
        assert!(*files <= 69, "independent preserved provider census");
        assert_eq!(
            fs::copy(source, target).expect("copy into private owned fixture"),
            metadata.len()
        );
    }
}

struct RealProviderFixture {
    // Actual root handle drops before TempDir on either normal return or panic.
    held: File,
    root: PathBuf,
    selected: SelectedRendererSource,
    owned: tempfile::TempDir,
}

impl RealProviderFixture {
    fn new() -> Self {
        require_d_path(&std::env::temp_dir());
        let input = PathBuf::from(
            std::env::var_os(INPUT_ENV).expect("set the explicit preserved GH502 fixture path"),
        );
        require_d_path(&input);
        let Fixture {
            owned,
            root,
            held,
            selected,
        } = Fixture::new();
        let result = Self {
            held,
            root,
            selected,
            owned,
        };
        let mut files = 0;
        for (input_name, relative) in PROVIDERS {
            let target = result.root.join(relative);
            // Only the freshly created inert provider subtree belongs here.
            // The preserved input and the actual installed consumer stay read-only.
            fs::remove_dir_all(&target).expect("replace only owned inert provider input");
            copy_provider(&input.join(input_name), &target, 0, &mut files);
        }
        assert_eq!(files, 69, "independent reviewed provider file count");
        Fixture::write(
            &result.root,
            ORIGINAL_RUNTIME_ENTRY_PATH,
            &original_entry_bootstrap_source(MAIN).expect("canonical runtime entry bytes"),
        );
        result
    }

    fn preflight(&self) -> Result<(), HostAppError> {
        verify_provider_trees_from_retained_root(
            &self.root,
            &self.held,
            &reviewed_inputs().expected_package_trees,
        )
    }

    fn capture(&self) -> Result<Vec<u8>, HostAppError> {
        make_graph_leaf_from_retained_root(
            &self.root,
            &self.held,
            &reviewed_inputs(),
            &self.selected,
            MAIN,
        )
    }

    fn bind(&self, bytes: &[u8]) -> Result<AdmittedStageGraph, HostAppError> {
        fs::write(self.root.join(GRAPH_FILE), bytes).expect("owned exact graph leaf");
        bind_with_qualification(
            &self.root,
            &self.held,
            Sha256::digest(bytes).into(),
            Path::new(MAIN),
            &self.selected,
            Some(&qualification()),
        )
    }

    fn close(self) {
        let Self {
            held,
            root,
            selected: _,
            owned,
        } = self;
        drop(held);
        owned
            .close()
            .expect("actual fixture handles released before cleanup");
        assert!(!root.exists(), "the owned copied fixture is removed");
    }
}

#[test]
#[ignore = "requires reviewed Windows GH502 provider inputs and D-owned temporary storage"]
fn reviewed_windows_provider_files_pass_retained_preflight_and_boot8_fixture() {
    let fixture = RealProviderFixture::new();
    fixture
        .preflight()
        .expect("actual preserved provider receipts");
    assert!(
        !fixture.root.join(GRAPH_FILE).exists(),
        "preflight writes no graph"
    );
    let bytes = fixture
        .capture()
        .expect("same owner captures real providers");
    fs::write(fixture.root.join(GRAPH_FILE), &bytes).expect("owned graph leaf");
    let result = read_windows_bound_graph_from_retained_root(
        &fixture.root,
        &fixture.held,
        Sha256::digest(&bytes).into(),
        Path::new(MAIN),
        &fixture.selected,
    );
    #[cfg(all(target_arch = "x86_64", target_env = "msvc"))]
    {
        let qualified = qualified_windows_distribution()
            .expect("the reviewed Windows target has a compiled receipt");
        assert_eq!(qualified.target(), "x86_64-pc-windows-msvc");
        assert_eq!(
            qualified.provider_trees(),
            &reviewed_inputs().expected_package_trees,
            "compiled providers equal the independently frozen three trees"
        );
        let graph = result.expect("actual compiled receipt admits the reviewed providers");
        graph.recheck().expect("unchanged real provider graph");
        assert_eq!(
            graph
                .read_selected_preload_bytes(&fixture.root.join(PRELOAD))
                .expect("actual bound fixture source"),
            PRELOAD_BYTES
        );
        let entry = fixture.root.join(ORIGINAL_RUNTIME_ENTRY_PATH);
        assert!(
            fs::write(&entry, b"replacement").is_err(),
            "actual retained File denies write"
        );
        assert!(
            fs::remove_file(&entry).is_err(),
            "actual retained File denies delete"
        );
        drop(graph);
        fs::write(&entry, b"released fixture entry").expect("same File owner actually released");
    }
    #[cfg(not(all(target_arch = "x86_64", target_env = "msvc")))]
    {
        assert!(
            qualified_windows_distribution().is_none(),
            "other Windows targets remain unqualified"
        );
        let error = result.expect_err("actual providers do not qualify another target");
        assert!(
            error
                .to_string()
                .contains("no independently qualified Windows target/provider"),
            "{error}"
        );
    }
    fixture.close();
}

#[test]
#[ignore = "requires reviewed Windows GH502 provider inputs and D-owned temporary storage"]
fn changed_missing_extra_or_malformed_actual_provider_files_are_refused() {
    for mutation in ["changed", "missing", "extra", "malformed", "hardlink"] {
        let fixture = RealProviderFixture::new();
        fixture
            .preflight()
            .expect("healthy actual input before mutation");
        match mutation {
            "changed" => {
                let path = fixture.root.join("node_modules/electron/dist/index.js");
                let mut bytes = fs::read(&path).expect("actual provider body");
                bytes[0] ^= 1;
                fs::write(path, bytes).expect("one-byte actual provider mutation");
            }
            "missing" => {
                fs::remove_file(fixture.root.join("node_modules/@keld/api/package.json"))
                    .expect("remove only the owned actual provider member");
            }
            "extra" => {
                fs::write(
                    fixture.root.join("node_modules/@keld/kipc/keld.boot.json"),
                    b"extra provider input cannot hide behind root sidecar exclusions",
                )
                .expect("extra actual provider member");
            }
            "hardlink" => {
                fs::hard_link(
                    fixture.root.join("node_modules/electron/dist/index.js"),
                    fixture.root.join("node_modules/electron/dist/alias.js"),
                )
                .expect("actual alias only inside the owned copied provider");
            }
            "malformed" => {
                fs::write(
                    fixture.root.join("node_modules/electron/package.json"),
                    b"{ malformed actual provider manifest",
                )
                .expect("malformed actual provider manifest");
            }
            _ => unreachable!(),
        }
        assert!(
            fixture.preflight().is_err(),
            "canonical preflight accepted {mutation}"
        );
        assert!(
            fixture.capture().is_err(),
            "canonical graph accepted {mutation}"
        );
        fixture.close();
    }
}

#[test]
#[ignore = "requires reviewed Windows GH502 provider inputs and D-owned temporary storage"]
fn a_matching_reissued_graph_cannot_qualify_changed_actual_provider_bytes() {
    let fixture = RealProviderFixture::new();
    fixture
        .preflight()
        .expect("healthy independently reviewed providers");
    let path = fixture.root.join("node_modules/@keld/api/index.js");
    let mut bytes = fs::read(&path).expect("actual API provider bytes");
    bytes.push(b'\n');
    fs::write(path, bytes).expect("changed actual provider source");
    assert!(
        fixture.capture().is_err(),
        "fixed independent input must refuse"
    );
    // This canonical data-only capture models a matching attacker-controlled
    // descriptor. It does not accept any qualification or create a native grant.
    let retained = borrow_retained_root(&fixture.root, &fixture.held).expect("same held root");
    let changed_graph = make_graph_leaf_on_retained_root(&fixture.root, &retained, MAIN)
        .expect("capture a self-consistent changed descriptor as data");
    drop(retained);
    let error = fixture
        .bind(&changed_graph)
        .expect_err("matching graph digest is not authority");
    assert!(
        error
            .to_string()
            .contains("external provider identity differs"),
        "{error}"
    );
    fixture.close();
}

#[test]
#[ignore = "requires reviewed Windows GH502 provider inputs and D-owned temporary storage"]
fn retained_real_provider_graph_rechecks_source_and_lock_custody() {
    for relative in [MAIN, PRELOAD, "package.json", "package-lock.json"] {
        let fixture = RealProviderFixture::new();
        let bytes = fixture
            .capture()
            .expect("same independently pinned fixture inputs");
        let graph = fixture.bind(&bytes).expect("actual retained graph");
        graph.recheck().expect("healthy before source mutation");
        let path = fixture.root.join(relative);
        let mut changed = fs::read(&path).expect("actual source/lock bytes");
        changed.push(b' ');
        fs::write(path, changed).expect("change only the owned source/lock file");
        assert!(
            graph.recheck().is_err(),
            "retained graph accepted changed {relative}"
        );
        if relative != MAIN {
            assert!(
                fixture.capture().is_err(),
                "fixed source receipt accepted changed {relative}"
            );
        }
        // A new development capture may select newly edited main bytes. The
        // existing admitted graph must still refuse them before reprovision.
        drop(graph);
        fixture.close();
    }
    let fixture = RealProviderFixture::new();
    let bytes = fixture
        .capture()
        .expect("fresh independently pinned fixture");
    let graph = fixture
        .bind(&bytes)
        .expect("actual selected file identities");
    let replacement = fixture.owned.path().join("replacement-preload.cjs");
    fs::write(&replacement, PRELOAD_BYTES).expect("identical bytes in a new physical file");
    fs::rename(&replacement, fixture.root.join(PRELOAD)).expect("replace only copied preload");
    assert!(
        graph.recheck().is_err(),
        "same bytes cannot substitute the selected file identity"
    );
    drop(graph);
    fixture.close();
}

#[test]
#[ignore = "requires reviewed Windows GH502 provider inputs and D-owned temporary storage"]
fn real_provider_preflight_refuses_an_unrelated_retained_root() {
    let original = RealProviderFixture::new();
    let other = RealProviderFixture::new();
    original.preflight().expect("original actual input");
    other.preflight().expect("byte-identical independent input");
    assert!(
        verify_provider_trees_from_retained_root(
            &other.root,
            &original.held,
            &reviewed_inputs().expected_package_trees,
        )
        .is_err(),
        "equal provider bytes cannot substitute the retained root"
    );
    original.close();
    other.close();
}
