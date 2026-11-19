//! Windows CLI process proof for bounded selected graph authoring.

#![cfg(windows)]

use std::fs;
use std::process::Command;

#[test]
fn ordinary_dev_refuses_oversized_selected_graph_config_before_doctor() {
    let fixture = tempfile::tempdir().expect("isolated graph project");
    let root = fixture.path();
    let mut config = String::from(
        "export default { id:'dev.example.inert', name:'Inert', initialWindow:'none',
        rendererSource:{root:'client/assets',entry:'index.html'} };\n//",
    );
    config.push_str(&"x".repeat(64 * 1024));
    fs::write(root.join("keld.config.ts"), config).expect("oversized selected config");

    let digest = "ab".repeat(32);
    let build = format!(
        "export default {{graphSource:{{packageSha256:'{digest}',lockSha256:'{digest}',
        providerTrees:{{electron:'{digest}',api:'{digest}',kipc:'{digest}'}},
        preload:{{path:'bootstrap/preload.cjs',sha256:'{digest}'}}}}}};"
    );
    fs::write(root.join("keld.build.ts"), build).expect("closed graph declaration");
    // Deliberately omit src/main.ts so old doctor-first ordering would report
    // a layout failure instead of the selected configuration bound.
    let output = Command::new(env!("CARGO_BIN_EXE_keld"))
        .arg("dev")
        .current_dir(root)
        .output()
        .expect("start ordinary keld dev");
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostic");
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("graph authoring bound") && stderr.contains("exceeds 64 KiB"),
        "{stderr}"
    );
    assert!(!root.join(".keld").exists(), "no stage was published");
}
