//! Independent literal source/provider receipts exercise the retained installed
//! graph owner. No fixture JavaScript is evaluated or passed off as real draw.io.

use std::fs::{self, File};
use std::os::unix::fs::{MetadataExt as _, symlink};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::{
    AdmittedStageGraph, ApprovedGraphCaptureInput, GRAPH_FILE, SelectedRendererSource,
    make_graph_leaf_from_retained_root, read_bound_graph_leaf_from_retained_root,
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
        let held = File::open(&root).expect("retained root");
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
    let retained = File::from(code_root)
        .metadata()
        .expect("actual retained code root");
    let selected = fs::metadata(root.join("client/assets")).expect("selected source directory");
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
fn retained_graph_refuses_unlisted_files_empty_directories_and_symlinks() {
    for mutation in [
        "file",
        "empty-dir",
        "symlink",
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
            "symlink" => symlink(fixture.root.join(MAIN), fixture.root.join("alias.mjs"))
                .expect("untrusted alias"),
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
