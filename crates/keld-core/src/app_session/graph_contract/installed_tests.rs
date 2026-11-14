//! Independent literal source/provider receipts exercise the retained installed
//! graph owner. No fixture JavaScript is evaluated or passed off as real draw.io.

use crate::app_session::graph_selection::SelectedRendererSource;

use std::fs::{self, File};
#[cfg(target_os = "macos")]
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::{
    AdmittedStageGraph, ApprovedGraphCaptureInput, GRAPH_FILE, make_graph_leaf_from_retained_root,
    read_bound_graph_leaf_from_retained_root,
};

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

fn untrusted_alias(source: &Path, target: &Path) {
    #[cfg(target_os = "macos")]
    std::os::unix::fs::symlink(source, target).expect("untrusted symlink alias");
    #[cfg(windows)]
    fs::hard_link(source, target).expect("untrusted hardlink alias");
}

struct Fixture {
    owned: tempfile::TempDir,
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
            owned,
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

    fn admitted(&self) -> AdmittedStageGraph {
        let bytes = self.compile().expect("independently pinned copied fixture");
        fs::write(self.root.join(GRAPH_FILE), &bytes).expect("fixed graph leaf");
        read_bound_graph_leaf_from_retained_root(
            &self.root,
            &self.held,
            Sha256::digest(&bytes).into(),
            MAIN,
            &self.selected,
        )
        .expect("bound graph and selected source")
    }
}

#[test]
fn retained_installed_graph_compiles_and_binds_an_unrelated_nested_editor() {
    let fixture = Fixture::new();
    let first = fixture.compile().expect("qualified inputs");
    assert_eq!(
        first,
        fixture
            .compile()
            .expect("same captured graph is deterministic")
    );
    let document: serde_json::Value = serde_json::from_slice(&first).expect("graph JSON");
    assert_eq!(document["schema"], "keld.stage.graph/v1");
    assert_eq!(document["entry"], MAIN);
    assert_eq!(document.as_object().expect("object").len(), 3);
    assert!(
        document["files"]
            .as_array()
            .expect("members")
            .iter()
            .any(|row| row["path"] == "keld-launch-facts/owner.mjs")
    );
    let graph = fixture.admitted();
    graph.recheck().expect("unchanged bound graph");
    assert_eq!(
        graph
            .read_selected_preload_bytes(&fixture.root.join(PRELOAD))
            .expect("selected preload"),
        PRELOAD_BYTES
    );
    let (code_root, assets, root, entry) = graph
        .verified_selected_source()
        .expect("private selected source")
        .into_parts();
    assert_eq!(root, fixture.root);
    assert_eq!(entry, Path::new("client/assets/nested/editor.html"));
    #[cfg_attr(
        windows,
        expect(
            clippy::useless_conversion,
            reason = "The same conversion consumes the macOS OwnedFd; Windows already supplies File, so retain one unchanged custody expression."
        )
    )]
    let retained = super::file_metadata(&File::from(code_root)).expect("actual retained code root");
    let selected =
        super::path_metadata(root.join("client/assets")).expect("selected source directory");
    assert_eq!(
        (retained.dev(), retained.ino()),
        (selected.dev(), selected.ino())
    );
    assert_eq!(assets.len(), 2);
    let selected_asset = assets
        .iter()
        .find(|asset| asset.path == Path::new("nested/editor.html"))
        .expect("selected nested resource in verified inventory");
    assert_eq!(
        selected_asset.size,
        b"<html>different editor</html>\n".len() as u64
    );
    assert_eq!(
        selected_asset.sha256,
        <[u8; 32]>::from(Sha256::digest(b"<html>different editor</html>\n"))
    );
    assert!(!fixture.root.join("app-ran").exists());
}

