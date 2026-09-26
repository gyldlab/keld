//! Linux fixture construction and temporary project ownership.

use super::stage::assert_imported_kipc_sidecar_exists;
use std::{fs, os::unix::fs::PermissionsExt as _, process::Command};

pub(crate) const DEV_HELPER_TEST: &str = "keld_dev_linux_helper";

/// Dark background for fixture renderers, so a test run does not flash
/// white windows across the operator's desktop. Cosmetic only: no test
/// asserts on it, and the beacon/marker contracts are unchanged.
pub(crate) const DARK_BG: &str = "<style>html,body{background:#111;color:#eee}</style>";
pub(crate) struct StageFixture {
    _root: tempfile::TempDir,
    pub(crate) project: std::path::PathBuf,
}

impl StageFixture {
    pub(crate) fn new() -> Self {
        let root = tempfile::tempdir().expect("stage fixture root");
        let project = root.path().join("project");
        fs::create_dir_all(project.join("src")).expect("project src");
        fs::write(
            project.join("keld.config.ts"),
            "export default { name: \"Linux no-flag\", entry: \"src/main.ts\", renderer: \"index.html\" } as const;\n",
        )
        .expect("project config");
        fs::write(project.join("src/main.ts"), "console.log('linux');\n").expect("entry");
        fs::write(
            project.join("index.html"),
            format!("<!doctype html>{DARK_BG}<h1>Linux</h1>\n"),
        )
        .expect("renderer");
        Self {
            _root: root,
            project,
        }
    }
}

pub(crate) const PRODUCT_TITLE: &str = "KEL96 T4 Linux Fixture";
pub(crate) struct ProductFixture {
    pub(crate) root: tempfile::TempDir,
    pub(crate) project: std::path::PathBuf,
}

impl ProductFixture {
    pub(crate) fn new() -> Self {
        let root = tempfile::tempdir().expect("product fixture root");
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700))
            .expect("owner-private product fixture root");
        let project = keld_cli::create::create_project(root.path(), "product")
            .expect("create product fixture through the stock scaffold owner");
        fs::write(
            project.join("keld.config.ts"),
            format!(
                "export default {{\n  name: \"{PRODUCT_TITLE}\",\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n}} as const;\n"
            ),
        )
        .expect("product config");
        // create_project already wrote src/kipc-transport.ts. Keep it: Linux
        // strict remaps src/main.ts to /code/main.ts and binds the sidecar to
        // /code/kipc-transport.ts as its own file mount.
        fs::write(
            project.join("src/main.ts"),
            format!(
                "{}{}",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../packages/@keld/electron/src/link.ts"
                ))
                .replace("../../kipc/src/transport.ts", "./kipc-transport.ts"),
                include_str!("../../../fixtures/t1b_harness.ts")
            ),
        )
        .expect("product entry");
        assert_imported_kipc_sidecar_exists(&project);
        fs::write(
            project.join("index.html"),
            format!("<!doctype html>{DARK_BG}\n"),
        )
        .expect("renderer");
        Self { root, project }
    }
}

pub(crate) fn prepare_keld_dev_helper(fixture: &ProductFixture) -> std::path::PathBuf {
    let helper_dir = fixture.root.path().join("bin");
    fs::create_dir(&helper_dir).expect("helper directory");
    let helper = helper_dir.join("keld-dev-helper");
    fs::copy(std::env::current_exe().expect("test executable"), &helper).expect("copy helper");
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).expect("helper mode");
    // libtest can exit zero when an exact selector matches nothing. Check the
    // copied binary's registry here; scenarios still require the live handshake.
    let selected = Command::new(&helper)
        .args(["--list", "--format", "terse", "--exact", DEV_HELPER_TEST])
        .output()
        .expect("list the copied dev helper selector");
    assert!(
        selected.status.success(),
        "helper listing failed: {selected:?}"
    );
    assert_eq!(
        String::from_utf8(selected.stdout).expect("helper listing UTF-8"),
        format!("{DEV_HELPER_TEST}: test\n"),
        "exact dev helper selector must resolve to one registered test"
    );
    let developer_host = helper_dir.join("keld-host");
    fs::copy(env!("CARGO_BIN_EXE_keld-host"), &developer_host).expect("copy sibling host");
    fs::set_permissions(&developer_host, fs::Permissions::from_mode(0o500)).expect("host mode");
    let developer_launcher = helper_dir.join("keld-role-launcher");
    fs::copy(
        env!("CARGO_BIN_EXE_keld-role-launcher"),
        &developer_launcher,
    )
    .expect("copy sibling role launcher");
    fs::set_permissions(&developer_launcher, fs::Permissions::from_mode(0o500))
        .expect("role launcher mode");
    helper
}
