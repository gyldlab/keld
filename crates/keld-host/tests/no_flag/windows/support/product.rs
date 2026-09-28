//! Existing Windows project-fixture resource owners.

use std::fs;

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
        fs::write(
            project.join("src/kipc-transport.ts"),
            include_str!("../../../../../../packages/@keld/kipc/src/transport.ts"),
        )
        .expect("canonical kipc transport");
        let link = include_str!("../../../../../../packages/@keld/electron/src/link.ts")
            .replace("../../kipc/src/transport.ts", "./kipc-transport.ts");
        fs::write(
            project.join("src/main.ts"),
            format!(
                "{}{}",
                link,
                include_str!("../../../fixtures/t1b_harness.ts")
            ),
        )
        .expect("product entry");
        fs::write(
            project.join("index.html"),
            format!("<!doctype html>{DARK_BG}\n"),
        )
        .expect("product renderer");
        Self { root, project }
    }
}
