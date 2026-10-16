//! Native Windows staged-archive readback integrity regressions.
//!
//! The authenticated retained source must stay unchanged; a staged copy whose
//! bytes change after writing must not become a published update, even when
//! its byte length is unchanged.

use std::fs;

use super::{ExtractionEvent, Fixture, GOLDEN_TAR, expect_extraction};

#[test]
fn changed_retained_tar_is_rejected_on_readback() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut stage_name = String::new();
    let mut tampered = false;
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::AfterStageCreate {
                stage_name = name.to_owned();
            }
            if event == ExtractionEvent::BeforeReadback && name == "content.tar" {
                fs::write(
                    fixture.versions().join(&stage_name).join("content.tar"),
                    b"changed",
                )?;
                tampered = true;
            }
            Ok(())
        })
        .expect_err("changed retained tar accepted");
    assert!(tampered, "hook reached content.tar after writer close");
    assert_eq!(
        expect_extraction(error, true).as_deref(),
        Some(stage_name.as_str())
    );
    assert_eq!(fs::read(source).expect("source retained"), GOLDEN_TAR);
    assert!(
        !fixture
            .versions()
            .join(stage_name)
            .join(".complete")
            .exists()
    );
}

#[test]
fn same_length_readback_tamper_fails_the_digest_predicate() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut stage_name = String::new();
    let mut tampered = false;
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::AfterStageCreate {
                stage_name = name.to_owned();
            }
            if event == ExtractionEvent::BeforeReadback && name == "content.tar" {
                let mut changed = GOLDEN_TAR.to_vec();
                changed[0] ^= 1;
                fs::write(
                    fixture.versions().join(&stage_name).join("content.tar"),
                    &changed,
                )?;
                tampered = true;
            }
            Ok(())
        })
        .expect_err("same-length readback tamper accepted");
    assert!(tampered, "hook reached content.tar after writer close");
    assert_eq!(
        expect_extraction(error, true).as_deref(),
        Some(stage_name.as_str())
    );
    let retained = fixture.versions().join(&stage_name).join("content.tar");
    assert_eq!(
        fs::metadata(&retained).expect("same object remains").len(),
        GOLDEN_TAR.len() as u64
    );
    assert_eq!(
        fs::read(&retained).expect("tampered stage copy")[0],
        GOLDEN_TAR[0] ^ 1
    );
    assert_eq!(
        fs::read(source).expect("authenticated source intact"),
        GOLDEN_TAR
    );
    assert!(
        !fixture
            .versions()
            .join(stage_name)
            .join(".complete")
            .exists()
    );
}