#[test]
fn retained_graph_producer_requires_each_independent_expected_receipt() {
    let fixture = Fixture::new();
    for changed in 0..5 {
        let mut expected = approved();
        match changed {
            0 => expected.expected_package_sha256[0] ^= 1,
            1 => expected.expected_lock_sha256[0] ^= 1,
            index => expected.expected_package_trees[index - 2][0] ^= 1,
        }
        assert!(
            make_graph_leaf_from_retained_root(
                &fixture.root,
                &fixture.held,
                &expected,
                &fixture.selected,
                MAIN
            )
            .is_err(),
            "receipt {changed}"
        );
    }
}

#[test]
fn retained_graph_binding_rejects_wrong_graph_main_and_selected_preload() {
    let fixture = Fixture::new();
    let bytes = fixture.compile().expect("qualified graph");
    fs::write(fixture.root.join(GRAPH_FILE), &bytes).expect("fixed leaf");
    let hash: [u8; 32] = Sha256::digest(&bytes).into();
    assert!(
        read_bound_graph_leaf_from_retained_root(
            &fixture.root,
            &fixture.held,
            [0; 32],
            MAIN,
            &fixture.selected
        )
        .is_err()
    );
    assert!(
        read_bound_graph_leaf_from_retained_root(
            &fixture.root,
            &fixture.held,
            hash,
            "main/other.mjs",
            &fixture.selected
        )
        .is_err()
    );
    let wrong_preload =
        SelectedRendererSource::new("client/assets", "nested/editor.html", PRELOAD, [0; 32])
            .expect("data only");
    assert!(
        read_bound_graph_leaf_from_retained_root(
            &fixture.root,
            &fixture.held,
            hash,
            MAIN,
            &wrong_preload
        )
        .is_err()
    );
    let wrong_entry = SelectedRendererSource::new(
        "client/assets",
        "missing.html",
        PRELOAD,
        Sha256::digest(PRELOAD_BYTES).into(),
    )
    .expect("data only");
    assert!(
        read_bound_graph_leaf_from_retained_root(
            &fixture.root,
            &fixture.held,
            hash,
            MAIN,
            &wrong_entry
        )
        .is_err()
    );
}

#[test]
fn retained_graph_refuses_unlisted_files_empty_directories_and_aliases() {
    for mutation in [
        "file",
        "empty-dir",
        "alias",
        "changed-resource",
        "changed-bootstrap",
    ] {
        let fixture = Fixture::new();
        let graph = fixture.admitted();
        match mutation {
            "file" => fs::write(fixture.root.join("unlisted.json"), b"{}").expect("extra file"),
            "empty-dir" => {
                fs::create_dir(fixture.root.join("unlisted-directory")).expect("empty directory");
            }
            "alias" => untrusted_alias(&fixture.root.join(MAIN), &fixture.root.join("alias.mjs")),
            "changed-resource" => fs::write(
                fixture.root.join("client/assets/styles.css"),
                b"body{display:none}\n",
            )
            .expect("changed resource"),
            "changed-bootstrap" => fs::write(
                fixture.root.join("keld-launch-facts/owner.mjs"),
                b"// changed\n",
            )
            .expect("changed fixed owner"),
            _ => unreachable!(),
        }
        assert!(graph.recheck().is_err(), "mutation {mutation}");
    }
}

#[test]
fn retained_graph_parser_is_closed_and_does_not_normalize_duplicate_or_unsorted_rows() {
    let fixture = Fixture::new();
    let bytes = fixture.compile().expect("qualified graph");
    let document: serde_json::Value = serde_json::from_slice(&bytes).expect("graph");
    let mut unknown = document.clone();
    unknown["grant"] = serde_json::json!("all");
    let mut reordered = document.clone();
    reordered["files"].as_array_mut().expect("rows").reverse();
    let mut duplicate = document.clone();
    let row = duplicate["files"][0].clone();
    duplicate["files"]
        .as_array_mut()
        .expect("rows")
        .insert(0, row);
    let text = String::from_utf8(bytes).expect("UTF8 graph");
    for broken in [
        unknown.to_string(),
        reordered.to_string(),
        duplicate.to_string(),
        text.replacen(
            "\"schema\":",
            "\"schema\":\"keld.stage.graph/v1\",\"schema\":",
            1,
        ),
        format!("{text} {{}}"),
    ] {
        fs::write(fixture.root.join(GRAPH_FILE), broken.as_bytes())
            .expect("invalid but digest-bound graph");
        assert!(
            read_bound_graph_leaf_from_retained_root(
                &fixture.root,
                &fixture.held,
                Sha256::digest(broken.as_bytes()).into(),
                MAIN,
                &fixture.selected
            )
            .is_err()
        );
    }
}

