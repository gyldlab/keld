//! Real Windows filesystem tests for ordinary producer selection.
//! Fixed inert GH499 receipts are test data, never shipping qualification.
#![allow(clippy::expect_used, clippy::panic)]
use super::*;
use std::fs;
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

const PROVIDERS: [&str; 3] = [
    "7e57f5030f61b455e6c7aecfd1554bdf1334cf9c5a98024ea388bc729b95646f",
    "24cb3148684ef325d0f0488f677f7d79c01be1ae881ab72cd758da75028fcec1",
    "e8893aeac32cb4828dce4360dd0fb432b51bf4d403a3f6b3e03b787017ff7e01",
];
const PRELOAD: &[u8] = b"/* pinned inert original quit preload; never evaluated */\n";
const APP: &str = "export default {name:'Producer Fixture',id:'dev.example.producer',entry:'main/app.mjs',initialWindow:'none',rendererSource:{root:'client/assets',entry:'nested/editor.html'}} as const;";
const HOST: &[u8] = b"KELD_CLI_INERT_HOST_FIXTURE\n";
const POLICY: &[u8] = b"{\"profile\":\"legacy\"}\n";

fn pin(text: &str) -> [u8; 32] {
    let mut result = [0_u8; 32];
    for (index, pair) in text.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        result[index] = u8::from_str_radix(std::str::from_utf8(pair).expect("literal ASCII"), 16)
            .expect("fixed independent fixture hash");
    }
    result
}
fn qualified() -> QualifiedWindowsFramework {
    QualifiedWindowsFramework {
        target: target(),
        host_sha256: pin("2e2e90fa8b8e012ca937ef11324dc0ac80e00cf068eeaa2f8dd1b402d8034b60"),
        provider_trees: PROVIDERS.map(pin),
    }
}
fn build() -> String {
    format!(
        "export default {{graphSource:{{packageSha256:'36f0819d6ec665ae79fc6fd13256fed97a93c6497b05c8e5e27f51c31c163dee',lockSha256:'fa43830d726d04578607c5d9566be4198718db3bbe914b73091c288617c3476b',providerTrees:{{electron:'{}',api:'{}',kipc:'{}'}},preload:{{path:'bootstrap/native-preload.cjs',sha256:'e51d29ee9a7965876fabf5bcfe68774e3bf8061334253a02e4c9648efafd390a'}}}}}} as const;",
        PROVIDERS[0], PROVIDERS[1], PROVIDERS[2],
    )
}
fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
    fs::write(path, bytes).expect("exact fixture source");
}
struct Fixture {
    owned: tempfile::TempDir,
    project: PathBuf,
    host: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let owned = tempfile::tempdir().expect("private fixture namespace");
        let project = owned.path().join("project");
        let host = owned.path().join("keld-host.exe");
        fs::create_dir(&project).expect("fixture project");
        fs::write(&host, HOST).expect("inert adjacent Host");
        for (relative, bytes) in [
            (
                "main/app.mjs",
                b"// inert original main; never run\n".as_slice(),
            ),
            (
                "client/assets/nested/editor.html",
                b"<html>selected non-default entry</html>\n",
            ),
            ("client/assets/styles.css", b"body { margin: 0; }\n"),
            ("bootstrap/native-preload.cjs", PRELOAD),
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
            ("keld.config.ts", APP.as_bytes()),
            ("keld.permissions.jsonc", POLICY),
        ] {
            write(&project, relative, bytes);
        }
        write(&project, "keld.build.ts", build().as_bytes());
        Self {
            owned,
            project,
            host,
        }
    }
    fn stage(&self) -> Result<DevBootStage, BootCompileError> {
        stage_with_qualification(&self.project, &self.host, Some(&qualified()))
    }
}

