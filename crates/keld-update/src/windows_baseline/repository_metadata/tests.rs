//! Negative D-23 restart controls: canonical non-ready heads and native residue.
//!
//! These tests are not D-90 lease qualification, TUF signature proof or physical
//! crash/power-loss acceptance. They exercise only the existing private reader.

use std::fs;

use cap_std::ambient_authority;
use cap_std::fs::Dir;

use super::{
    require_canonical_anchor_leaf, require_empty_generation_namespace,
    require_established_ready_generation, require_only_genesis_and_successor,
    require_only_genesis_generation, require_pending_generation_namespace,
};
use crate::UpdateError;
use crate::records::decode_metadata_generation;

mod source_contract;

fn refusal_step(error: UpdateError) -> &'static str {
    match error {
        UpdateError::Baseline { step, .. } => step,
        other => panic!("expected native D-23 baseline refusal, observed {other}"),
    }
}

#[cfg(windows)]
#[test]
fn wrong_case_physical_enrollment_anchor_refuses_before_canonical_read() {
    let canonical = format!("{}.anchor", "ab".repeat(32));
    for alias in [
        format!("{}.ANCHOR", "ab".repeat(32)),
        format!("{}.Anchor", "ab".repeat(32)),
        canonical.to_uppercase(),
    ] {
        let fixture = tempfile::tempdir().expect("isolated enrollment parent");
        let alias_path = fixture.path().join(&alias);
        let original_bytes = b"unaltered-noncanonical-enrollment";
        fs::write(&alias_path, original_bytes).expect("persist physical noncanonical anchor");
        // The native Windows fixture demonstrates why the original derived
        // lowercase read could have admitted the wrong physical name.
        assert_eq!(
            fs::read(fixture.path().join(&canonical)).expect("NTFS case-insensitive alias"),
            original_bytes
        );
        let retained = Dir::open_ambient_dir(fixture.path(), ambient_authority())
            .expect("retain the actual enrollment parent");
        let error = require_canonical_anchor_leaf(&retained, &canonical)
            .expect_err("wrong-case anchor cannot become the protected D-23 identity");
        assert_eq!(refusal_step(error), "metadata enrollment namespace");
        assert_eq!(
            fs::read(&alias_path).expect("preserve original bytes"),
            original_bytes
        );
        assert_eq!(
            fs::read_dir(fixture.path())
                .expect("read original names")
                .count(),
            1,
            "negative namespace inspection must not modify or rename the anchor"
        );
    }
}

