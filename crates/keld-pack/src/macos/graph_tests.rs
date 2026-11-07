//! Copied-tree graph transaction controls. Core separately owns graph semantics;
//! these tests cannot qualify an installed application or original editor.

use std::cell::Cell;
use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::process::Command;

use tempfile::TempDir;

use super::{compile_fixture, fixture, run_tool};
use crate::macos::capture::{Capture, digest_file};
use crate::{MacosGeneratedFile, MacosGraphCompiled, produce_macos_local_with_graph};

const GRAPH: &[u8] = b"{\"schema\":\"pack-transaction-test-only\"}\n";
const BOOT: &[u8] = b"{\"schema\":6,\"test_only\":true}\n";

fn compiled() -> MacosGraphCompiled {
    MacosGraphCompiled {
        graph: GRAPH.to_vec(),
        boot: BOOT.to_vec(),
    }
}

#[test]
fn graph_transaction_requires_deferred_boot_and_rejects_presupplied_authority() {
    let temporary = TempDir::new().expect("owned graph fixture");
    let mut input = fixture(temporary.path());
    assert!(
        Capture::plan_graph(&input).is_err(),
        "a prior boot is not a graph placeholder"
    );
    input.boot.clear();
    assert!(
        Capture::plan(&input).is_err(),
        "plain packaging still requires a compiled boot"
    );
    Capture::plan_graph(&input).expect("the graph-only deferred boot");
    for path in ["keld.boot.json", "keld.stage-graph.json"] {
        input.generated = vec![MacosGeneratedFile {
            path: path.into(),
            bytes: b"{}".to_vec(),
        }];
        assert!(
            Capture::plan_graph(&input).is_err(),
            "caller supplied {path}"
        );
    }
}

#[test]
fn graph_compiler_failure_is_once_and_never_publishes_or_writes_a_placeholder() {
    let temporary = TempDir::new().expect("owned graph fixture");
    let mut input = fixture(temporary.path());
    input.boot.clear();
    let calls = Cell::new(0);
    let error = produce_macos_local_with_graph(&input, |path, root, members| {
        calls.set(calls.get() + 1);
        assert!(!path.join("keld.boot.json").exists());
        assert!(!path.join("keld.stage-graph.json").exists());
        let named = fs::symlink_metadata(path).expect("copied path");
        let held = root.metadata().expect("retained root");
        assert_eq!((named.dev(), named.ino()), (held.dev(), held.ino()));
        assert!(
            members
                .iter()
                .any(|member| member.path == "keld.permissions.jsonc")
        );
        assert!(members.iter().any(|member| member.path == "assets/main.js"));
        Err::<MacosGraphCompiled, _>("independent source qualification refused")
    })
    .expect_err("compiler refusal");
    assert_eq!(calls.get(), 1);
    assert!(error.to_string().contains("graph compilation"), "{error}");
    assert!(
        error
            .to_string()
            .contains("independent source qualification refused"),
        "{error}"
    );
    assert!(!input.project_root.join("output").exists());
    assert!(
        fs::read_dir(&input.project_root)
            .expect("retained project")
            .all(|entry| !entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".keld-macos-build-"))
    );
}

#[test]
fn graph_transaction_refuses_byte_identical_leaf_swap_extra_missing_and_mode_changes() {
    for mutation in ["same-byte-swap", "extra", "missing", "mode", "content"] {
        let temporary = TempDir::new().expect("owned graph mutation fixture");
        let mut input = fixture(temporary.path());
        input.boot.clear();
        let called = Cell::new(false);
        let error = produce_macos_local_with_graph(&input, |path, _, _| {
            called.set(true);
            let leaf = path.join("assets/main.js");
            match mutation {
                "same-byte-swap" => {
                    let replacement = path.join("assets/replacement.js");
                    fs::copy(&leaf, &replacement).expect("same-byte replacement");
                    assert_ne!(
                        fs::metadata(&leaf).expect("old identity").ino(),
                        fs::metadata(&replacement).expect("new identity").ino()
                    );
                    fs::rename(replacement, &leaf).expect("atomic copied-leaf replacement");
                }
                "extra" => fs::write(path.join("unqualified.json"), b"{}").expect("extra member"),
                "missing" => fs::remove_file(&leaf).expect("remove selected member"),
                "mode" => fs::set_permissions(&leaf, fs::Permissions::from_mode(0o600))
                    .expect("mode mutation"),
                "content" => {
                    fs::write(&leaf, b"console.log('different');\n").expect("content mutation");
                }
                _ => unreachable!(),
            }
            Ok::<_, String>(compiled())
        })
        .expect_err("changed compiler input must refuse");
        assert!(called.get(), "mutation was not exercised");
        assert!(
            error.to_string().contains("copied graph identity"),
            "{mutation}: {error}"
        );
        assert!(!input.project_root.join("output").exists(), "{mutation}");
    }
}

