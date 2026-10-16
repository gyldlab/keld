//! Native Windows acceptance for protected update root identity and topology.
//!
//! Authenticated parent/child roots must not be substituted by weak ACLs or
//! reparse links. A literal tilde in an existing admitted root remains valid.

use std::fs::{self, File};
use std::os::windows::fs::symlink_dir;
use std::path::Path;

use cap_std::ambient_authority;

use super::{
    Fixture, GOLDEN_TAR, admitted_for, assert_empty_versions, expect_extraction, signing_key,
};

#[test]
fn owner_private_root_refuses_install_profile_and_topology_substitution() {
    let fixture = Fixture::new();
    let install = fixture.temp.path().join("unprotected-install");
    let update = install.join("updates");
    fs::create_dir(&install).expect("ordinary inherited install fixture");
    fs::create_dir(&update).expect("ordinary inherited update fixture");
    fs::create_dir(update.join("versions")).expect("ordinary inherited versions fixture");

    let mut identity = fixture.identity.clone();
    identity.install_root = install;
    identity.update_root = update.clone();
    let admitted = admitted_for(identity.clone(), &signing_key(), "1.0.0");
    assert!(
        admitted.open_windows_extraction_root().is_err(),
        "matching topology cannot compensate for a missing owner-private install profile"
    );

    identity.update_root = fixture.temp.path().join("sibling-updates");
    let admitted = admitted_for(identity, &signing_key(), "1.0.0");
    assert!(
        admitted.open_windows_extraction_root().is_err(),
        "a sibling update root cannot substitute for the trusted direct child"
    );
}

#[test]
fn tilde_in_existing_update_root_preserves_authenticated_extraction() {
    let mut fixture = Fixture::new();
    let renamed = fixture.identity.install_root.join("updates~stable");
    fs::rename(&fixture.identity.update_root, &renamed)
        .expect("rename preserves protected directory descriptors");
    fixture.identity.update_root = renamed;
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("existing filesystem roots permit a literal tilde");
    let stage = root
        .extract(&receipt, &source)
        .expect("authenticated extraction beneath existing tilde root");
    let stage_path = fixture.versions().join(stage.name());
    assert_eq!(
        fs::read(stage_path.join("content.tar")).expect("exact retained content"),
        GOLDEN_TAR
    );
    assert_eq!(
        fs::read(stage_path.join("tree/nest/one")).expect("extracted payload"),
        b"!"
    );
    assert!(!stage_path.join(".complete").exists());
}

#[test]
fn missing_or_noncanonical_protected_root_refuses_without_creating_stage() {
    let missing = Fixture::new();
    fs::remove_dir(missing.versions()).expect("remove unused versions root");
    let error = missing
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("missing versions accepted");
    expect_extraction(error, false);

    let permissive = Fixture::new();
    fs::remove_dir(permissive.versions()).expect("remove protected versions root");
    fs::create_dir(permissive.versions()).expect("ordinary inherited versions ACL");
    let error = permissive
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("permissive versions accepted");
    expect_extraction(error, false);
    assert_empty_versions(&permissive.versions());

    let permissive = Fixture::new();
    fs::remove_dir(permissive.versions()).expect("remove protected versions root");
    fs::remove_dir(&permissive.identity.update_root).expect("remove protected update root");
    fs::create_dir(&permissive.identity.update_root).expect("ordinary inherited update ACL");
    fs::create_dir(permissive.versions()).expect("ordinary inherited versions ACL");
    let error = permissive
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("permissive update root accepted");
    expect_extraction(error, false);
    assert_empty_versions(&permissive.versions());
}

#[test]
fn reparse_update_or_versions_root_refuses_before_stage_creation() {
    let versions_link = Fixture::new();
    let target = versions_link.temp.path().join("other-versions");
    fs::create_dir(&target).expect("reparse target");
    fs::remove_dir(versions_link.versions()).expect("replace versions with link");
    symlink_dir(&target, versions_link.versions())
        .expect("real Windows directory reparse fixture requires symlink privilege");
    let error = versions_link
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("reparse versions root accepted");
    expect_extraction(error, false);
    assert_empty_versions(&target);

    let update_link = Fixture::new();
    let target = update_link.temp.path().join("other-update");
    fs::create_dir(&target).expect("reparse target");
    fs::remove_dir(update_link.versions()).expect("empty protected update root");
    fs::remove_dir(&update_link.identity.update_root).expect("replace update with link");
    symlink_dir(&target, &update_link.identity.update_root)
        .expect("real Windows directory reparse fixture requires symlink privilege");
    let error = update_link
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("reparse update root accepted");
    expect_extraction(error, false);
    assert_empty_versions(&target);
}

