// GH499 test-first boundary: original draw.io/Electron are never evaluated.
// The original *reference* dependency-data cache is not used by these tests.
#[cfg(target_os = "macos")]
pub(in crate::app_session) mod gh499_graph {
    use std::fs;
    use std::io::Read as _;
    use std::path::{Path, PathBuf};

    use std::sync::Arc;

    use keld_guard::verified_manifest::load_verified_manifest;
    use keld_guard::{ProfileState, WindowCspProfile};
    use keld_wv::wkwebview::AppPreloadBudget;

    use sha2::{Digest as _, Sha256};

    use super::super::launch_facts::DevelopmentLaunchFacts;
    use super::super::original_webapp::PreparedOriginalSource;

    use super::super::graph_contract::{
        ApprovedGraphCaptureInput, GRAPH_FILE, admit_stage_graph, make_graph_leaf,
        read_bound_graph_leaf,
    };

    const APP: &str = "import { app } from 'electron'; /* never execute */\n";
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
    // These bytes prove graph-member selection only; SDK integration tests
    // execute the actual produced wrapper and its awaiting Init exchange.
    const SDK_RUNTIME_PRELOAD: &str =
        "import '@keld/kipc';\nawait Promise.resolve(); // inert graph fixture; never evaluated\n";
    const TRANSPORT_PRELOAD: &str = "export const neverEvaluate = 'canonical transport fixture';\n";
    // Fixed independent observations over the two exact fixture subtrees.
    const PIN_API_WITH_RUNTIME: &str =
        "49040e318f0e8ea7b5c942e84b01b19063ec4652a4037ce87433412612796912";
    const PIN_KIPC_WITH_TRANSPORT: &str =
        "1f7d011e394b58828e1f5392423d2e23082b1b970fe728f6b34e7c74b6937dbc";

    // Fixed independent SHA observations, computed from the exact fixture bytes
    // before this test's graph serializer existed. Rejecting a counterfeit in
    // each of the THREE packages must be independently observable.
    const PIN_ROOT: &str = "36f0819d6ec665ae79fc6fd13256fed97a93c6497b05c8e5e27f51c31c163dee";
    const PIN_LOCK: &str = "fa43830d726d04578607c5d9566be4198718db3bbe914b73091c288617c3476b";
    const PIN_TREES: [&str; 3] = [
        "7e57f5030f61b455e6c7aecfd1554bdf1334cf9c5a98024ea388bc729b95646f",
        "24cb3148684ef325d0f0488f677f7d79c01be1ae881ab72cd758da75028fcec1",
        "e8893aeac32cb4828dce4360dd0fb432b51bf4d403a3f6b3e03b787017ff7e01",
    ];

