//! Inputs only; actual child/renderer observations belong to the scenarios.

use crate::support::product::ProductFixture;
use std::fs;

const INPUTS: &str = "../../../fixtures/multifile_staging/";

pub(super) fn project(name: &str) -> ProductFixture {
    let fixture = ProductFixture::new(name);
    fs::write(
        fixture.project.join("src/sibling.ts"),
        include_bytes!("../../../fixtures/multifile_staging/sibling.ts"),
    )
    .expect("original sibling module");
    fs::write(
        fixture.project.join("relative.css"),
        include_bytes!("../../../fixtures/multifile_staging/relative.css"),
    )
    .expect("original relative stylesheet");
    fs::write(
        fixture.project.join("relative.svg"),
        include_bytes!("../../../fixtures/multifile_staging/relative.svg"),
    )
    .expect("original relative image");
    fixture
}

pub(super) fn renderer(fixture: &ProductFixture, port: u16) {
    let source = include_str!("../../../fixtures/multifile_staging/renderer.html");
    assert_eq!(source.matches("__GH1037_PORT__").count(), 1);
    fs::write(
        fixture.project.join("index.html"),
        source.replace("__GH1037_PORT__", &port.to_string()),
    )
    .expect("renderer with relative assets and independent inline controls");
}

pub(super) fn assert_original_inputs_exist(fixture: &ProductFixture) {
    for relative in ["src/sibling.ts", "relative.css", "relative.svg"] {
        assert!(
            fixture.project.join(relative).is_file(),
            "{INPUTS}{relative} must exist before the real stage producer runs"
        );
    }
}
