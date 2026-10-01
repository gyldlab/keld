//! Linux fixture construction and temporary project ownership.

use super::stage::assert_self_contained_kipc_entry;
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

fn bundle_t1b_entry(repo: &std::path::Path, project: &std::path::Path) -> String {
    let source = project.join("src/t1b-entry.ts");
    let bundled = project.join("src/t1b-entry.bundle.js");
    let api_link = serde_json::to_string(
        &repo
            .join("packages/@keld/api/src/link.ts")
            .to_string_lossy(),
    )
    .expect("encode canonical @keld/api link path");
    let kipc = serde_json::to_string(
        &repo
            .join("packages/@keld/kipc/src/transport.ts")
            .to_string_lossy(),
    )
    .expect("encode canonical @keld/kipc transport path");
    let prelude = format!(
        "import {{ DrainSignal, FrameKind, FrameReader, LIFECYCLE_CHANNEL, WriteQueue, errorFromErrFrame, isWin32PipeEndpoint, parseAppLink, withIoDeadline }} from {api_link};\n\
         import {{ parseWin32DiagnosticPort }} from {kipc};\n"
    );
    fs::write(
        &source,
        format!(
            "{prelude}{}",
            include_str!("../../../fixtures/t1b_harness.ts")
        ),
    )
    .expect("write Linux T1b bundle entry");
    let output = Command::new("bun")
        .arg("build")
        .arg(&source)
        .args(["--target=bun", "--format=esm", "--outfile"])
        .arg(&bundled)
        .output()
        .expect("bundle Linux T1b fixture through canonical @keld/api owner");
    assert!(
        output.status.success(),
        "Linux T1b API bundle failed: {output:?}"
    );
    let entry = fs::read_to_string(&bundled).expect("read bundled Linux T1b entry");
    fs::remove_file(source).expect("remove temporary Linux T1b source");
    fs::remove_file(bundled).expect("remove temporary Linux T1b bundle");
    entry
}

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
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("workspace root");
        let entry = bundle_t1b_entry(repo, &project);
        fs::write(project.join("src/main.ts"), entry).expect("product entry");
        let sidecar = project.join("src/kipc-transport.ts");
        if sidecar.exists() {
            fs::remove_file(&sidecar).expect("remove obsolete Linux KIPC sidecar");
        }
        assert_self_contained_kipc_entry(&project);
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