    fn digest(raw: &str) -> [u8; 32] {
        assert_eq!(raw.len(), 64);
        let mut result = [0_u8; 32];
        for (index, pair) in raw.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            result[index] = u8::from_str_radix(std::str::from_utf8(pair).expect("hex"), 16)
                .expect("pinned SHA");
        }
        result
    }

    fn approved() -> ApprovedGraphCaptureInput {
        ApprovedGraphCaptureInput {
            expected_package_sha256: digest(PIN_ROOT),
            expected_lock_sha256: digest(PIN_LOCK),
            expected_package_trees: PIN_TREES.map(digest),
        }
    }

    struct InertStage {
        owned: tempfile::TempDir,
        root: PathBuf,
    }

    impl InertStage {
        fn new() -> Self {
            let owned = tempfile::tempdir().expect("private test workspace");
            let root = owned.path().join("stage");
            fs::create_dir(&root).expect("private first-proof stage");
            for (name, contents) in [
                ("src/main/electron.js", APP),
                ("drawio/src/main/webapp/index.html", "<html>inert</html>\n"),
                ("node_modules/electron/package.json", ELECTRON_PACKAGE),
                (
                    "node_modules/electron/dist/index.js",
                    "export const neverEvaluate = \"FIRST_PARTY_FIXTURE_ONLY\";\n",
                ),
                ("node_modules/@keld/api/package.json", API_PACKAGE),
                ("node_modules/@keld/kipc/package.json", KIPC_PACKAGE),
                ("package.json", ROOT_PACKAGE),
                ("package-lock.json", ROOT_LOCK),
            ] {
                Self::write(&root, name, contents.as_bytes());
            }
            Self { owned, root }
        }
        fn write(root: &Path, name: &str, content: &[u8]) {
            let file = root.join(name);
            fs::create_dir_all(file.parent().expect("test parent")).expect("dir");
            fs::write(file, content).expect("test bytes");
        }
        fn leaf(&self, approval: &ApprovedGraphCaptureInput) -> Vec<u8> {
            let leaf = make_graph_leaf(&self.root, "src/main/electron.js", approval)
                .expect("valid standalone inert stage");
            fs::write(self.root.join(GRAPH_FILE), &leaf).expect("stage graph");
            leaf
        }
        fn selected(
            &self,
            approval: &ApprovedGraphCaptureInput,
        ) -> Result<super::super::graph_contract::AdmittedStageGraph, super::super::HostAppError>
        {
            let leaf = fs::read(self.root.join(GRAPH_FILE)).expect("graph selected");
            admit_stage_graph(&self.root, &leaf, approval)
        }
    }

    fn stage_with_preloads() -> (InertStage, ApprovedGraphCaptureInput) {
        let stage = InertStage::new();
        InertStage::write(
            &stage.root,
            "node_modules/@keld/api/runtime-preload.js",
            SDK_RUNTIME_PRELOAD.as_bytes(),
        );
        InertStage::write(
            &stage.root,
            "node_modules/@keld/kipc/transport.js",
            TRANSPORT_PRELOAD.as_bytes(),
        );
        let mut approval = approved();
        approval.expected_package_trees[1] = digest(PIN_API_WITH_RUNTIME);
        approval.expected_package_trees[2] = digest(PIN_KIPC_WITH_TRANSPORT);
        (stage, approval)
    }

    #[test]
    fn positive_closed_inert_graph_has_three_independent_package_pins() {
        let stage = InertStage::new();
        let approval = approved();
        stage.leaf(&approval);
        let graph = stage
            .selected(&approval)
            .expect("three independently pinned providers");
        graph.recheck().expect("unchanged G2 selection");
        let assets = graph
            .verified_resource_assets()
            .expect("same captured file index");
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].path, Path::new("index.html"));
        let _code_dir = graph
            .clone_verified_code_root()
            .expect("owned nofollow code root");
        assert!(!stage.root.join("app-ran").exists());
        assert!(!stage.root.join("foreign-ran").exists());
    }

    #[test]
    fn installed_runtime_preload_selects_the_fixed_sdk_member() {
        let (stage, approval) = stage_with_preloads();
        stage.leaf(&approval);
        let graph = stage.selected(&approval).expect("three approved providers");
        let runtime = graph
            .installed_runtime_preload()
            .expect("canonical SDK wrapper selected");
        assert_eq!(
            runtime,
            stage.root.join("node_modules/@keld/api/runtime-preload.js")
        );
        assert_eq!(
            fs::read(runtime).expect("selected SDK bytes"),
            SDK_RUNTIME_PRELOAD.as_bytes()
        );
        assert_eq!(
            graph
                .installed_transport_preload()
                .expect("original canonical transport still selected"),
            stage.root.join("node_modules/@keld/kipc/transport.js")
        );
    }

    #[test]
    fn installed_runtime_preload_missing_row_cannot_use_transport_instead() {
        let stage = InertStage::new();
        InertStage::write(
            &stage.root,
            "node_modules/@keld/kipc/transport.js",
            TRANSPORT_PRELOAD.as_bytes(),
        );
        let mut approval = approved();
        approval.expected_package_trees[2] = digest(PIN_KIPC_WITH_TRANSPORT);
        stage.leaf(&approval);
        let graph = stage
            .selected(&approval)
            .expect("valid transport-only graph");
        graph
            .installed_transport_preload()
            .expect("transport is present and valid");
        let error = graph
            .installed_runtime_preload()
            .expect_err("fixed SDK wrapper row is required");
        assert!(
            error
                .to_string()
                .contains("canonical installed runtime preload is absent"),
            "{error}"
        );
    }

    #[test]
    fn installed_runtime_preload_and_transport_check_their_own_captured_bytes() {
        let (stage, approval) = stage_with_preloads();
        stage.leaf(&approval);
        let graph = stage.selected(&approval).expect("approved graph");
        let mut changed = SDK_RUNTIME_PRELOAD.as_bytes().to_vec();
        changed[0] ^= 1;
        InertStage::write(
            &stage.root,
            "node_modules/@keld/api/runtime-preload.js",
            &changed,
        );
        let error = graph
            .installed_runtime_preload()
            .expect_err("same-sized changed SDK wrapper must refuse");
        assert!(
            error
                .to_string()
                .contains("canonical installed runtime preload changed"),
            "{error}"
        );
        graph
            .installed_transport_preload()
            .expect("distinct captured transport remains valid");

        let (independent, approval) = stage_with_preloads();
        independent.leaf(&approval);
        let graph = independent.selected(&approval).expect("approved graph");
        let mut changed = TRANSPORT_PRELOAD.as_bytes().to_vec();
        changed[0] ^= 1;
        InertStage::write(
            &independent.root,
            "node_modules/@keld/kipc/transport.js",
            &changed,
        );
        let error = graph
            .installed_transport_preload()
            .expect_err("same-sized changed transport must still refuse");
        assert!(
            error
                .to_string()
                .contains("canonical installed transport changed"),
            "{error}"
        );
        graph
            .installed_runtime_preload()
            .expect("distinct captured SDK wrapper remains valid");
    }

    #[test]
    fn installed_runtime_preload_refuses_substituted_stage_root() {
        let (stage, approval) = stage_with_preloads();
        stage.leaf(&approval);
        let graph = stage.selected(&approval).expect("approved graph");
        fs::rename(&stage.root, stage.owned.path().join("retained-stage"))
            .expect("move selected stage");
        fs::create_dir(&stage.root).expect("substituted root");
        let error = graph
            .installed_runtime_preload()
            .expect_err("retained graph cannot authorize a substituted root pathname");
        assert!(
            error.to_string().contains("root was substituted"),
            "{error}"
        );
    }

    #[test]
    fn installed_runtime_preload_refuses_a_symlink_to_identical_bytes() {
        let (stage, approval) = stage_with_preloads();
        stage.leaf(&approval);
        let graph = stage.selected(&approval).expect("approved graph");
        let outside = stage.owned.path().join("outside-runtime-preload.js");
        fs::write(&outside, SDK_RUNTIME_PRELOAD).expect("identical external bytes");
        let member = stage.root.join("node_modules/@keld/api/runtime-preload.js");
        fs::remove_file(&member).expect("remove canonical member");
        std::os::unix::fs::symlink(outside, member).expect("substituted symlink");
        let error = graph
            .installed_runtime_preload()
            .expect_err("nofollow member selection must reject even equal external bytes");
        assert!(error.to_string().contains("runtime preload"), "{error}");
    }

    #[test]
    fn verified_webapp_source_retains_custody_and_selected_stage_provenance() {
        let stage = InertStage::new();
        stage.leaf(&approved());
        let graph = stage.selected(&approved()).expect("admitted stage");
        let source = graph
            .verified_webapp_source()
            .expect("one verified source selection");
        drop(graph);
        let (code_root, assets, selected_stage_root) = source.into_parts();
        assert_eq!(selected_stage_root, stage.root);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].path, Path::new("index.html"));
        assert_eq!(assets[0].size, b"<html>inert</html>\n".len() as u64);
        assert_eq!(
            assets[0].sha256,
            <[u8; 32]>::from(Sha256::digest(b"<html>inert</html>\n"))
        );

        // Selection owns the original object, independently of both the graph
        // owner and a later replacement at the recorded provenance path.
        let retained = stage.owned.path().join("retained-stage");
        fs::rename(&stage.root, retained).expect("move selected stage");
        fs::create_dir(&stage.root).expect("replacement stage pathname");
        InertStage::write(
            &stage.root,
            "drawio/src/main/webapp/index.html",
            b"<html>replacement must not become authority</html>\n",
        );
        let mut index = super::super::open_relative_file(
            &code_root,
            Path::new("index.html"),
            "test retained source",
        )
        .expect("original descriptor remains owned");
        let mut original = String::new();
        index
            .read_to_string(&mut original)
            .expect("read through retained descriptor");
        assert_eq!(original, "<html>inert</html>\n");
        assert_eq!(selected_stage_root, stage.root);
    }

    /// Existing inert graph + exact verified policy; a component selection,
    /// never a Runtime/native/original-application qualification receipt.
    pub(in crate::app_session) fn original_quit_selection() -> (
        tempfile::TempDir,
        crate::app_session::original_webapp::OriginalLoadSelection,
    ) {
        const POLICY: &str = r#"{"profile":"legacy","windows":{"editor":{"web":{"csp":"original-editor-offline"}}}}"#;
        let stage = InertStage::new();
        stage.leaf(&approved());
        let graph = Arc::new(
            stage
                .selected(&approved())
                .expect("same admitted inert graph"),
        );
        let policy_path = stage.owned.path().join("original-quit-policy.jsonc");
        fs::write(&policy_path, POLICY).expect("same exact policy fixture");
        let verified = load_verified_manifest(
            fs::File::open(&policy_path).expect("retained source"),
            policy_path,
            digest("064b118963ba56f7c1ff05f6dc8eefa25dc3c4dc3ce955a207ab05c67341b03d"),
        )
        .expect("same verified policy owner");
        let policy = verified
            .resolve_unique_window_csp(WindowCspProfile::OriginalEditorOffline)
            .expect("one key");
        let prepared = PreparedOriginalSource::prepare(
            graph,
            ProfileState::Legacy,
            policy,
            AppPreloadBudget::default(),
        )
        .expect("actual typed source preparation");
        let selected = prepared.with_runtime_versions(std::collections::BTreeMap::new());
        (stage.owned, selected)
    }

    #[test]
    fn original_resource_facts_bind_same_prepared_stage_and_refuse_foreign_stage() {
        const POLICY: &str = r#"{"profile":"legacy","windows":{"editor":{"web":{"csp":"original-editor-offline"}}}}"#;
        // Independently pinned exact fixture bytes, not a digest of a mutable
        // stage/manifest used as its own authority.
        const POLICY_SHA: &str = "064b118963ba56f7c1ff05f6dc8eefa25dc3c4dc3ce955a207ab05c67341b03d";
        let stage_a = InertStage::new();
        let stage_b = InertStage::new();
        let leaf_a = stage_a.leaf(&approved());
        let leaf_b = stage_b.leaf(&approved());
        assert_eq!(
            leaf_a, leaf_b,
            "equal content does not identify its stage root"
        );
        assert_ne!(stage_a.root, stage_b.root);
        let graph_a = Arc::new(stage_a.selected(&approved()).expect("actual graph A"));
        let graph_b = Arc::new(stage_b.selected(&approved()).expect("actual graph B"));
        // This policy lives beside the stage, so its creation cannot mutate
        // either already captured graph census.
        let policy_path = stage_a.owned.path().join("original-policy.jsonc");
        fs::write(&policy_path, POLICY).expect("exact independently pinned policy fixture");
        let verified = load_verified_manifest(
            fs::File::open(&policy_path).expect("retain actual policy file"),
            policy_path,
            digest(POLICY_SHA),
        )
        .expect("actual verified manifest, no raw policy constructor");
        let policy = verified
            .resolve_unique_window_csp(WindowCspProfile::OriginalEditorOffline)
            .expect("one verified editor key");
        let budget = AppPreloadBudget::default();
        let prepared_a = PreparedOriginalSource::prepare(
            graph_a,
            ProfileState::Legacy,
            policy.clone(),
            budget.clone(),
        )
        .expect("actual successful source preparation for A");
        let prepared_b =
            PreparedOriginalSource::prepare(graph_b, ProfileState::Legacy, policy, budget)
                .expect("actual successful source preparation for B");
        let mut facts_a = DevelopmentLaunchFacts::from_validated_development(
            "graph-test",
            &stage_a.root,
            Some("0.0.1"),
            true,
        )
        .expect("actual validated launch facts A")
        .expect("development facts carrier");
        let before = facts_a
            .private_argument()
            .expect("unpublished schema4 facts");
        let error = facts_a
            .set_original_resource_policy(&prepared_b)
            .expect_err("prepared B cannot publish a descriptor for facts A");
        assert_eq!(error.code(), "KELD-CORE-037");
        let detail = error.to_string();
        assert!(
            detail.contains("belongs to another selected stage"),
            "{detail}"
        );
        assert_eq!(
            facts_a
                .private_argument()
                .expect("unchanged facts after refusal"),
            before
        );
        let unpublished: serde_json::Value =
            serde_json::from_str(&before).expect("original facts JSON");
        assert_eq!(unpublished["schema"], 4);
        assert!(unpublished.get("originalResourcePolicy").is_none());

        facts_a
            .set_original_resource_policy(&prepared_a)
            .expect("same prepared stage must publish, not blanket refuse");
        let published = facts_a
            .private_argument()
            .expect("bounded schema5 publication");
        let payload: serde_json::Value =
            serde_json::from_str(&published).expect("published facts JSON");
        assert_eq!(payload["schema"], 5);
        assert_eq!(
            payload["root"],
            stage_a.root.to_str().expect("UTF8 fixture root")
        );
        assert_eq!(
            payload["originalResourcePolicy"],
            serde_json::json!({
                "contract": "gh473-original-editor-offline/v1",
                "graphSha256": format!("sha256:{:x}", Sha256::digest(&leaf_a)),
                "policySha256": format!("sha256:{POLICY_SHA}"),
                "windowKey": "editor",
                "selector": "original-editor-offline",
            })
        );
        let decoded = DevelopmentLaunchFacts::from_private_argument(&published, &stage_a.root)
            .expect("same-source descriptor passes the current closed decoder");
        assert_eq!(
            decoded.private_argument().expect("exact retained metadata"),
            published
        );
    }

    #[test]
    fn changed_graph_leaf_refuses_verified_webapp_source() {
        let stage = InertStage::new();
        let mut leaf = stage.leaf(&approved());
        let graph = stage.selected(&approved()).expect("admitted stage");
        // The parsed document remains equivalent; exact selected bytes differ.
        leaf.push(b'\n');
        fs::write(stage.root.join(GRAPH_FILE), leaf).expect("changed graph bytes");
        let error = graph
            .verified_webapp_source()
            .expect_err("source requires its canonical graph recheck");
        assert!(
            error.to_string().contains("selected leaf changed"),
            "{error}"
        );
    }

    #[test]
    fn changed_any_captured_member_refuses_verified_webapp_source() {
        for (path, replacement) in [
            (
                "drawio/src/main/webapp/index.html",
                b"<html>evil!</html>\n".as_slice(),
            ),
            ("node_modules/@keld/api/package.json", b"{}\n".as_slice()),
        ] {
            let stage = InertStage::new();
            stage.leaf(&approved());
            let graph = stage.selected(&approved()).expect("admitted stage");
            InertStage::write(&stage.root, path, replacement);
            let error = graph
                .verified_webapp_source()
                .expect_err("source must recheck assets and the complete provider graph");
            assert!(
                error.to_string().contains("file digest")
                    || error.to_string().contains("file size"),
                "{path}: {error}"
            );
        }
    }

    #[test]
    fn substituted_stage_root_refuses_verified_webapp_source() {
        let stage = InertStage::new();
        stage.leaf(&approved());
        let graph = stage.selected(&approved()).expect("admitted stage");
        fs::rename(&stage.root, stage.owned.path().join("retained-stage"))
            .expect("move selected stage");
        fs::create_dir(&stage.root).expect("substituted root");
        let error = graph
            .verified_webapp_source()
            .expect_err("source must recheck the selected root identity");
        assert!(
            error.to_string().contains("root was substituted"),
            "{error}"
        );
    }

    #[test]
    fn closed_graph_without_index_cannot_produce_a_webapp_source() {
        let stage = InertStage::new();
        fs::rename(
            stage.root.join("drawio/src/main/webapp/index.html"),
            stage.root.join("drawio/src/main/webapp/other.html"),
        )
        .expect("closed fixture with no index");
        stage.leaf(&approved());
        let graph = stage.selected(&approved()).expect("otherwise valid graph");
        graph.recheck().expect("canonical graph remains valid");
        let error = graph
            .verified_webapp_source()
            .expect_err("native original source requires captured index.html");
        assert!(
            error
                .to_string()
                .contains("verified webapp index.html missing"),
            "{error}"
        );
    }

    #[test]
    fn counterfeit_each_of_three_provider_pins_independently_refuses() {
        let stage = InertStage::new();
        let approval = approved();
        stage.leaf(&approval);
        for index in 0..3 {
            let mut forged = approved();
            forged.expected_package_trees[index][0] ^= 0x55;
            let error = stage
                .selected(&forged)
                .expect_err("foreign provider subtree");
            assert!(
                error.to_string().contains("approved package artifact"),
                "provider index {index}: {error}"
            );
        }
    }

    #[test]
    fn nested_real_electron_refuses_even_when_graph_and_lock_are_self_consistent() {
        let stage = InertStage::new();
        InertStage::write(
            &stage.root,
            "node_modules/dependency/package.json",
            b"{\"name\":\"dependency\",\"version\":\"1.0.0\"}\n",
        );
        InertStage::write(
            &stage.root,
            "node_modules/dependency/node_modules/electron/package.json",
            b"{\"name\":\"electron\",\"version\":\"44.5.1\"}\n",
        );
        let mut lock: serde_json::Value =
            serde_json::from_str(ROOT_LOCK).expect("original inert lock data");
        lock["packages"]["node_modules/dependency"] = serde_json::json!({"version": "1.0.0"});
        lock["packages"]["node_modules/dependency/node_modules/electron"] =
            serde_json::json!({"name":"electron","version":"44.5.1"});
        let bytes = serde_json::to_vec(&lock).expect("self-consistent nested lock");
        InertStage::write(&stage.root, "package-lock.json", &bytes);
        let mut now = approved();
        now.expected_lock_sha256 = Sha256::digest(&bytes).into();
        // The file rows, lock, and deliberately updated standalone lock digest
        // are internally consistent. The foreign nested package remains denied.
        let err = make_graph_leaf(&stage.root, "src/main/electron.js", &now)
            .expect_err("GH499 source stage must refuse before any code evaluation");
        assert!(
            err.to_string().contains("electron"),
            "must refuse actual nested package identity, not a missing module: {err}"
        );
        assert!(!stage.root.join("foreign-ran").exists());
    }

    #[test]
    fn changed_bound_lock_and_unlisted_runtime_file_are_separate_negatives() {
        let stage = InertStage::new();
        let approval = approved();
        stage.leaf(&approval);
        InertStage::write(&stage.root, "package-lock.json", b"{}\n");
        let lock_error = stage.selected(&approval).expect_err("changed lock bytes");
        assert!(lock_error.to_string().contains("lock"), "{lock_error}");

        let independent = InertStage::new();
        independent.leaf(&approval);
        InertStage::write(
            &independent.root,
            "node_modules/electron/unlisted.js",
            b"console.log('foreign code that must not execute');\n",
        );
        let error = independent
            .selected(&approval)
            .expect_err("extra unbound installed JS");
        assert!(error.to_string().contains("unlisted"), "{error}");
    }

    #[test]
    fn changed_webapp_after_g1_refuses_before_g2() {
        let stage = InertStage::new();
        let approval = approved();
        stage.leaf(&approval);
        let selected = stage
            .selected(&approval)
            .expect("original selected G1 graph");
        InertStage::write(
            &stage.root,
            "drawio/src/main/webapp/index.html",
            b"<html>changed before G2</html>\n",
        );
        let err = selected
            .recheck()
            .expect_err("G2 must refuse changed captured asset");
        assert!(
            err.to_string().contains("file size") || err.to_string().contains("file digest"),
            "{err}"
        );
    }

    #[test]
    fn graph_leaf_size_bound_uses_8_mib_and_no_candidate() {
        let stage = InertStage::new();
        let huge = vec![b' '; 8 * 1024 * 1024 + 1];
        let err =
            admit_stage_graph(&stage.root, &huge, &approved()).expect_err("8 MiB exact leaf limit");
        assert!(err.to_string().contains("8 MiB"), "{err}");
        assert!(!stage.root.join("app-ran").exists());
    }

    #[test]
    fn root_package_import_redirect_is_refused_before_source_body() {
        let stage = InertStage::new();
        InertStage::write(
            &stage.root, "package.json",
            b"{\"name\":\"graph-test\",\"version\":\"0.0.1\",\"imports\":{\"#foreign\":\"/tmp/foreign.js\"},\"dependencies\":{\"electron\":\"npm:@keld/electron@0.0.1\",\"@keld/api\":\"0.0.1\",\"@keld/kipc\":\"0.0.1\"}}\n",
        );
        let mut now = approved();
        now.expected_package_sha256 =
            Sha256::digest(fs::read(stage.root.join("package.json")).expect("new package")).into();
        let err = make_graph_leaf(&stage.root, "src/main/electron.js", &now)
            .expect_err("unapproved imports map must refuse before JS");
        assert!(err.to_string().contains("imports"), "{err}");
    }

    #[test]
    fn bound_graph_entry_must_equal_host_and_guardian_selected_entry() {
        let stage = InertStage::new();
        InertStage::write(
            &stage.root,
            "src/main/other.js",
            b"// inert alternate entry\n",
        );
        let leaf = stage.leaf(&approved());
        let hash: [u8; 32] = Sha256::digest(&leaf).into();
        read_bound_graph_leaf(&stage.root, hash, Path::new("src/main/electron.js"))
            .expect("the exact selected entry matches");
        let err = read_bound_graph_leaf(&stage.root, hash, Path::new("src/main/other.js"))
            .expect_err("boot entry cannot select another captured code file");
        assert!(err.to_string().contains("entry"), "{err}");
        assert!(!stage.root.join("app-ran").exists());
    }

    #[test]
    fn empty_unlisted_directory_added_between_g1_and_g2_is_not_closed() {
        let stage = InertStage::new();
        stage.leaf(&approved());
        let bound = stage.selected(&approved()).expect("valid first generation");
        fs::create_dir(stage.root.join("unlisted-empty")).expect("simulated late empty directory");
        let err = bound
            .recheck()
            .expect_err("second generation must reject extra directory");
        assert!(err.to_string().contains("directory"), "{err}");
    }

    #[test]
    fn nested_app_package_scope_cannot_introduce_import_redirect() {
        let stage = InertStage::new();
        InertStage::write(
            &stage.root,
            "src/main/package.json",
            br##"{"imports":{"#helper":"./foreign.js"}}"##,
        );
        InertStage::write(&stage.root, "src/main/foreign.js", b"// never run\n");
        let err = make_graph_leaf(&stage.root, "src/main/electron.js", &approved())
            .expect_err("every effective package scope must be closed");
        assert!(err.to_string().contains("imports"), "{err}");
    }

    #[test]
    fn nested_counterfeit_api_or_kipc_package_cannot_override_independent_pins() {
        for provider in ["@keld/api", "@keld/kipc"] {
            let stage = InertStage::new();
            InertStage::write(
                &stage.root,
                "node_modules/consumer/package.json",
                br#"{"name":"consumer","version":"1.0.0"}"#,
            );
            let nested = format!("node_modules/consumer/node_modules/{provider}");
            InertStage::write(
                &stage.root,
                &format!("{nested}/package.json"),
                format!("{{\"name\":\"{provider}\",\"version\":\"0.0.1\"}}\n").as_bytes(),
            );
            InertStage::write(
                &stage.root,
                &format!("{nested}/index.js"),
                b"// counterfeit closer provider must not be evaluated\n",
            );
            let mut lock: serde_json::Value =
                serde_json::from_str(ROOT_LOCK).expect("inert fixture lock");
            lock["packages"]["node_modules/consumer"] =
                serde_json::json!({"name":"consumer","version":"1.0.0"});
            lock["packages"][nested.as_str()] =
                serde_json::json!({"name":provider,"version":"0.0.1"});
            let lock_bytes = serde_json::to_vec(&lock).expect("self-consistent test lock");
            InertStage::write(&stage.root, "package-lock.json", &lock_bytes);
            let mut approval = approved();
            approval.expected_lock_sha256 = Sha256::digest(&lock_bytes).into();
            let err = make_graph_leaf(&stage.root, "src/main/electron.js", &approval)
                .expect_err("nested counterfeit cannot shadow independently pinned provider");
            assert!(err.to_string().contains("provider"), "{provider}: {err}");
            assert!(!stage.root.join("app-ran").exists());
        }
    }

    #[test]
    fn duplicate_json_keys_are_refused_even_when_external_sha_matches() {
        let stage = InertStage::new();
        let duplicate_package = ROOT_PACKAGE.replacen(
            "\"dependencies\":",
            "\"dependencies\":{\"electron\":\"electron\"},\"dependencies\":",
            1,
        );
        InertStage::write(&stage.root, "package.json", duplicate_package.as_bytes());
        let mut approval = approved();
        approval.expected_package_sha256 = Sha256::digest(duplicate_package.as_bytes()).into();
        let err = make_graph_leaf(&stage.root, "src/main/electron.js", &approval)
            .expect_err("duplicate root dependencies must not disappear inside Value");
        assert!(err.to_string().contains("duplicate"), "{err}");

        let independent = InertStage::new();
        let duplicate_lock =
            ROOT_LOCK.replacen("\"packages\":", "\"packages\":{},\"packages\":", 1);
        InertStage::write(
            &independent.root,
            "package-lock.json",
            duplicate_lock.as_bytes(),
        );
        let mut pinned = approved();
        pinned.expected_lock_sha256 = Sha256::digest(duplicate_lock.as_bytes()).into();
        let err = make_graph_leaf(&independent.root, "src/main/electron.js", &pinned)
            .expect_err("duplicate lock packages must not disappear inside Value");
        assert!(err.to_string().contains("duplicate"), "{err}");
    }

    // Inert test-only pin adjustment: production must load expected file bytes
    // from actual approved packed Electron/API/KIPC archive custody. This
    // helper never claims to qualify the app or an arbitrary installed tree.
    fn retarget_local_electron_archive(
        stage: &InertStage,
        alias: &str,
        resolved: &str,
    ) -> ApprovedGraphCaptureInput {
        let mut root: serde_json::Value =
            serde_json::from_str(ROOT_PACKAGE).expect("valid fixture package");
        root["dependencies"]["electron"] = alias.into();
        let package_bytes = serde_json::to_vec(&root).expect("fixture root package");
        InertStage::write(&stage.root, "package.json", &package_bytes);

        let mut lock: serde_json::Value =
            serde_json::from_str(ROOT_LOCK).expect("valid fixture lock");
        lock["packages"][""]["dependencies"]["electron"] = alias.into();
        lock["packages"]["node_modules/electron"]["resolved"] = resolved.into();
        lock["packages"]["node_modules/electron"]["integrity"] =
            format!("sha512-{}==", "A".repeat(86)).into();
        let lock_bytes = serde_json::to_vec(&lock).expect("fixture lock data");
        InertStage::write(&stage.root, "package-lock.json", &lock_bytes);

        let mut pinned = approved();
        pinned.expected_package_sha256 = Sha256::digest(&package_bytes).into();
        pinned.expected_lock_sha256 = Sha256::digest(&lock_bytes).into();
        pinned
    }

    #[test]
    fn independently_pinned_local_first_party_electron_archive_is_admitted() {
        let stage = InertStage::new();
        let selected = "file:/private/var/tmp/verified/keld-electron-0.0.1.tgz";
        let pinned = retarget_local_electron_archive(&stage, selected, selected);
        stage.leaf(&pinned);
        stage
            .selected(&pinned)
            .expect("approved real installed provider identity")
            .recheck()
            .expect("same verified data at G2");
        assert!(!stage.root.join("app-ran").exists());
    }

    #[test]
    fn unapproved_file_tarball_mappings_and_mismatched_lock_are_denied() {
        for local in [
            "file:/tmp/foreign-electron-0.0.1.tgz",
            "file:../unapproved/keld-electron-0.0.1.tgz",
            "file://foreign.host/keld-electron-0.0.1.tgz",
            "file:/tmp/keld-electron-0.0.1.tgz?unapproved=1",
            "file:/tmp/../keld-electron-0.0.1.tgz",
        ] {
            let stage = InertStage::new();
            let pinned = retarget_local_electron_archive(&stage, local, local);
            let err = make_graph_leaf(&stage.root, "src/main/electron.js", &pinned)
                .expect_err("unapproved local archive spec must fail closed");
            assert!(err.to_string().contains("electron alias"), "{local}: {err}");
        }

        let stage = InertStage::new();
        let declared = "file:/private/var/tmp/verified/keld-electron-0.0.1.tgz";
        let different = "file:/private/var/tmp/foreign/keld-electron-0.0.1.tgz";
        let pinned = retarget_local_electron_archive(&stage, declared, different);
        let err = make_graph_leaf(&stage.root, "src/main/electron.js", &pinned)
            .expect_err("locked resolved Electron archive must match approved root spec");
        assert!(err.to_string().contains("resolved"), "{err}");
    }
    fn local_updater(stage: &InertStage, alias: &str, resolved: &str) -> ApprovedGraphCaptureInput {
        let mut package: serde_json::Value =
            serde_json::from_str(ROOT_PACKAGE).expect("fixture root");
        package["dependencies"]["electron-updater"] = serde_json::json!(alias);
        let package_bytes = serde_json::to_vec(&package).expect("fixture root encode");
        InertStage::write(&stage.root, "package.json", &package_bytes);
        let mut lock: serde_json::Value = serde_json::from_str(ROOT_LOCK).expect("fixture lock");
        lock["packages"][""]["dependencies"]["electron-updater"] = serde_json::json!(alias);
        lock["packages"]["node_modules/electron-updater"] = serde_json::json!({
            "name": "@keld/electron-updater", "version": "0.0.1", "resolved": resolved,
            "integrity": format!("sha512-{}==", "A".repeat(86))
        });
        let lock_bytes = serde_json::to_vec(&lock).expect("fixture lock encode");
        InertStage::write(&stage.root, "package-lock.json", &lock_bytes);
        InertStage::write(
            &stage.root,
            "node_modules/electron-updater/package.json",
            br#"{"name":"@keld/electron-updater","version":"0.0.1"}"#,
        );
        InertStage::write(
            &stage.root,
            "node_modules/electron-updater/index.js",
            b"// inert adapter fixture\n",
        );
        let mut approval = approved();
        approval.expected_package_sha256 = Sha256::digest(&package_bytes).into();
        approval.expected_lock_sha256 = Sha256::digest(&lock_bytes).into();
        approval
    }

    #[test]
    fn local_updater_archive_uses_existing_three_provider_source_contract() {
        let stage = InertStage::new();
        let alias = "file:.keld/approved-artifacts/keld-electron-updater-0.0.1.tgz";
        let approval = local_updater(&stage, alias, alias);
        stage.leaf(&approval);
        let graph = stage
            .selected(&approval)
            .expect("canonical local updater admitted");
        graph.recheck().expect("unchanged selected updater");
        InertStage::write(
            &stage.root,
            "node_modules/electron-updater/index.js",
            b"// evil! adapter fixture\n",
        );
        let error = graph
            .recheck()
            .expect_err("post-selection updater byte change refuses");
        assert!(error.to_string().contains("file digest"), "{error}");
    }

    #[test]
    fn local_updater_rejects_foreign_spellings_and_mismatched_lock_resolution() {
        for alias in [
            "file:.keld/approved-artifacts/electron-updater-0.0.1.tgz",
            "file:.keld/approved-artifacts/keld-electron-updater-9.9.9.tgz",
            "file:../keld-electron-updater-0.0.1.tgz",
            "file://host/keld-electron-updater-0.0.1.tgz",
            "file:.keld/approved-artifacts/keld-electron-updater-0.0.1.tgz?x",
        ] {
            let stage = InertStage::new();
            let approval = local_updater(&stage, alias, alias);
            let error = make_graph_leaf(&stage.root, "src/main/electron.js", &approval)
                .expect_err("only canonical bounded archive spelling is accepted");
            assert!(error.to_string().contains("updater"), "{alias}: {error}");
        }
        let stage = InertStage::new();
        let approval = local_updater(
            &stage,
            "file:.keld/approved-artifacts/keld-electron-updater-0.0.1.tgz",
            "file:other/keld-electron-updater-0.0.1.tgz",
        );
        let error = make_graph_leaf(&stage.root, "src/main/electron.js", &approval)
            .expect_err("lock resolution must equal selected root archive");
        assert!(
            error.to_string().contains("updater resolved archive"),
            "{error}"
        );
    }

    #[test]
    fn local_updater_requires_sha512_receipt_and_canonical_installed_identity() {
        for (field, value) in [
            ("integrity", "sha512-bad"),
            ("name", "electron-updater"),
            ("version", "9.9.9"),
        ] {
            let stage = InertStage::new();
            let alias = "file:.keld/approved-artifacts/keld-electron-updater-0.0.1.tgz";
            let mut approval = local_updater(&stage, alias, alias);
            if field == "integrity" {
                let mut lock: serde_json::Value = serde_json::from_slice(
                    &fs::read(stage.root.join("package-lock.json")).expect("lock"),
                )
                .expect("JSON");
                lock["packages"]["node_modules/electron-updater"][field] = serde_json::json!(value);
                let bytes = serde_json::to_vec(&lock).expect("lock bytes");
                InertStage::write(&stage.root, "package-lock.json", &bytes);
                approval.expected_lock_sha256 = Sha256::digest(&bytes).into();
            } else {
                let mut package =
                    serde_json::json!({"name":"@keld/electron-updater","version":"0.0.1"});
                package[field] = serde_json::json!(value);
                InertStage::write(
                    &stage.root,
                    "node_modules/electron-updater/package.json",
                    &serde_json::to_vec(&package).expect("manifest bytes"),
                );
            }
            make_graph_leaf(&stage.root, "src/main/electron.js", &approval)
                .expect_err("updater receipt/installed identity mismatch must refuse");
        }
    }

    #[test]
    fn harmless_build_extends_and_unrelated_paths_remain_bound_graph_data() {
        let stage = InertStage::new();
        // Same relative build-only inheritance shape as the actual failing
        // vendor data, without a package/path exception or executed source.
        let configs = [
            (
                "vendor/library/src/packages/fonts/scripts/tsconfig.json",
                r#"{"extends":"../tsconfig.json","compilerOptions":{"rootDir":null,"outDir":null,"incremental":false}}"#,
            ),
            (
                "vendor/library/src/packages/fonts/tsconfig.json",
                r#"{"extends":"../../../tsconfig.json","compilerOptions":{"outDir":"dist"}}"#,
            ),
            (
                "tsconfig.json",
                r#"{"extends":"./config/build-base.json","compilerOptions":{"paths":{"@application/*":["src/*"],"electronish*":["helpers/*"],"@keld/apiculture/*":["helpers/*"]}}}"#,
            ),
            (
                "config/build-base.json",
                r#"{"compilerOptions":{"strict":true,"paths":{"utils/*":["helpers/*"]}}}"#,
            ),
        ];
        for (path, bytes) in configs {
            InertStage::write(&stage.root, path, bytes.as_bytes());
        }
        let approval = approved();
        stage.leaf(&approval);
        let graph = stage
            .selected(&approval)
            .expect("harmless config remains admitted");
        graph
            .recheck()
            .expect("same unchanged config at next generation");
        for (path, bytes) in configs {
            assert_eq!(
                fs::read(stage.root.join(path)).expect("config is retained"),
                bytes.as_bytes()
            );
        }
        InertStage::write(
            &stage.root,
            "config/build-base.json",
            b"{\"compilerOptions\":{\"strict\":false}}\n",
        );
        let error = graph
            .recheck()
            .expect_err("later config mutation cannot reuse admission");
        assert!(
            error.to_string().contains("file size") || error.to_string().contains("file digest"),
            "{error}"
        );
        assert!(!stage.root.join("app-ran").exists());
    }

    #[test]
    fn exact_subpath_and_broad_reserved_config_keys_refuse_before_evaluation() {
        for (configuration, pattern) in [
            ("tsconfig.json", "electron"),
            ("tsconfig.json", "electron/main"),
            ("tsconfig.json", "electron/renderer"),
            ("tsconfig.json", "electron/common"),
            ("tsconfig.json", "electron/utility"),
            ("tsconfig.json", "electron/internal/original-ipc"),
            ("tsconfig.json", "electron/internal/*"),
            ("tsconfig.json", "@keld/api"),
            ("tsconfig.json", "@keld/api/runtime-preload"),
            ("tsconfig.json", "@keld/kipc/transport"),
            ("tsconfig.json", "@keld/*"),
            ("tsconfig.json", "electron/*"),
            ("tsconfig.json", "*"),
            ("tsconfig.json", "elect*"),
            ("tsconfig.json", "*tron/main"),
            ("jsconfig.json", "electron"),
            ("src/main/tsconfig.json", "electron/*"),
            ("node_modules/consumer/tsconfig.json", "electron/common"),
        ] {
            let stage = InertStage::new();
            let config =
                serde_json::json!({"compilerOptions":{"paths":{pattern:["./foreign.js"]}}});
            InertStage::write(
                &stage.root,
                configuration,
                &serde_json::to_vec(&config).expect("authored config data"),
            );
            InertStage::write(
                &stage.root,
                "foreign.js",
                b"throw new Error('foreign must never evaluate');\n",
            );
            let error = make_graph_leaf(&stage.root, "src/main/electron.js", &approved())
                .expect_err("reserved-name config redirect must refuse");
            assert!(
                error.to_string().contains("remapping"),
                "{configuration} {pattern}: {error}"
            );
            assert!(!stage.root.join("app-ran").exists());
        }
    }

    #[test]
    fn inherited_named_json_reserved_mapping_is_checked_before_evaluation() {
        let stage = InertStage::new();
        InertStage::write(
            &stage.root,
            "tsconfig.json",
            br#"{"extends":"./config/base.json"}"#,
        );
        InertStage::write(
            &stage.root,
            "config/base.json",
            br#"{"compilerOptions":{"paths":{"electron/*":["../foreign.js"]}}}"#,
        );
        InertStage::write(&stage.root, "foreign.js", b"// never evaluated\n");
        let error = make_graph_leaf(&stage.root, "src/main/electron.js", &approved())
            .expect_err("actual inherited configuration is also inspected");
        assert!(
            error.to_string().contains("config/base.json")
                && error.to_string().contains("remapping"),
            "{error}"
        );
    }

    #[test]
    fn escaping_unknown_or_cyclic_config_inheritance_remains_unqualified() {
        for reference in ["../outside.json", "@foreign/config", "./cycle.json"] {
            let stage = InertStage::new();
            InertStage::write(
                &stage.root,
                "tsconfig.json",
                &serde_json::to_vec(&serde_json::json!({"extends":reference}))
                    .expect("inert config"),
            );
            if reference == "./cycle.json" {
                InertStage::write(
                    &stage.root,
                    "cycle.json",
                    br#"{"extends":"./tsconfig.json"}"#,
                );
            }
            let error = make_graph_leaf(&stage.root, "src/main/electron.js", &approved())
                .expect_err("unqualified config source is never followed or executed");
            assert!(
                error.to_string().contains("configuration inheritance"),
                "{reference}: {error}"
            );
            assert!(!stage.root.join("app-ran").exists());
        }
    }

    #[test]
    fn base_url_without_paths_is_not_mistaken_for_proved_package_resolution() {
        let stage = InertStage::new();
        InertStage::write(
            &stage.root,
            "tsconfig.json",
            br#"{"compilerOptions":{"baseUrl":"./redirect"}}"#,
        );
        InertStage::write(
            &stage.root,
            "redirect/electron.js",
            b"// authored foreign sentinel, never evaluated\n",
        );
        let error = make_graph_leaf(&stage.root, "src/main/electron.js", &approved())
            .expect_err("baseUrl can bypass the package without a paths key");
        assert!(error.to_string().contains("baseUrl"), "{error}");
        assert!(!stage.root.join("app-ran").exists());
    }

    #[test]
    fn config_inheritance_depth_fanout_and_duplicate_keys_are_bounded() {
        let stage = InertStage::new();
        InertStage::write(
            &stage.root,
            "tsconfig.json",
            br#"{"extends":"./configs/level0.json"}"#,
        );
        for index in 0..65 {
            let config = serde_json::json!({"extends":format!("./level{}.json", index + 1)});
            InertStage::write(
                &stage.root,
                &format!("configs/level{index}.json"),
                &serde_json::to_vec(&config).expect("bounded chain fixture"),
            );
        }
        let error = make_graph_leaf(&stage.root, "src/main/electron.js", &approved())
            .expect_err("unbounded inherited config depth must refuse");
        assert!(error.to_string().contains("depth"), "{error}");
        let stage = InertStage::new();
        let references: Vec<_> = (0..65).map(|index| format!("./base{index}.json")).collect();
        InertStage::write(
            &stage.root,
            "tsconfig.json",
            &serde_json::to_vec(&serde_json::json!({"extends":references}))
                .expect("bounded references fixture"),
        );
        let error = make_graph_leaf(&stage.root, "src/main/electron.js", &approved())
            .expect_err("inherited config fanout must be bounded");
        assert!(error.to_string().contains("64"), "{error}");
        let stage = InertStage::new();
        InertStage::write(
            &stage.root,
            "tsconfig.json",
            br#"{"compilerOptions":{"paths":{"electron":["./foreign.js"]},"paths":{}}}"#,
        );
        let error = make_graph_leaf(&stage.root, "src/main/electron.js", &approved())
            .expect_err("duplicate config keys cannot erase a reserved mapping");
        assert!(error.to_string().contains("duplicate"), "{error}");
    }

    #[test]
    fn inherited_reference_uses_physical_absence_not_only_census_spelling() {
        let stage = InertStage::new();
        InertStage::write(
            &stage.root,
            "tsconfig.json",
            br#"{"extends":"./configs/base.json"}"#,
        );
        InertStage::write(
            &stage.root,
            "configs/Base.json",
            br#"{"compilerOptions":{"paths":{"electron":["./foreign.js"]}}}"#,
        );
        // The independent filesystem observation selects the applicable oracle:
        // on a case-sensitive volume the differently spelled input is truly
        // absent; on a case-insensitive volume it is reachable and must refuse.
        let physically_present = stage
            .root
            .join("configs/base.json")
            .try_exists()
            .expect("observe actual config lookup spelling");
        let admitted = make_graph_leaf(&stage.root, "src/main/electron.js", &approved());
        if physically_present {
            let error = admitted.expect_err(
                "physical inherited input cannot be omitted from exact captured spelling",
            );
            assert!(
                error.to_string().contains("exact captured spelling"),
                "{error}"
            );
        } else {
            admitted.expect("genuinely absent relative build input has no mapping effect");
        }
        eprintln!("config case-alias physically present: {physically_present}");
    }

    fn original_preload_fixture_bytes() -> Vec<u8> {
        let bytes = serde_json::from_str::<String>(include_str!(
            "../../../../keld-wv/tests/fixtures/drawio-original-preload.json"
        ))
        .expect("existing original preload fixture as data only")
        .into_bytes();
        assert_eq!(bytes.len(), 3_201);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            keld_wv::wkwebview::ORIGINAL_DRAWIO_PRELOAD_SHA256
        );
        bytes
    }

    #[test]
    fn original_preload_selector_accepts_the_approved_desktop_member() {
        let stage = InertStage::new();
        InertStage::write(
            &stage.root,
            "src/main/electron-preload.js",
            &original_preload_fixture_bytes(),
        );
        let leaf = stage.leaf(&approved());
        let captured: serde_json::Value =
            serde_json::from_slice(&leaf).expect("actual captured layout");
        assert_eq!(captured["entry"], "src/main/electron.js");
        let graph = stage
            .selected(&approved())
            .expect("same admitted source graph");
        let budget = AppPreloadBudget::default();
        let arguments = vec![String::from("--initial-adaptive-colors=simple")];
        let versions: std::collections::BTreeMap<String, String> = [(
            String::from("keld"),
            String::from(env!("CARGO_PKG_VERSION")),
        )]
        .into();
        let image = graph
            .prepare_original_preload(
                &stage.root.join("src/main/electron-preload.js"),
                arguments.clone(),
                versions.clone(),
                &budget,
            )
            .expect("approved desktop preload must not be confused with webapp resources");
        assert_eq!(image.source_bytes(), 3_201);
        assert_eq!(image.additional_arguments(), arguments.as_slice());
        assert_eq!(image.versions(), &versions);
        assert_eq!(budget.retained_bytes(), Some(3_201));
        drop(image);
        assert_eq!(budget.retained_bytes(), Some(0));
        assert!(
            !stage.root.join("app-ran").exists(),
            "no preload or app script evaluated by this component test"
        );
    }

    #[test]
    fn original_preload_selector_refuses_old_webapp_layout_and_path_aliases() {
        let stage = InertStage::new();
        let bytes = original_preload_fixture_bytes();
        for path in [
            "src/main/electron-preload.js",
            "drawio/src/main/electron-preload.js",
            "src/main/electron-preload-copy.js",
        ] {
            InertStage::write(&stage.root, path, &bytes);
        }
        stage.leaf(&approved());
        let graph = stage
            .selected(&approved())
            .expect("same pinned bytes in independently captured members");
        let budget = AppPreloadBudget::default();
        for selector in [
            stage.root.join("drawio/src/main/electron-preload.js"),
            stage.root.join("src/main/electron-preload-copy.js"),
            stage.root.join("src/main/../main/electron-preload.js"),
            PathBuf::from("src/main/electron-preload.js"),
        ] {
            let error = graph
                .prepare_original_preload(
                    &selector,
                    vec![],
                    [(String::from("keld"), String::from("fixture-facts"))].into(),
                    &budget,
                )
                .expect_err("identical bytes do not authorize a different selector");
            assert!(
                error.to_string().contains("preload selector"),
                "{}: {error}",
                selector.display()
            );
            assert_eq!(budget.retained_bytes(), Some(0));
        }
    }
}