#[test]
fn graph_census_excludes_only_the_platform_host_and_fixed_boot_sidecars() {
    let fixture = Fixture::new();
    #[cfg(windows)]
    let host = "keld-host.exe";
    #[cfg(target_os = "macos")]
    let host = "keld-host";
    for leaf in [host, "keld.boot.json", "keld.permissions.jsonc"] {
        fs::write(fixture.root.join(leaf), b"fixed external owner").expect("host sidecar");
    }
    let bytes = fixture
        .compile()
        .expect("host sidecars excluded from source graph");
    let document: serde_json::Value = serde_json::from_slice(&bytes).expect("graph");
    let rows = document["files"].as_array().expect("file rows");
    for leaf in [host, "keld.boot.json", "keld.permissions.jsonc"] {
        assert!(!rows.iter().any(|row| row["path"] == leaf), "{leaf}");
    }
    fixture
        .admitted()
        .recheck()
        .expect("sidecars do not expand the source grant");
}

#[test]
fn selected_preload_replacement_with_identical_bytes_refuses_graph_recheck() {
    let fixture = Fixture::new();
    let graph = fixture.admitted();
    graph.recheck().expect("original selected identity");
    let replacement = fixture.owned.path().join("new-preload");
    fs::write(&replacement, PRELOAD_BYTES).expect("independently allocated identical bytes");
    fs::rename(&replacement, fixture.root.join(PRELOAD)).expect("replace selected inode");
    assert!(
        graph.recheck().is_err(),
        "digest equality does not authorize a substituted selected file"
    );
}

#[cfg(windows)]
#[test]
fn retained_graph_root_identity_rejects_an_unrelated_locator() {
    let fixture = Fixture::new();
    let other = fixture.owned.path().join("other-root");
    fs::create_dir(&other).expect("unrelated real directory");
    assert!(super::borrow_retained_root(&other, &fixture.held).is_err());
    assert!(super::borrow_retained_root(&fixture.root, &fixture.held).is_ok());
}

#[test]
fn provider_preflight_uses_independent_receipts_before_any_stage_exists() {
    let fixture = Fixture::new();
    let expected = approved().expected_package_trees;
    super::verify_provider_trees_from_retained_root(&fixture.root, &fixture.held, &expected)
        .expect("fixed independent provider receipts");
    for changed in 0..3 {
        let mut wrong = expected;
        wrong[changed][0] ^= 1;
        assert!(
            super::verify_provider_trees_from_retained_root(&fixture.root, &fixture.held, &wrong,)
                .is_err(),
            "provider {changed}"
        );
    }
    assert!(
        !fixture.root.join(".keld").exists(),
        "preflight creates no stage"
    );
    fs::write(
        fixture.root.join("node_modules/electron/keld.boot.json"),
        b"extra provider file",
    )
    .expect("provider member resembling a root-only sidecar");
    assert!(
        super::verify_provider_trees_from_retained_root(&fixture.root, &fixture.held, &expected,)
            .is_err(),
        "root-only exclusions must never hide provider files"
    );
}