/// Fixture-only protected directory: the platform ACL is independently checked
/// so a test's expected failure cannot be caused by another weak ancestor.
fn protected_child(parent: &File, name: &str) -> File {
    let directory = crate::windows_fs::create_directory_relative(parent, name)
        .expect("create exact owner-private directory");
    keld_guard::validate_windows_owner_private_directory(&directory)
        .expect("fixture child has the intended strict ACL");
    directory
}

fn retained_fixture_parent(path: &Path) -> File {
    cap_std::fs::Dir::open_ambient_dir(path, ambient_authority())
        .expect("retain controlled fixture parent")
        .into_std_file()
}

#[test]
fn weak_install_refuses_with_protected_update_and_versions() {
    let fixture = Fixture::new();
    let weak_install = fixture.temp.path().join("weak-install");
    fs::create_dir(&weak_install).expect("inherited install ACL");
    let install = retained_fixture_parent(&weak_install);
    assert!(
        keld_guard::validate_windows_owner_private_directory(&install).is_err(),
        "fixture must have a weak install ACL"
    );
    let update = protected_child(&install, "updates");
    let _versions = protected_child(&update, "versions");

    let mut identity = fixture.identity.clone();
    identity.install_root = weak_install;
    identity.update_root = identity.install_root.join("updates");
    let error = admitted_for(identity.clone(), &signing_key(), "1.0.0")
        .open_windows_extraction_root()
        .expect_err("weak install accepted despite independently protected descendants");
    expect_extraction(error, false);
    assert_empty_versions(&identity.update_root.join("versions"));
}

#[test]
fn weak_update_refuses_with_protected_install_and_versions() {
    let fixture = Fixture::new();
    let install = retained_fixture_parent(&fixture.identity.install_root);
    keld_guard::validate_windows_owner_private_directory(&install)
        .expect("install fixture is protected");
    fs::remove_dir(fixture.versions()).expect("remove unused protected versions");
    fs::remove_dir(&fixture.identity.update_root).expect("remove unused protected update");
    fs::create_dir(&fixture.identity.update_root).expect("inherited update ACL");
    let update = retained_fixture_parent(&fixture.identity.update_root);
    assert!(
        keld_guard::validate_windows_owner_private_directory(&update).is_err(),
        "fixture must have a weak update ACL"
    );
    let _versions = protected_child(&update, "versions");

    let error = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("weak update accepted despite protected install and versions");
    expect_extraction(error, false);
    assert_empty_versions(&fixture.versions());
}

#[test]
fn fully_protected_sibling_update_root_cannot_replace_declared_parent() {
    let fixture = Fixture::new();
    let parent = retained_fixture_parent(fixture.temp.path());
    let sibling_install = protected_child(&parent, "sibling-install");
    let sibling_update = protected_child(&sibling_install, "updates");
    let _sibling_versions = protected_child(&sibling_update, "versions");

    let mut identity = fixture.identity.clone();
    identity.update_root = fixture.temp.path().join("sibling-install/updates");
    let error = admitted_for(identity.clone(), &signing_key(), "1.0.0")
        .open_windows_extraction_root()
        .expect_err("protected sibling must not replace the declared install's update child");
    assert!(
        error
            .to_string()
            .contains("direct extraction root topology is invalid"),
        "refusal must come from topology admission, not an unrelated ACL or missing path: {error}"
    );
    assert_empty_versions(&identity.update_root.join("versions"));
}

#[test]
fn reparse_to_protected_target_cannot_substitute_versions_or_update_root() {
    let fixture = Fixture::new();
    let parent = retained_fixture_parent(&fixture.identity.update_root);
    let _target_versions = protected_child(&parent, "protected-target-versions");
    let versions_target = fixture
        .identity
        .update_root
        .join("protected-target-versions");
    fs::remove_dir(fixture.versions()).expect("remove unneeded protected versions");
    symlink_dir("protected-target-versions", fixture.versions())
        .expect("native relative versions symlink");
    let error = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("versions reparse to an otherwise valid protected target accepted");
    expect_extraction(error, false);
    assert_empty_versions(&versions_target);

    let fixture = Fixture::new();
    let parent = retained_fixture_parent(&fixture.identity.install_root);
    let target_update = protected_child(&parent, "protected-target-update");
    let _target_versions = protected_child(&target_update, "versions");
    let update_target = fixture
        .identity
        .install_root
        .join("protected-target-update");
    fs::remove_dir(fixture.versions()).expect("remove original versions");
    fs::remove_dir(&fixture.identity.update_root).expect("remove original update");
    symlink_dir("protected-target-update", &fixture.identity.update_root)
        .expect("native relative update symlink");
    let error = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("update root reparse to an otherwise valid protected target accepted");
    expect_extraction(error, false);
    assert_empty_versions(&update_target.join("versions"));
}
