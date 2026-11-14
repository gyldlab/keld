//! Source-only B-3 capture probe against the actual keld-pack graph transaction.
//!
//! The callback checks the copied tree before any Host/Bun image is opened,
//! signed or published. It intentionally refuses graph compilation. The caller
//! creates an EMPTY node_modules directory solely to test directory capture;
//! this file never selects or installs any provider.

use std::cell::Cell;
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use keld_pack::{
    MacosBundleMetadata, MacosCopiedMemberKind, MacosExecutableInput, MacosGraphCompiled,
    MacosPackageInput, produce_macos_local_with_graph,
};

fn require_file(root: &Path, copied: &Path, members: &[keld_pack::MacosCopiedMember], name: &str) {
    assert!(
        members
            .iter()
            .any(|member| member.path == name && member.kind == MacosCopiedMemberKind::File),
        "Pack did not capture required root file: {name}"
    );
    assert_eq!(
        fs::read(copied.join(name)).expect("copied root input"),
        fs::read(root.join(name)).expect("original root input"),
        "Pack changed root file during capture: {name}"
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let project_root = PathBuf::from(args.next().ok_or("missing project root")?).canonicalize()?;
    let resources: Vec<String> = args
        .map(|value| value.into_string().map_err(|_| "non-UTF-8 resource root"))
        .collect::<Result<_, _>>()?;
    assert!(!resources.is_empty(), "no declared capture roots");

    let input = MacosPackageInput {
        project_root: project_root.clone(),
        metadata: MacosBundleMetadata {
            app_id: "dev.keld.gh1404.minimal".into(),
            name: "Keld GH1404 Minimal".into(),
            version: "0.0.1".into(),
            bundle_version: "1".into(),
            minimum_system_version: "14.0".into(),
            icon: false,
            bun_sha256: [0; 32],
        },
        // No executable is opened: the graph callback refuses first.
        // These deliberately invalid test inputs are never source approvals.
        host: MacosExecutableInput {
            path: PathBuf::from("UNQUALIFIED_HOST"),
            sha256: [0; 32],
            version: "unqualified".into(),
        },
        bun: MacosExecutableInput {
            path: PathBuf::from("UNQUALIFIED_BUN"),
            sha256: [0; 32],
            version: "unqualified".into(),
        },
        entry: "src/main.mjs".into(),
        renderer: None,
        resources,
        icon: None,
        boot: Vec::new(),
        permissions: fs::read(project_root.join("keld.permissions.jsonc"))?,
        generated: Vec::new(),
        out_dir: PathBuf::from("dist"),
    };

    let called = Cell::new(0);
    let error = produce_macos_local_with_graph(&input, |copied, retained, members| {
        called.set(called.get() + 1);
        assert!(
            retained.metadata().expect("retained copied root").is_dir(),
            "Pack must supply a retained copied root"
        );
        require_file(&project_root, copied, members, "package.json");
        require_file(&project_root, copied, members, "package-lock.json");
        assert!(
            members.iter().any(|member| {
                member.path == "node_modules" && member.kind == MacosCopiedMemberKind::Directory
            }),
            "Pack did not capture declared node_modules root"
        );
        assert!(
            copied.join("node_modules").is_dir()
                && fs::read_dir(copied.join("node_modules"))
                    .expect("copied unqualified empty directory")
                    .next()
                    .is_none(),
            "the probe must not create or qualify a provider"
        );
        Err::<MacosGraphCompiled, _>("GH1404_B3_EXPECTED_PRE_SIGN_REFUSAL")
    })
    .expect_err("capture probe must intentionally refuse before signing");
    assert_eq!(
        called.get(),
        1,
        "actual Pack callback was not reached exactly once"
    );
    assert!(
        error
            .to_string()
            .contains("GH1404_B3_EXPECTED_PRE_SIGN_REFUSAL"),
        "failure occurred before the copied-tree evidence: {error}"
    );
    assert!(
        !project_root.join("dist").exists(),
        "test-only refusal must publish no output"
    );
    println!("GH1404_B3_PACK_CAPTURE_PASS_REAL_TRANSACTION");
    Ok(())
}
