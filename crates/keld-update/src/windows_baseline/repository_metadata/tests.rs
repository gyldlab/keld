//! Negative D-23 restart controls: canonical non-ready heads and native residue.
//!
//! These tests are not D-90 lease qualification, TUF signature proof or physical
//! crash/power-loss acceptance. They exercise only the existing private reader.

use std::fs;

use cap_std::ambient_authority;
use cap_std::fs::Dir;

use super::{require_empty_generation_namespace, require_established_ready_generation};
use crate::UpdateError;
use crate::records::decode_metadata_generation;

fn refusal_step(error: UpdateError) -> &'static str {
    match error {
        UpdateError::Baseline { step, .. } => step,
        other => panic!("expected native D-23 baseline refusal, observed {other}"),
    }
}

#[test]
fn absent_anchor_must_not_overlook_non_genesis_or_interrupted_generation() {
    // Each arm starts empty, then an independently written durable name
    // appears while the same native directory handle remains retained.
    // Removing this guard would wrongly allow a fresh pending anchor.
    for name in ["2.json", "pending-uncommitted", "1.json", "unknown"] {
        let fixture = tempfile::tempdir().expect("isolated D23 generation parent");
        let retained = Dir::open_ambient_dir(fixture.path(), ambient_authority())
            .expect("retain real Windows generation parent");
        require_empty_generation_namespace(&retained)
            .expect("empty local observation does not mint repository authority");

        let orphan = fixture.path().join(name);
        fs::write(&orphan, b"interrupted-generation-residue")
            .expect("persist an independent pre-existing generation member");
        let error = require_empty_generation_namespace(&retained)
            .expect_err("even non-genesis/unknown members block a fresh anchor");
        assert_eq!(refusal_step(error), "metadata generation residue");
        assert_eq!(
            fs::read(&orphan).expect("retain the exact orphan for operator diagnosis"),
            b"interrupted-generation-residue",
        );
    }
}

fn canonical_generation(phase: &str) -> Vec<u8> {
    let digest = "11".repeat(32);
    let role = format!(r#"{{"sha256":"{digest}","length":1,"version":1}}"#);
    let (timestamp, snapshot, targets, refresh) = match phase {
        "root-refresh" => (
            "null".to_owned(),
            "null".to_owned(),
            "null".to_owned(),
            format!(r#"{{"timestampKeysSha256":"{digest}","snapshotKeysSha256":"{digest}"}}"#),
        ),
        "roles" => (
            role.clone(),
            "null".to_owned(),
            "null".to_owned(),
            "null".to_owned(),
        ),
        "ready" => (role.clone(), role.clone(), role.clone(), "null".to_owned()),
        other => panic!("unexpected test phase {other}"),
    };
    format!(
        r#"{{"schema":"keld.metadata-generation/v1","repositoryId":"{digest}","sequence":1,"recoveryEpoch":0,"previousGenerationSha256":null,"policySha256":"{digest}","utcFloorMillis":0,"phase":"{phase}","root":{role},"timestamp":{timestamp},"snapshot":{snapshot},"targets":{targets},"rootRefresh":{refresh}}}"#,
    )
    .into_bytes()
}

#[test]
fn established_current_never_selects_a_canonical_incomplete_generation() {
    // These are structurally valid D-23 records, unlike a corrupt JSON
    // negative. A missing readiness guard would accept both partial heads.
    for phase in ["root-refresh", "roles"] {
        let bytes = canonical_generation(phase);
        let generation =
            decode_metadata_generation(&bytes).expect("canonical in-progress generation");
        let error = require_established_ready_generation(&generation)
            .expect_err("established selection requires a complete ready generation");
        assert_eq!(refusal_step(error), "metadata established readiness");
    }

    let ready_bytes = canonical_generation("ready");
    let ready = decode_metadata_generation(&ready_bytes).expect("canonical complete ready record");
    require_established_ready_generation(&ready)
        .expect("ready phase passes only the local phase check, not TUF verification");
}