#[test]
fn graph_transaction_refuses_copied_root_replacement_even_with_equal_contents() {
    let temporary = TempDir::new().expect("owned graph root fixture");
    let mut input = fixture(temporary.path());
    input.boot.clear();
    let error = produce_macos_local_with_graph(&input, |path, root, _| {
        let original = path.with_file_name("old-app-object");
        fs::rename(path, &original).expect("move original root");
        let mut copy = Command::new("/usr/bin/ditto");
        copy.arg(&original).arg(path);
        run_tool(copy);
        assert_ne!(
            root.metadata().expect("retained root").ino(),
            fs::metadata(path).expect("replacement root").ino()
        );
        Ok::<_, String>(compiled())
    })
    .expect_err("copied root replacement refused");
    assert!(error.to_string().contains("inventory root path"), "{error}");
    assert!(!input.project_root.join("output").exists());
}

#[test]
fn graph_transaction_refuses_empty_oversized_and_native_compiled_leaves() {
    for case in [
        "empty-graph",
        "empty-boot",
        "large-graph",
        "large-boot",
        "native-graph",
    ] {
        let temporary = TempDir::new().expect("owned graph bound fixture");
        let mut input = fixture(temporary.path());
        input.boot.clear();
        let error = produce_macos_local_with_graph(&input, |_, _, _| {
            let mut result = compiled();
            match case {
                "empty-graph" => result.graph.clear(),
                "empty-boot" => result.boot.clear(),
                "large-graph" => result.graph = vec![b' '; 8 * 1024 * 1024 + 1],
                "large-boot" => result.boot = vec![b' '; 64 * 1024 + 1],
                "native-graph" => result.graph = b"\x7fELF\x02\x01".to_vec(),
                _ => unreachable!(),
            }
            Ok::<_, String>(result)
        })
        .expect_err("invalid compiled leaf");
        assert_eq!(error.code(), "KELD-PACK-014");
        assert!(
            error.to_string().contains(if case == "native-graph" {
                "generated native code"
            } else {
                "compiled graph bounds"
            }),
            "{case}: {error}"
        );
        assert!(!input.project_root.join("output").exists());
    }
}

#[test]
fn native_graph_transaction_signs_and_copy_verifies_only_the_compiled_fixed_leaves() {
    let temporary = TempDir::new().expect("owned native graph fixture");
    let mut input = fixture(temporary.path());
    input.boot.clear();
    input.generated.push(MacosGeneratedFile {
        path: "generated/bootstrap.mjs".into(),
        bytes: b"export const bootstrap = true;\n".to_vec(),
    });
    let host = compile_fixture(&input.project_root, "host");
    let bun = compile_fixture(&input.project_root, "bun");
    let mut sign = Command::new("/usr/bin/codesign");
    sign.args(["--sign", "-", "--force", "--timestamp=none"])
        .arg(&bun);
    run_tool(sign);
    input.host.sha256 = digest_file(&host).expect("independent fixture Host pin");
    input.bun.sha256 = digest_file(&bun).expect("independent fixture Bun pin");
    input.metadata.bun_sha256 = input.bun.sha256;
    let calls = Cell::new(0);
    let output = produce_macos_local_with_graph(&input, |path, _, members| {
        calls.set(calls.get() + 1);
        assert_ne!(path, input.project_root);
        assert!(
            members
                .iter()
                .any(|member| member.path == "generated/bootstrap.mjs")
        );
        assert!(!members.iter().any(|member| member.path == "keld.boot.json"));
        assert!(
            !members
                .iter()
                .any(|member| member.path == "keld.stage-graph.json")
        );
        Ok::<_, String>(compiled())
    })
    .expect("native graph packaging transaction");
    assert_eq!(calls.get(), 1);
    let app = output.app.join("Contents/Resources/app");
    assert_eq!(
        fs::read(app.join("keld.stage-graph.json")).expect("signed graph"),
        GRAPH
    );
    assert_eq!(
        fs::read(app.join("keld.boot.json")).expect("signed boot"),
        BOOT
    );
    assert_eq!(
        digest_file(&output.app.join("Contents/Helpers/bun")).expect("final Bun"),
        input.bun.sha256
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&output.receipt).expect("receipt")).expect("receipt JSON");
    for name in ["keld.boot.json", "keld.stage-graph.json"] {
        assert!(
            receipt["capturedApplicationFiles"]
                .as_array()
                .expect("captured members")
                .iter()
                .any(|member| member["path"] == name && member["mode"] == 0o644)
        );
    }
    assert!(output.dmg.is_file());
}
