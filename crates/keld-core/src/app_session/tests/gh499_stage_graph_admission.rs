// GH499 test-first boundary: original draw.io/Electron are never evaluated.
// The original *reference* dependency-data cache is not used by these tests.
#[cfg(target_os = "macos")]
mod gh499_graph {
    use std::fs;
    use std::path::{Path, PathBuf};

    use sha2::{Digest as _, Sha256};

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
        _owned: tempfile::TempDir,
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
            Self {
                _owned: owned,
                root,
            }
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
}