#[test]
fn absent_production_qualification_refuses_before_stage() {
    let f = Fixture::new();
    let error = stage_for_project(&f.project, &f.host).expect_err("no shipping qualification");
    assert!(
        error
            .to_string()
            .contains("no independently qualified Windows Host/provider tuple"),
        "{error}"
    );
    assert!(
        !f.project.join(".keld").exists(),
        "refusal must precede stage creation"
    );
}
#[test]
fn qualified_fixture_copies_non_default_source_and_emits_closed_boot8() {
    let f = Fixture::new();
    write(&f.project, ".git/inert-history", b"not source");
    let stage = f.stage().expect("internal fixture qualification only");
    let boot_bytes = fs::read(stage.root().join("keld.boot.json")).expect("published boot");
    let boot: serde_json::Value = serde_json::from_slice(&boot_bytes).expect("closed descriptor");
    assert_eq!(boot.as_object().expect("root").len(), 7);
    assert_eq!(boot["schema"], 8);
    assert_eq!(boot["entry"], "main/app.mjs");
    assert_eq!(
        boot["rendererSource"],
        serde_json::json!({"root":"client/assets","entry":"nested/editor.html"})
    );
    assert_eq!(
        boot["permissions"]["content_sha256"],
        "sha256:c8e2fb1f67314067c0dd111708809338215a701f457b94aaca686d46e1cf7a63"
    );
    assert_eq!(
        fs::read(stage.root().join("bootstrap/native-preload.cjs")).expect("copied preload"),
        PRELOAD
    );
    assert_eq!(
        fs::read(stage.root().join("client/assets/styles.css")).expect("copied asset"),
        b"body { margin: 0; }\n"
    );
    for absent in [".git", "keld.config.ts", "keld.build.ts"] {
        assert!(
            !stage.root().join(absent).exists(),
            "private authoring/history was copied"
        );
    }
    let graph =
        fs::read(stage.root().join(keld_core::app_session::GRAPH_FILE)).expect("published graph");
    let root = open_windows_launch_guard(stage.root()).expect("retained stage");
    let selected = SelectedRendererSource::new(
        "client/assets",
        "nested/editor.html",
        "bootstrap/native-preload.cjs",
        pin("e51d29ee9a7965876fabf5bcfe68774e3bf8061334253a02e4c9648efafd390a"),
    )
    .expect("fixture selection");
    keld_core::app_session::read_bound_graph_leaf_from_retained_root(
        stage.root(),
        &root,
        Sha256::digest(&graph).into(),
        "main/app.mjs",
        &selected,
    )
    .expect("actual Core consumer accepts captured graph");
    assert!(
        fs::rename(stage.root(), stage.root().with_extension("swapped")).is_err(),
        "namespace remains pinned until Host/sentinel handoff"
    );
    drop(root);
    drop(stage);
}
#[test]
fn project_cannot_self_approve_changed_provider_and_matching_project_pin() {
    let f = Fixture::new();
    write(
        &f.project,
        "node_modules/electron/dist/index.js",
        b"export const tampered = true;\n",
    );
    let changed = build().replace(
        PROVIDERS[0],
        "73edb1022ad981616481848eee914fddf39554f3a484f42d8af3aee63c6d71c8",
    );
    write(&f.project, "keld.build.ts", changed.as_bytes());
    let error = f
        .stage()
        .expect_err("self-consistent project pin is not distribution qualification");
    assert!(
        error.to_string().contains("compiled qualification tuple"),
        "{error}"
    );
    assert!(!f.project.join(".keld").exists());
}
#[test]
fn actual_provider_and_host_bytes_are_checked_before_stage() {
    for member in ["node_modules/electron/dist/index.js", "HOST"] {
        let f = Fixture::new();
        if member == "HOST" {
            fs::write(&f.host, b"changed Host").expect("foreign Host");
        } else {
            write(&f.project, member, b"changed provider");
        }
        let error = f
            .stage()
            .expect_err("actual bytes must match independent tuple");
        assert!(
            error.to_string().contains(if member == "HOST" {
                "adjacent Host bytes"
            } else {
                "provider"
            }),
            "{error}"
        );
        assert!(
            !f.project.join(".keld").exists(),
            "failed preflight staged a launch"
        );
    }
}
#[test]
fn closed_preparation_profile_and_unavailable_metadata_fail_before_stage() {
    for (name, bytes) in [
        (
            "keld.config.ts",
            APP.replace(
                "initialWindow:'none',",
                "initialWindow:'none',version:'1.2.3',",
            )
            .into_bytes(),
        ),
        (
            "keld.config.ts",
            APP.replace(
                "initialWindow:'none',",
                "initialWindow:'none',singleInstance:{id:'dev.example.producer'},",
            )
            .into_bytes(),
        ),
        (
            "keld.config.ts",
            APP.replace("rendererSource:", "['rendererSource']:")
                .into_bytes(),
        ),
        (
            "keld.build.ts",
            build()
                .replace("graphSource:", "target:'windows',graphSource:")
                .into_bytes(),
        ),
        (
            "keld.build.ts",
            build()
                .replace("graphSource:{", "graphSource:{unknown:true,")
                .into_bytes(),
        ),
        (
            "keld.build.ts",
            build()
                .replace("preload:{", "preload:null,preload:{")
                .into_bytes(),
        ),
        ("keld.permissions.jsonc", b"{}".to_vec()),
        (
            "keld.permissions.jsonc",
            b"{\"profile\":\"strict\"}".to_vec(),
        ),
    ] {
        let f = Fixture::new();
        write(&f.project, name, &bytes);
        assert!(
            f.stage().is_err(),
            "accepted malformed/unavailable preparation: {}",
            String::from_utf8_lossy(&bytes)
        );
        assert!(!f.project.join(".keld").exists());
    }
}
#[test]
fn source_census_detects_late_inserted_member_before_publication() {
    let f = Fixture::new();
    let held = open_windows_launch_guard(&f.project).expect("held project");
    let target = f.owned.path().join("capture");
    fs::create_dir(&target).expect("private capture fixture");
    let stage = open_windows_launch_guard(&target).expect("held destination");
    let trigger = f.project.join("bootstrap/native-preload.cjs");
    let mut visits = 0;
    let mut inserted = false;
    let error = super::super::graph_capture::capture_complete_tree(
        &f.project,
        &held,
        &target,
        &stage,
        &mut |path| {
            if path == trigger {
                visits += 1;
                if visits == 2 {
                    write(&f.project, "late-unscanned.txt", b"late source");
                    inserted = true;
                }
            }
            Ok(())
        },
    )
    .expect_err("census/copy/census must refuse membership drift");
    assert!(inserted, "mutation actually ran after initial census");
    assert!(
        error.to_string().contains("census") || error.to_string().contains("identity"),
        "{error}"
    );
    assert!(!target.join("keld.boot.json").exists());
}
#[test]
fn byte_identical_source_replacement_after_census_is_rejected() {
    let f = Fixture::new();
    let held = open_windows_launch_guard(&f.project).expect("held project");
    let target = f.owned.path().join("capture");
    fs::create_dir(&target).expect("private capture fixture");
    let stage = open_windows_launch_guard(&target).expect("held destination");
    let trigger = f.project.join("bootstrap/native-preload.cjs");
    let mut visits = 0;
    let mut swapped = false;
    let error = super::super::graph_capture::capture_complete_tree(
        &f.project,
        &held,
        &target,
        &stage,
        &mut |path| {
            if path == trigger {
                visits += 1;
                if visits == 2 {
                    fs::rename(path, f.owned.path().join("old-preload"))
                        .expect("swap original identity");
                    fs::write(path, PRELOAD).expect("byte-identical replacement");
                    swapped = true;
                }
            }
            Ok(())
        },
    )
    .expect_err("bytes alone cannot replace captured source identity");
    assert!(swapped);
    assert!(error.to_string().contains("identity"), "{error}");
}
#[test]
fn ordinary_one_to_four_descriptors_remain_byte_exact() {
    for (facade, profile, schema) in [
        (false, false, 1),
        (true, false, 2),
        (false, true, 3),
        (true, true, 4),
    ] {
        let f = Fixture::new();
        fs::remove_file(f.project.join("keld.build.ts")).expect("ordinary authoring");
        let mode = if facade {
            "initialWindow: \"none\""
        } else {
            "renderer: \"index.html\""
        };
        write(
            &f.project,
            "keld.config.ts",
            format!(
                "export default {{\n  name: \"Ordinary\",\n  entry: \"main/app.mjs\",\n  {mode}\n}} as const;\n"
            )
            .as_bytes(),
        );
        write(&f.project, "index.html", b"<html>ordinary</html>");
        write(
            &f.project,
            "keld.permissions.jsonc",
            if profile { POLICY } else { b"{}\n" },
        );
        let stage =
            stage_for_project(&f.project, &f.host).expect("ordinary path needs no framework tuple");
        let permissions = if profile {
            "c8e2fb1f67314067c0dd111708809338215a701f457b94aaca686d46e1cf7a63"
        } else {
            "ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"
        };
        let renderer = if facade {
            ""
        } else {
            ",\"renderer\":\"index.html\""
        };
        let expected = format!(
            "{{\"entry\":\"main/app.mjs\",\"name\":\"Ordinary\",\"permissions\":{{\"content_sha256\":\"sha256:{permissions}\",\"file\":\"keld.permissions.jsonc\"}}{renderer},\"schema\":{schema}}}"
        );
        assert_eq!(
            fs::read(stage.root().join("keld.boot.json")).expect("ordinary boot bytes"),
            expected.as_bytes()
        );
        assert!(
            !stage
                .root()
                .join(keld_core::app_session::GRAPH_FILE)
                .exists()
        );
        drop(stage);
    }
}
