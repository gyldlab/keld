//! Actual retained Windows files, with inert independent fixture receipts.
//! These controls do not qualify a shipping provider, Host, Bun, or native view.
use super::super::windows_boot::{
    WindowsDistributionQualification, bind_with_qualification, qualified_windows_distribution,
    read_windows_bound_graph_from_retained_root, windows_graph_target,
};
use super::{AdmittedStageGraph, Fixture, GRAPH_FILE, MAIN, approved};
use crate::app_session::{ORIGINAL_RUNTIME_ENTRY_PATH, original_entry_bootstrap_source};
use sha2::{Digest as _, Sha256};
use std::fs::{self, File};
use std::path::Path;

fn qualification() -> WindowsDistributionQualification {
    WindowsDistributionQualification {
        target: windows_graph_target(),
        provider_trees: approved().expected_package_trees,
    }
}

fn fixture_with_entry() -> Fixture {
    let f = Fixture::new();
    Fixture::write(
        &f.root,
        ORIGINAL_RUNTIME_ENTRY_PATH,
        &original_entry_bootstrap_source(MAIN).expect("canonical source"),
    );
    f
}

fn bind(
    f: &Fixture,
    q: Option<&WindowsDistributionQualification>,
) -> Result<AdmittedStageGraph, crate::app_session::HostAppError> {
    let bytes = f.compile()?;
    fs::write(f.root.join(GRAPH_FILE), &bytes).expect("same captured leaf");
    bind_with_qualification(
        &f.root,
        &f.held,
        Sha256::digest(&bytes).into(),
        Path::new(MAIN),
        &f.selected,
        q,
    )
}

#[test]
fn production_boot8_refuses_even_a_self_consistent_real_graph_without_receipt() {
    let f = fixture_with_entry();
    let bytes = f.compile().expect("fixture graph");
    fs::write(f.root.join(GRAPH_FILE), &bytes).expect("same graph");
    let missing = bind_with_qualification(
        &f.root,
        &f.held,
        Sha256::digest(&bytes).into(),
        Path::new(MAIN),
        &f.selected,
        None,
    )
    .expect_err("an explicit missing qualification always refuses");
    assert!(
        missing
            .to_string()
            .contains("no independently qualified Windows target/provider"),
        "{missing}"
    );
    let qualified = qualified_windows_distribution();
    #[cfg(all(target_arch = "x86_64", target_env = "msvc"))]
    {
        let qualified = qualified.expect("the reviewed Windows target has a compiled receipt");
        assert_eq!(qualified.target(), "x86_64-pc-windows-msvc");
        assert_ne!(
            qualified.provider_trees(),
            &approved().expected_package_trees
        );
    }
    #[cfg(not(all(target_arch = "x86_64", target_env = "msvc")))]
    assert!(
        qualified.is_none(),
        "other Windows targets remain unqualified"
    );
    let error = read_windows_bound_graph_from_retained_root(
        &f.root,
        &f.held,
        Sha256::digest(&bytes).into(),
        Path::new(MAIN),
        &f.selected,
    )
    .expect_err("inert fixture providers cannot use the production receipt");
    #[cfg(all(target_arch = "x86_64", target_env = "msvc"))]
    assert!(
        error
            .to_string()
            .contains("external provider identity differs"),
        "{error}"
    );
    #[cfg(not(all(target_arch = "x86_64", target_env = "msvc")))]
    assert!(
        error
            .to_string()
            .contains("no independently qualified Windows target/provider"),
        "{error}"
    );
}

#[test]
fn target_and_provider_receipts_cannot_be_replaced_by_matching_project_data() {
    let f = fixture_with_entry();
    let mut foreign = qualification();
    foreign.target = "foreign-target";
    let error = bind(&f, Some(&foreign)).expect_err("foreign target");
    assert!(
        error.to_string().contains("foreign build target"),
        "{error}"
    );
    let mut wrong_provider = qualification();
    wrong_provider.provider_trees[1][0] ^= 1;
    let error = bind(&f, Some(&wrong_provider)).expect_err("independent provider mismatch");
    assert!(
        error
            .to_string()
            .contains("external provider identity differs"),
        "{error}"
    );
}

#[test]
fn exact_generated_entry_file_is_pinned_until_the_actual_graph_owner_drops() {
    let f = fixture_with_entry();
    let graph = bind(&f, Some(&qualification())).expect("inert internal fixture receipt");
    let path = f.root.join(ORIGINAL_RUNTIME_ENTRY_PATH);
    assert_eq!(
        graph
            .windows_runtime_entry(&f.root, Path::new(MAIN))
            .expect("same owner"),
        path
    );
    assert!(
        graph
            .windows_runtime_entry(&f.root.join("other"), Path::new(MAIN))
            .is_err()
    );
    assert!(
        graph
            .windows_runtime_entry(&f.root, Path::new("other.mjs"))
            .is_err()
    );
    assert!(
        fs::write(&path, b"replacement").is_err(),
        "actual FILE_SHARE_READ denies writes"
    );
    assert!(
        fs::remove_file(&path).is_err(),
        "actual held file denies deletion"
    );
    drop(graph);
    fs::write(&path, b"replacement").expect("last entry owner actually released");
}

