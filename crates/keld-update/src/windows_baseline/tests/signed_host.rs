//! The installed host's embedded expectation anchors executable-located selection through
//! one open handle (KEL-254 A3 §4, task T3 Part B) over real per-user Windows state.
//!
//! The tree's `keld-host.exe` is this linked test executable with a canonical container
//! embedded by keld-pack. The same handle feeds `ExpectedAppIdentity::from_signed_image`
//! and `select_active_package_for_executable`, as the host does with the handle KEL-135
//! verified; Authenticode verification itself is the host's and is not exercised here.

use super::locate::{host_path, open_image, payload_for_identity, refuses};
use super::support::host_package_content_with;
use super::writer::seed_per_user_baseline_with;
use crate::windows_baseline::select_active_package_for_executable;
use crate::{ExpectedAppIdentity, ProvenanceField, UpdateError};

/// The canonical payload a host built for `app_id` and the fixture installation's
/// channel, target and release key embeds.
pub(super) fn fixture_payload(app_id: &str) -> keld_pack::ExpectedAppIdentityPayload {
    let mut identity = crate::tests::expected_identity();
    app_id.clone_into(&mut identity.app_id);
    payload_for_identity(&identity)
}

/// This test executable with a canonical container for `app_id`.
fn host_embedded_for(app_id: &str) -> Vec<u8> {
    let image = std::fs::read(std::env::current_exe().expect("test executable path"))
        .expect("read the test executable image");
    keld_pack::embed_host_identity(&image, &fixture_payload(app_id))
        .expect("the linked test executable is an admissible host image")
}

#[test]
fn the_tree_host_expectation_selects_through_the_same_handle() {
    let fixture = tempfile::tempdir().expect("signed-host selection fixture");
    let trust = seed_per_user_baseline_with(
        fixture.path(),
        &host_package_content_with(&host_embedded_for(
            &crate::tests::expected_identity().app_id,
        )),
    );
    let locator = host_path(&trust, "1.0.0");
    let executable = open_image(&locator);
    let expected = ExpectedAppIdentity::from_signed_image(&executable)
        .expect("the installed host carries one canonical container");
    let selection = select_active_package_for_executable(
        crate::WindowsLocatedImage::Host,
        &locator,
        &executable,
        &expected,
    )
    .expect("the host's own expectation selects its installation");
    assert_eq!(selection.artifact().version, "1.0.0");
    assert_eq!(selection.install_identity(), &trust.installation);
    assert_eq!(
        selection.tree_root(),
        locator.parent().expect("tree root"),
        "the selected tree holds the very host that was read"
    );
}

#[test]
fn a_host_embedded_for_another_app_refuses_on_the_record() {
    let fixture = tempfile::tempdir().expect("foreign-expectation fixture");
    let trust = seed_per_user_baseline_with(
        fixture.path(),
        &host_package_content_with(&host_embedded_for("dev.keld.other")),
    );
    let locator = host_path(&trust, "1.0.0");
    let executable = open_image(&locator);
    let expected = ExpectedAppIdentity::from_signed_image(&executable)
        .expect("the foreign container is itself canonical");
    let error = refuses(
        &trust,
        &locator,
        &executable,
        &expected,
        "the record does not carry the embedded expectation's app id",
    );
    assert_eq!(error.code(), "KELD-UPDATE-003");
    assert!(
        matches!(
            &error,
            UpdateError::ProvenanceMismatch {
                field: ProvenanceField::AppId,
                ..
            }
        ),
        "{error:?}"
    );
}
