//! Native Windows extraction stage admission and reparse refusal oracles.
//!
//! Every test observes actual retained Windows filesystem effects and must
//! refuse staging collisions, symlink escapes or unconfirmed publication.

use std::fs;
use std::os::windows::fs::{symlink_dir, symlink_file};

use super::{ExtractionEvent, Fixture, GOLDEN_TAR, UpdateError, expect_extraction};

#[test]
fn stage_fault_is_named_and_incomplete_without_publication() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut created_name = String::new();
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::AfterStageCreate {
                created_name = name.to_owned();
                return Err(std::io::Error::other("injected after stage creation"));
            }
            Ok(())
        })
        .expect_err("injected failure returned a stage");
    assert_ne!(created_name.len(), 0);
    assert_eq!(
        expect_extraction(error, true).as_deref(),
        Some(created_name.as_str())
    );
    let stage_path = fixture.versions().join(&created_name);
    assert!(
        stage_path.is_dir(),
        "incomplete stage remains diagnostic state"
    );
    assert!(!stage_path.join(".complete").exists());
    assert!(!fixture.versions().join("2.0.0").exists());
}

#[test]
fn precreate_stage_name_collision_refuses_without_reusing_existing_object() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut colliding_name = String::new();
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::PreCreate {
                colliding_name = name.to_owned();
                fs::create_dir(fixture.versions().join(name))?;
            }
            Ok(())
        })
        .expect_err("colliding stage reused");
    assert_ne!(colliding_name.len(), 0);
    assert!(matches!(
        &error,
        UpdateError::Extraction {
            incomplete_stage: Some(name),
            step: "stage creation (outcome unconfirmed)",
            ..
        } if name == &colliding_name
    ));
    assert_eq!(error.code(), "KELD-UPDATE-012");
    let collision = fixture.versions().join(colliding_name);
    assert!(collision.is_dir());
    assert_eq!(
        fs::read_dir(&collision)
            .expect("collision untouched")
            .count(),
        0
    );
}

#[test]
fn existing_child_collision_refuses_before_writing_that_member() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut stage_name = String::new();
    let mut planted = false;
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::AfterStageCreate {
                stage_name = name.to_owned();
            }
            if event == ExtractionEvent::BeforeMember && name == "nest" {
                fs::write(
                    fixture.versions().join(&stage_name).join("tree/nest"),
                    b"collision",
                )?;
                planted = true;
            }
            Ok(())
        })
        .expect_err("existing file ancestor accepted");
    assert!(planted, "hook reached the contested directory");
    assert_eq!(
        expect_extraction(error, true).as_deref(),
        Some(stage_name.as_str())
    );
    let stage_path = fixture.versions().join(&stage_name);
    assert_eq!(
        fs::read(stage_path.join("tree/nest")).expect("collision survives"),
        b"collision"
    );
    assert!(!stage_path.join("tree/nest/one").exists());
    assert!(!stage_path.join(".complete").exists());
}

#[test]
fn preplanted_leaf_reparse_cannot_redirect_extracted_file() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let outside = fixture.temp.path().join("outside-marker.txt");
    fs::write(&outside, b"outside original").expect("outside sentinel");
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut stage_name = String::new();
    let mut planted = false;
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::AfterStageCreate {
                stage_name = name.to_owned();
            }
            if event == ExtractionEvent::BeforeMember && name == "nest/one" {
                let leaf = fixture.versions().join(&stage_name).join("tree/nest/one");
                symlink_file(&outside, leaf)?;
                planted = true;
            }
            Ok(())
        })
        .expect_err("leaf reparse redirected extraction");
    assert!(planted, "fixture planted a real leaf reparse");
    assert_eq!(
        expect_extraction(error, true).as_deref(),
        Some(stage_name.as_str())
    );
    assert_eq!(
        fs::read(&outside).expect("outside sentinel remains"),
        b"outside original"
    );
    assert!(
        !fixture
            .versions()
            .join(stage_name)
            .join(".complete")
            .exists()
    );
}

#[test]
fn preplanted_ancestor_reparse_cannot_redirect_extracted_tree() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let outside = fixture.temp.path().join("outside-dir");
    fs::create_dir(&outside).expect("outside directory");
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut stage_name = String::new();
    let mut planted = false;
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::AfterStageCreate {
                stage_name = name.to_owned();
            }
            if event == ExtractionEvent::BeforeMember && name == "nest" {
                let ancestor = fixture.versions().join(&stage_name).join("tree/nest");
                symlink_dir(&outside, ancestor)?;
                planted = true;
            }
            Ok(())
        })
        .expect_err("ancestor reparse redirected extraction");
    assert!(planted, "fixture planted a real directory reparse");
    assert_eq!(
        expect_extraction(error, true).as_deref(),
        Some(stage_name.as_str())
    );
    assert_eq!(
        fs::read_dir(&outside)
            .expect("outside stays readable")
            .count(),
        0
    );
    assert!(
        !fixture
            .versions()
            .join(stage_name)
            .join(".complete")
            .exists()
    );
}