#[test]
fn a_self_consistent_graph_cannot_change_or_omit_the_canonical_entry() {
    let f = fixture_with_entry();
    Fixture::write(
        &f.root,
        ORIGINAL_RUNTIME_ENTRY_PATH,
        b"// foreign self-consistent source\n",
    );
    let error = bind(&f, Some(&qualification())).expect_err("graph hash is not template authority");
    assert!(
        error
            .to_string()
            .contains("graph does not pin canonical bytes"),
        "{error}"
    );
    fs::remove_file(f.root.join(ORIGINAL_RUNTIME_ENTRY_PATH)).expect("no retained admitted owner");
    let error = bind(&f, Some(&qualification())).expect_err("entry missing");
    assert!(
        error.to_string().contains("canonical entry is absent"),
        "{error}"
    );
}

#[test]
fn retained_root_and_original_digest_are_checked_before_graph_admission() {
    let f = fixture_with_entry();
    let bytes = f.compile().expect("fixture graph");
    fs::write(f.root.join(GRAPH_FILE), &bytes).expect("same graph");
    let error = bind_with_qualification(
        &f.root,
        &f.held,
        [0; 32],
        Path::new(MAIN),
        &f.selected,
        Some(&qualification()),
    )
    .expect_err("wrong compiled digest");
    assert!(error.to_string().contains("graph hash differs"), "{error}");
    let other = Fixture::new();
    let error = bind_with_qualification(
        &other.root,
        &f.held,
        Sha256::digest(&bytes).into(),
        Path::new(MAIN),
        &f.selected,
        Some(&qualification()),
    )
    .expect_err("a foreign retained root cannot select this graph");
    assert!(
        error.to_string().contains("root was substituted"),
        "{error}"
    );
}

fn prepared(f: &Fixture) -> crate::app_session::original_webapp::PreparedOriginalSource {
    prepared_with_selector(f, "original-editor-offline")
}

fn prepared_with_selector(
    f: &Fixture,
    selector: &str,
) -> crate::app_session::original_webapp::PreparedOriginalSource {
    use crate::app_session::{load_verified_manifest, original_webapp::PreparedOriginalSource};
    use std::sync::Arc;
    let policy = serde_json::json!({
        "profile":"legacy", "windows":{"editor":{"web":{"csp":selector}}}
    });
    let bytes = serde_json::to_vec(&policy).expect("exact policy fixture bytes");
    let policy_path = f.owned.path().join("verified-policy.json");
    fs::write(&policy_path, &bytes).expect("same policy fixture");
    let verified = load_verified_manifest(
        File::open(&policy_path).expect("actual policy file"),
        policy_path,
        Sha256::digest(&bytes).into(),
    )
    .expect("verified immutable source");
    let policy = verified
        .resolve_unique_original_window_csp()
        .expect("actual unique CSP");
    PreparedOriginalSource::prepare(
        Arc::new(bind(f, Some(&qualification())).expect("retained fixture graph")),
        keld_guard::ProfileState::Legacy,
        policy,
        keld_wv::preload_image::AppPreloadBudget::default(),
    )
    .expect("same graph/policy/budget source")
}

#[test]
fn prepared_original_requires_actual_versions_and_moves_the_supplied_facts_unchanged() {
    use crate::app_session::windows_boot::bind_runtime_facts;
    use std::collections::BTreeMap;
    let f = fixture_with_entry();
    let Err(error) = bind_runtime_facts(Some(prepared(&f)), None) else {
        panic!("missing authenticated facts cannot create an original selection");
    };
    assert!(
        error
            .to_string()
            .contains("no pre-app Runtime process facts"),
        "{error}"
    );
    // Metadata unit input only. Authentication itself belongs to the existing
    // Runtime Bound owner, not this control or these deliberately inert values.
    let facts = BTreeMap::from([
        ("bun".to_owned(), "unit-input-bun".to_owned()),
        ("keld".to_owned(), "unit-input-host".to_owned()),
    ]);
    let selected = bind_runtime_facts(Some(prepared(&f)), Some(&facts))
        .expect("actual supplied metadata")
        .expect("same original selected");
    assert_eq!(selected.runtime_versions(), &facts);
    assert_eq!(selected.window_key(), "editor");
}

