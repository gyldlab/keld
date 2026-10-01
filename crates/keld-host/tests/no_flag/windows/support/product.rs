//! Existing Windows project-fixture resource owners.

use std::{fs, process::Command};

use crate::{DARK_BG, PRODUCT_TITLE};

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
            "export default {\n  name: \"KEL96 T4 Fixture\",\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n} as const;\n",
        )
        .expect("project config");
        fs::write(project.join("src/main.ts"), "console.log('fixture');\n").expect("project entry");
        fs::write(project.join("index.html"), "<p id=exact>fixture</p>\n")
            .expect("project renderer");
        Self {
            _root: root,
            project,
        }
    }
}

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
    .expect("write Windows T1b bundle entry");
    let output = Command::new("bun")
        .arg("build")
        .arg(&source)
        .args(["--target=bun", "--format=esm", "--outfile"])
        .arg(&bundled)
        .output()
        .expect("bundle Windows T1b fixture through canonical @keld/api owner");
    assert!(
        output.status.success(),
        "Windows T1b API bundle failed: {output:?}"
    );
    let entry = fs::read_to_string(&bundled).expect("read bundled Windows T1b entry");
    fs::remove_file(source).expect("remove temporary Windows T1b source");
    fs::remove_file(bundled).expect("remove temporary Windows T1b bundle");
    entry
}

pub(crate) struct ProductFixture {
    pub(crate) root: tempfile::TempDir,
    pub(crate) project: std::path::PathBuf,
}

impl ProductFixture {
    pub(crate) fn new() -> Self {
        let root = tempfile::tempdir().expect("product fixture root");
        let project = root.path().join("project");
        fs::create_dir_all(project.join("src")).expect("product project src");
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
        fs::write(
            project.join("index.html"),
            format!("<!doctype html>{DARK_BG}\n"),
        )
        .expect("product renderer");
        Self { root, project }
    }
}