#[test]
fn provider_preflight_ignores_unrelated_project_inputs_and_refuses_changed_provider_bytes() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("keld.config.ts"), b"never evaluate").expect("project config");
    fs::create_dir(fixture.root.join(".git")).expect("unrelated project metadata");
    let expected = approved().expected_package_trees;
    super::verify_provider_trees_from_retained_root(&fixture.root, &fixture.held, &expected)
        .expect("only the canonical providers are preflight inputs");
    fs::write(
        fixture.root.join("node_modules/electron/dist/index.js"),
        b"changed provider",
    )
    .expect("provider mutation");
    assert!(
        super::verify_provider_trees_from_retained_root(&fixture.root, &fixture.held, &expected,)
            .is_err()
    );
    assert!(!fixture.root.join(".keld").exists());
}

#[cfg(windows)]
#[test]
fn core_prepares_windows_source_and_selected_preload_from_the_same_graph_owner() {
    use crate::app_session::original_webapp::PreparedOriginalSource;
    use keld_wv::preload_image::AppPreloadBudget;
    use std::sync::Arc;

    const POLICY: &str = "{\"profile\":\"legacy\",\"app\":{},\"windows\":{\"fixture\":{\"web\":{\"csp\":\"original-editor-offline\"}}}}\n";
    const EXPECTED: [u8; 32] = [
        215, 46, 191, 72, 140, 173, 92, 255, 48, 59, 90, 236, 196, 197, 67, 68, 75, 213, 119, 101,
        68, 44, 13, 125, 203, 40, 242, 147, 25, 32, 233, 33,
    ];
    let fixture = Fixture::new();
    let graph = Arc::new(fixture.admitted());
    let policy_path = fixture.root.join("keld.permissions.jsonc");
    fs::write(&policy_path, POLICY).expect("fixed independently pinned policy");
    let policy = keld_guard::verified_manifest::load_verified_manifest(
        File::open(&policy_path).expect("policy handle"),
        policy_path,
        EXPECTED,
    )
    .expect("same immutable policy owner")
    .resolve_window_csp("fixture")
    .expect("trusted fixed window key");
    let budget = AppPreloadBudget::default();
    let prepared = PreparedOriginalSource::prepare(
        Arc::clone(&graph),
        keld_guard::ProfileState::Legacy,
        policy,
        budget.clone(),
    )
    .expect("retained graph into the shared source owner");
    assert_eq!(prepared.graph_sha256(), graph.graph_sha256());
    assert_eq!(prepared.selected_stage(), fixture.root);
    assert_eq!(prepared.window_key(), "fixture");
    assert_eq!(prepared.source().verified_manifest_sha256(), EXPECTED);
    // Literal modeled facts exercise immutable preload capture, not a real Bun
    // runtime observation or native execution receipt.
    let selection = prepared.with_runtime_versions(std::collections::BTreeMap::from([(
        String::from("fixture"),
        String::from("1"),
    )]));
    let uri = format!(
        "file:///{}/client/assets/nested/editor.html?queryObj=%257b",
        fixture
            .root
            .to_str()
            .expect("fixture UTF8")
            .replace('\\', "/")
    );
    assert_eq!(
        selection
            .qualify_uri(&uri)
            .expect("same selected-entry parser")
            .as_optional_str(),
        Ok(Some("queryObj=%257b"))
    );
    assert!(
        selection
            .qualify_uri(&uri.replace("nested/editor.html", "styles.css"))
            .is_err()
    );
    let image = selection
        .prepare_preload(
            fixture
                .root
                .join(PRELOAD)
                .to_str()
                .expect("canonical selected preload"),
            Vec::new(),
        )
        .expect("same selected image and shared budget");
    assert_eq!(image.source_bytes(), PRELOAD_BYTES.len());
    assert_eq!(budget.retained_bytes(), Some(PRELOAD_BYTES.len()));
    drop(image);
    assert_eq!(budget.retained_bytes(), Some(0));
}

#[cfg(windows)]
mod windows_boot;

#[cfg(windows)]
mod windows_preload_selector;

#[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
mod real_windows_providers;