#[test]
fn runtime_source_recheck_observes_a_real_changed_original_before_reprovision() {
    use keld_runtime::primary::RoleSourceRecheck;
    let f = fixture_with_entry();
    let graph = bind(&f, Some(&qualification())).expect("same inert retained source");
    RoleSourceRecheck::recheck(&graph).expect("unchanged actual graph");
    // The original source is mutable under Legacy. A remembered graph hash
    // cannot approve a later generation after its real bytes changed.
    fs::write(
        f.root.join(MAIN),
        b"// changed actual original before recovery\n",
    )
    .expect("change the source between attempts");
    assert!(RoleSourceRecheck::recheck(&graph).is_err());
    fs::write(f.root.join(MAIN), b"// Inert app source; never execute.\n")
        .expect("restore exact fixture bytes");
    RoleSourceRecheck::recheck(&graph).expect("restored bytes match the same retained graph");
}

#[test]
fn original_development_descriptor_uses_the_same_retained_graph_policy_and_source() {
    use crate::app_session::launch_facts::DevelopmentLaunchFacts;
    use crate::app_session::windows_boot::bind_runtime_facts;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    for selector in [
        "original-editor-offline",
        "original-editor-offline-empty-style",
    ] {
        let mut f = fixture_with_entry();
        f.root = f
            .root
            .canonicalize()
            .expect("actual retained Windows spelling");
        let prepared = prepared_with_selector(&f, selector);
        let source = Arc::clone(prepared.source());
        let facts = DevelopmentLaunchFacts::from_validated_development(
            "Exact λ",
            &f.root,
            Some("31.7.0-selected"),
        )
        .expect("explicit metadata")
        .expect("canonical local drive");
        let generic = facts
            .bun_define_argument()
            .expect("unchanged generic facts");
        let decode = |argument: &str| -> serde_json::Value {
            serde_json::from_str(
                argument
                    .strip_prefix("--define=__KELD_HOST_LAUNCH_FACTS__:")
                    .expect("one private definition"),
            )
            .expect("closed JSON")
        };
        let before = decode(&generic);
        assert_eq!(
            before,
            serde_json::json!({
                "schema":3, "kind":"development", "name":"Exact λ",
                "root":f.root.to_str().expect("UTF-8 fixture"),
                "version":"31.7.0-selected", "paths":{}
            })
        );
        let first = facts
            .original_bun_define_argument(&f.root, &prepared)
            .expect("same prepared owner");
        let expected_policy = serde_json::json!({
            "contract":"gh473-original-editor-offline/v1",
            "graphSha256":format!("sha256:{:x}", Sha256::digest(
                fs::read(f.root.join(GRAPH_FILE)).expect("actual graph bytes")
            )),
            "policySha256":format!("sha256:{:x}", Sha256::digest(
                fs::read(f.owned.path().join("verified-policy.json"))
                    .expect("actual verified policy bytes")
            )),
            "windowKey":"editor", "selector":selector
        });
        let mut expected = before;
        expected["schema"] = serde_json::json!(8);
        expected["originalResourcePolicy"] = expected_policy;
        assert_eq!(decode(&first), expected);
        assert_eq!(facts.bun_define_argument().expect("still generic"), generic);
        assert_eq!(
            facts
                .original_bun_define_argument(&f.root, &prepared)
                .expect("reused capture"),
            first,
        );
        let versions = BTreeMap::from([("bun".to_owned(), "inert-bound-input".to_owned())]);
        let selected = bind_runtime_facts(Some(prepared), Some(&versions))
            .expect("supplied Bound metadata")
            .expect("same original");
        assert!(Arc::ptr_eq(selected.source(), &source));
        assert_eq!(selected.runtime_versions(), &versions);
        assert_eq!(selected.window_key(), "editor");
    }
}

#[test]
fn original_development_join_refuses_foreign_or_missing_captured_facts() {
    use crate::app_session::PrimaryRoleConfig;
    use crate::app_session::launch_facts::{
        DevelopmentLaunchFacts, configure_original_primary_launch,
    };
    let mut f = fixture_with_entry();
    f.root = f
        .root
        .canonicalize()
        .expect("actual retained Windows spelling");
    let prepared = prepared(&f);
    let make = |root: &Path| {
        DevelopmentLaunchFacts::from_validated_development("Exact", root, None)
            .expect("explicit metadata")
            .expect("local drive")
    };
    let facts = make(&f.root);
    let foreign_root = f.root.join("foreign");
    let foreign = make(&foreign_root);
    for (root, captured) in [
        (f.root.as_path(), None),
        (f.root.as_path(), Some(&foreign)),
        (foreign_root.as_path(), Some(&facts)),
        (foreign_root.as_path(), Some(&foreign)),
    ] {
        let error = configure_original_primary_launch(
            PrimaryRoleConfig::new("bun"),
            captured,
            root,
            &prepared,
        )
        .expect_err("one same-root captured owner is required");
        assert_eq!(error.code(), "KELD-CORE-037");
    }
}
