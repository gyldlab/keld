//! Bounded inert source for the existing original-menu and window-router tests.
//! The graph, provider pins, verified Legacy policy and native source intake
//! use their actual production owners. No app code is evaluated by this fixture.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::Path;
use std::sync::Arc;

use keld_guard::verified_manifest::load_verified_manifest;
use keld_guard::{ProfileState, WindowCspProfile};
use keld_wv::wkwebview::AppPreloadBudget;
use sha2::{Digest as _, Sha256};

use super::super::graph_contract::{
    ApprovedGraphCaptureInput, GRAPH_FILE, SelectedRendererSource,
    make_graph_leaf_from_retained_root, read_bound_graph_leaf_from_retained_root,
};
use super::super::original_webapp::{OriginalLoadSelection, PreparedOriginalSource};

const MAIN: &str = "import { app } from 'electron'; /* never execute */\n";
const ROOT_PACKAGE: &str = concat!(
    "{\"name\":\"graph-test\",\"type\":\"module\",\"version\":\"0.0.1\",",
    "\"dependencies\":{\"electron\":\"npm:@keld/electron@0.0.1\",",
    "\"@keld/api\":\"0.0.1\",\"@keld/kipc\":\"0.0.1\"}}\n"
);
const ROOT_LOCK: &str = concat!(
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
const API_PACKAGE: &str = "{\"name\":\"@keld/api\",\"version\":\"0.0.1\"}\n";
const KIPC_PACKAGE: &str = "{\"name\":\"@keld/kipc\",\"version\":\"0.0.1\"}\n";
const PRELOAD: &[u8] = b"/* pinned inert original quit preload; never evaluated */\n";
const POLICY: &str =
    r#"{"profile":"legacy","windows":{"editor":{"web":{"csp":"original-editor-offline"}}}}"#;

// Independently fixed digests of the exact inert fixture bytes. These are
// data-only test inputs, not a qualified shipping-app or publisher receipt.
const PIN_ROOT: &str = "36f0819d6ec665ae79fc6fd13256fed97a93c6497b05c8e5e27f51c31c163dee";
const PIN_LOCK: &str = "fa43830d726d04578607c5d9566be4198718db3bbe914b73091c288617c3476b";
const PIN_TREES: [&str; 3] = [
    "7e57f5030f61b455e6c7aecfd1554bdf1334cf9c5a98024ea388bc729b95646f",
    "24cb3148684ef325d0f0488f677f7d79c01be1ae881ab72cd758da75028fcec1",
    "e8893aeac32cb4828dce4360dd0fb432b51bf4d403a3f6b3e03b787017ff7e01",
];
const PIN_PRELOAD: &str = "e51d29ee9a7965876fabf5bcfe68774e3bf8061334253a02e4c9648efafd390a";
const PIN_POLICY: &str = "064b118963ba56f7c1ff05f6dc8eefa25dc3c4dc3ce955a207ab05c67341b03d";

fn sha(raw: &str) -> [u8; 32] {
    assert_eq!(raw.len(), 64, "independent SHA256 pin must be full length");
    let mut digest = [0_u8; 32];
    for (index, pair) in raw.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        digest[index] = u8::from_str_radix(std::str::from_utf8(pair).expect("SHA hex"), 16)
            .expect("lowercase hex digest");
    }
    digest
}

fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("relative fixture parent"))
        .expect("create confined fixture directory");
    fs::write(path, bytes).expect("write exact inert fixture bytes");
}

/// Exercise the real sealed-graph + resolved policy constructor selected by
/// both current menu and window-router quit-component tests. Returning the
/// `TempDir` keeps every graph descriptor and manifest path in its live scope.
pub(in crate::app_session) fn original_quit_selection() -> (tempfile::TempDir, OriginalLoadSelection)
{
    original_selection_with_versions(BTreeMap::new())
}

/// Supply explicit inert process facts when a component test builds the preload.
pub(in crate::app_session) fn original_selection_with_versions(
    versions: BTreeMap<String, String>,
) -> (tempfile::TempDir, OriginalLoadSelection) {
    let owned = tempfile::tempdir().expect("private selected-source fixture");
    let root = owned.path().join("app");
    fs::create_dir(&root).expect("separate copied app root");
    for (path, bytes) in [
        ("src/main/electron.js", MAIN.as_bytes()),
        (
            "client/assets/nested/editor.html",
            b"<html>inert verified editor</html>\n".as_slice(),
        ),
        ("src/main/electron-preload.js", PRELOAD),
        (
            "node_modules/electron/package.json",
            ELECTRON_PACKAGE.as_bytes(),
        ),
        (
            "node_modules/electron/dist/index.js",
            b"export const neverEvaluate = \"FIRST_PARTY_FIXTURE_ONLY\";\n".as_slice(),
        ),
        (
            "node_modules/@keld/api/package.json",
            API_PACKAGE.as_bytes(),
        ),
        (
            "node_modules/@keld/kipc/package.json",
            KIPC_PACKAGE.as_bytes(),
        ),
        ("package.json", ROOT_PACKAGE.as_bytes()),
        ("package-lock.json", ROOT_LOCK.as_bytes()),
    ] {
        write(&root, path, bytes);
    }

    let expected = ApprovedGraphCaptureInput {
        expected_package_sha256: sha(PIN_ROOT),
        expected_lock_sha256: sha(PIN_LOCK),
        expected_package_trees: PIN_TREES.map(sha),
    };
    let selected = SelectedRendererSource::new(
        "client/assets",
        "nested/editor.html",
        "src/main/electron-preload.js",
        sha(PIN_PRELOAD),
    )
    .expect("canonical generic editor and pinned preload paths");
    let retained_root = File::open(&root).expect("retained directory for real graph owner");
    let graph_bytes = make_graph_leaf_from_retained_root(
        &root,
        &retained_root,
        &expected,
        &selected,
        "src/main/electron.js",
    )
    .expect("exact independent package/lock/provider graph preflight");
    fs::write(root.join(GRAPH_FILE), &graph_bytes).expect("one owned graph leaf");
    let graph = Arc::new(
        read_bound_graph_leaf_from_retained_root(
            &root,
            &retained_root,
            Sha256::digest(&graph_bytes).into(),
            "src/main/electron.js",
            &selected,
        )
        .expect("one retained and graph-checked original-source selection"),
    );

    let policy_path = owned.path().join("quit-policy.jsonc");
    fs::write(&policy_path, POLICY).expect("exact verified Legacy/CSP policy");
    let verified = load_verified_manifest(
        File::open(&policy_path).expect("retained independent policy"),
        policy_path,
        sha(PIN_POLICY),
    )
    .expect("verified manifest bytes and explicit Legacy profile");
    let resolved = verified
        .resolve_unique_window_csp(WindowCspProfile::OriginalEditorOffline)
        .expect("exact original offline CSP key");
    let prepared = PreparedOriginalSource::prepare(
        graph,
        ProfileState::Legacy,
        resolved,
        AppPreloadBudget::default(),
    )
    .expect("actual native source preparation with graph and Guard");
    (owned, prepared.with_runtime_versions(versions))
}
