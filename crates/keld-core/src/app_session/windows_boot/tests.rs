//! Policy/runtime metadata controls; no fake supervisor or native acceptance.
use super::bind_runtime_facts;
use crate::app_session::{load_verified_manifest, profile_preflight};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};

#[test]
fn generic_facade_does_not_require_or_invent_an_original_selection() {
    assert!(
        bind_runtime_facts(None, None)
            .expect("generic facade")
            .is_none()
    );
    let actual = BTreeMap::from([("bun".to_owned(), "metadata fixture only".to_owned())]);
    assert!(
        bind_runtime_facts(None, Some(&actual))
            .expect("generic facts")
            .is_none()
    );
}

#[test]
fn graph_profile_requires_explicit_legacy_and_cannot_reinterpret_old_boots() {
    let directory = tempfile::tempdir().expect("owned policy files");
    for (name, bytes) in [
        ("legacy", br#"{"profile":"legacy"}"#.as_slice()),
        ("strict", br#"{"profile":"strict"}"#.as_slice()),
        ("missing", b"{}".as_slice()),
    ] {
        let path = directory.path().join(name);
        fs::write(&path, bytes).expect("exact policy bytes");
        let verified = load_verified_manifest(
            File::open(&path).expect("actual policy file"),
            path,
            Sha256::digest(bytes).into(),
        )
        .expect("verified immutable policy");
        let admitted = profile_preflight::validate_windows_graph_profile(8, &verified);
        if name == "legacy" {
            assert_eq!(
                admitted.expect("explicit Legacy"),
                keld_guard::ProfileState::Legacy
            );
        } else {
            assert!(
                admitted.is_err(),
                "no implicit Legacy or uncontained Strict"
            );
        }
        for schema in [1, 2, 3, 4, 6] {
            assert!(profile_preflight::validate_windows_graph_profile(schema, &verified).is_err());
        }
        for diagnostic in [false, true] {
            assert!(profile_preflight::validate_profile(8, &verified, diagnostic).is_err());
        }
    }
}