#[test]
fn canonical_enrollment_anchor_and_other_repository_remain_negative_only() {
    let fixture = tempfile::tempdir().expect("isolated enrollment parent");
    let canonical = format!("{}.anchor", "ab".repeat(32));
    let other_repository = format!("{}.anchor", "cd".repeat(32));
    let own_path = fixture.path().join(&canonical);
    let other_path = fixture.path().join(&other_repository);
    fs::write(&own_path, b"original-own-anchor").expect("persist canonical own anchor");
    fs::write(&other_path, b"unrelated-repository-anchor")
        .expect("persist another exact repository anchor");
    let retained = Dir::open_ambient_dir(fixture.path(), ambient_authority())
        .expect("retain enrollment namespace");
    require_canonical_anchor_leaf(&retained, &canonical)
        .expect("exact own anchor and unrelated canonical neighbor do not trigger alias denial");
    assert_eq!(
        fs::read(&own_path).expect("read unchanged canonical own anchor"),
        b"original-own-anchor"
    );
    assert_eq!(
        fs::read(&other_path).expect("read unchanged other repository anchor"),
        b"unrelated-repository-anchor"
    );
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

#[test]
fn pending_enrollment_retains_exact_empty_or_genesis_crash_cuts() {
    // A pending anchor may precede its first generation or have exactly
    // genesis 1.json. Current, if published, requires that same genesis.
    for (genesis_present, has_current) in [(false, false), (true, false), (true, true)] {
        let fixture = tempfile::tempdir().expect("isolated pending enrollment");
        if genesis_present {
            fs::write(fixture.path().join("1.json"), b"unchanged-genesis")
                .expect("persist original genesis");
        }
        let retained = Dir::open_ambient_dir(fixture.path(), ambient_authority())
            .expect("retain pending generations parent");
        require_pending_generation_namespace(&retained, has_current)
            .expect("legitimate pending crash-cut namespace passes only a negative check");
        if genesis_present {
            assert_eq!(
                fs::read(fixture.path().join("1.json")).expect("read original genesis"),
                b"unchanged-genesis",
            );
        }
    }
}

#[test]
fn pending_enrollment_rejects_orphaned_generation_members_without_retirement() {
    for (names, has_current) in [
        (&[][..], true),
        (&["2.json"][..], false),
        (&["pending-uncommitted"][..], false),
        (&["1.json", "2.json"][..], false),
        (&["1.json", "3.json"][..], true),
        (&["1.json", "pending-leftover"][..], true),
        (&["1.json", "unknown"][..], false),
        (&["1.JSON"][..], false),
        (&["01.json"][..], true),
    ] {
        let fixture = tempfile::tempdir().expect("isolated pending generation residue");
        for &name in names {
            fs::write(fixture.path().join(name), b"unchanged-pending-member")
                .expect("persist original crash-cut member");
        }
        let retained = Dir::open_ambient_dir(fixture.path(), ambient_authority())
            .expect("retain original pending directory");
        assert_eq!(
            refusal_step(
                require_pending_generation_namespace(&retained, has_current)
                    .expect_err("missing or foreign pending generation must refuse")
            ),
            "metadata generation residue",
        );
        for &name in names {
            assert_eq!(
                fs::read(fixture.path().join(name)).expect("retain original evidence"),
                b"unchanged-pending-member",
                "refusal must not retire or replace source evidence",
            );
        }
    }
}

#[test]
fn committed_genesis_must_not_hide_unreferenced_successor_or_pending_sibling() {
    // This is the D-24 restart cut after a durable Ready genesis and before
    // a successor publication/retirement. Without the whole-directory
    // negative check, a valid hash of 1.json would hide an orphan 2.json.
    // The fixture intentionally uses a real retained parent; it does not
    // pretend to qualify an OS-protected D-90 repository lease.
    for foreign_name in ["2.json", "pending-uncommitted", "3.json", "unknown"] {
        let fixture = tempfile::tempdir().expect("isolated committed genesis");
        let genesis = fixture.path().join("1.json");
        let foreign = fixture.path().join(foreign_name);
        let genesis_bytes = b"unchanged-committed-genesis";
        let foreign_bytes = b"unaccounted-preparation";
        fs::write(&genesis, genesis_bytes).expect("persist committed genesis leaf");
        let retained = Dir::open_ambient_dir(fixture.path(), ambient_authority())
            .expect("retain exact native generation parent");
        require_only_genesis_generation(&retained)
            .expect("one genesis leaf passes only this negative namespace check");

        fs::write(&foreign, foreign_bytes)
            .expect("persist a distinct orphan generation or partial publication");
        let error = require_only_genesis_generation(&retained)
            .expect_err("successor residue cannot be bypassed by initial current or new write");
        assert_eq!(refusal_step(error), "metadata generation residue");
        assert_eq!(
            fs::read(&genesis).expect("read unchanged committed generation"),
            genesis_bytes,
            "negative census must not rewrite the committed predecessor",
        );
        assert_eq!(
            fs::read(&foreign).expect("read orphan for recovery"),
            foreign_bytes,
            "negative census must preserve every unaccounted object for D-24 recovery",
        );
    }
}

#[test]
fn missing_genesis_is_not_a_valid_selected_generation_namespace() {
    let fixture = tempfile::tempdir().expect("isolated generation directory");
    let retained = Dir::open_ambient_dir(fixture.path(), ambient_authority())
        .expect("retain the empty native directory handle");
    assert_eq!(
        refusal_step(
            require_only_genesis_generation(&retained)
                .expect_err("missing 1.json cannot satisfy the established-current census")
        ),
        "metadata generation residue",
    );
}

#[test]
fn postpublication_census_refuses_extra_generation_without_retiring_any_bytes() {
    // The latest approved writer can prepare exactly one N=2 successor from a
    // committed N=1 genesis; an unexpected N=3 or pending sibling is not
    // evidence that N=2 has been committed or that cleanup can run.
    for unexpected in ["3.json", "pending-leftover"] {
        let fixture = tempfile::tempdir().expect("isolated published successor");
        let genesis = fixture.path().join("1.json");
        let successor = fixture.path().join("2.json");
        let foreign = fixture.path().join(unexpected);
        fs::write(&genesis, b"committed-n1").expect("persist native genesis fixture");
        fs::write(&successor, b"prepared-n2").expect("persist native successor fixture");
        let retained = Dir::open_ambient_dir(fixture.path(), ambient_authority())
            .expect("retain native generations parent");
        require_only_genesis_and_successor(&retained)
            .expect("one committed genesis plus one prepared successor is a local shape only");
        fs::write(&foreign, b"unknown-n3").expect("persist unexplained third member");

        assert_eq!(
            refusal_step(
                require_only_genesis_and_successor(&retained)
                    .expect_err("an unexplained third generation blocks publication readback")
            ),
            "metadata generation residue",
        );
        assert_eq!(
            fs::read(&genesis).expect("genesis preserved"),
            b"committed-n1"
        );
        assert_eq!(
            fs::read(&successor).expect("successor preserved"),
            b"prepared-n2"
        );
        assert_eq!(
            fs::read(&foreign).expect("foreign preserved"),
            b"unknown-n3"
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
