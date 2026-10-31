//! Native Windows authenticated archive-source admission and retention checks.
//!
//! Stage creation must refuse an untrusted source object, while accepted
//! retained sources are protected against concurrent mutation.

use std::fs::{self, OpenOptions};
use std::os::windows::fs::{OpenOptionsExt as _, symlink_file};

use super::{
    ExtractionEvent, Fixture, GOLDEN_TAR, assert_empty_versions, expect_extraction, source_mapping,
};

#[test]
fn locked_and_hardlinked_sources_refuse_before_stage_creation() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");

    let lock = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&source)
        .expect("exclusive source lock");
    let error = root
        .extract(&receipt, &source)
        .expect_err("locked source extracted");
    expect_extraction(error, false);
    assert_empty_versions(&fixture.versions());
    drop(lock);

    let alias = fixture.temp.path().join("source-link.tar");
    fs::hard_link(&source, &alias).expect("create real NTFS hardlink");
    let error = root
        .extract(&receipt, &source)
        .expect_err("hardlinked source extracted");
    expect_extraction(error, false);
    assert_empty_versions(&fixture.versions());
}

#[test]
fn reparse_source_refuses_before_stage_creation() {
    let fixture = Fixture::new();
    let target = fixture.source(GOLDEN_TAR);
    let link = fixture.temp.path().join("source-link-symlink.tar");
    symlink_file(&target, &link)
        .expect("real Windows source reparse fixture requires symlink privilege");
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let error = root
        .extract(&receipt, &link)
        .expect_err("reparse source extracted");
    expect_extraction(error, false);
    assert_empty_versions(&fixture.versions());
}

#[test]
fn writable_mapping_blocks_source_admission_but_readonly_mapping_is_valid() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let writable = source_mapping(&source, true);
    let error = root
        .extract(&receipt, &source)
        .expect_err("writable mapped source extracted");
    expect_extraction(error, false);
    assert_empty_versions(&fixture.versions());
    drop(writable);

    let readonly = source_mapping(&source, false);
    let stage = root
        .extract(&receipt, &source)
        .expect("readonly mapping is safe");
    assert_eq!(
        fs::read(fixture.versions().join(stage.name()).join("content.tar"))
            .expect("protected content"),
        GOLDEN_TAR
    );
    drop(stage);
    drop(readonly);
}

#[test]
fn retained_source_blocks_writer_after_complete_preflight() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut observed = false;
    let stage = root
        .extract_inner(&receipt, &source, |event, _| {
            if event == ExtractionEvent::PreCreate {
                observed = true;
                assert!(
                    OpenOptions::new().write(true).open(&source).is_err(),
                    "retained source must exclude a concurrent writer"
                );
            }
            Ok(())
        })
        .expect("source remains valid after denied writer");
    assert!(observed, "hook ran after complete preflight");
    assert_eq!(
        fs::read(fixture.versions().join(stage.name()).join("content.tar"))
            .expect("protected content"),
        GOLDEN_TAR
    );
}
