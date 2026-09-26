//! KEL-96/T1b real-macOS no-flag host/window/session acceptance.
#![cfg(target_os = "macos")]
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: assertions are the oracle
#![allow(clippy::zombie_processes)] // cleanup owns host plus the enrolled Bun process group
#![allow(unsafe_code)] // test-only macOS kill(2) group cleanup; local SAFETY proof is at the call

#[path = "no_flag/macos/boot_admission.rs"]
mod boot_admission;
#[path = "no_flag/macos/descriptor_attribution.rs"]
mod descriptor_attribution;
#[path = "no_flag/macos/descriptor_liveness.rs"]
mod descriptor_liveness;
#[path = "no_flag/macos/dev_lifecycle.rs"]
mod dev_lifecycle;
#[path = "no_flag/macos/lifecycle.rs"]
mod lifecycle;
#[cfg(feature = "profile-test-hooks")]
#[path = "no_flag/macos/media/mod.rs"]
mod media;
#[cfg(feature = "profile-test-hooks")]
#[path = "no_flag/macos/profiles/mod.rs"]
mod profiles;
#[path = "no_flag/macos/recovery.rs"]
mod recovery;
#[path = "no_flag/macos/startup_rollback.rs"]
mod startup_rollback;
#[path = "no_flag/macos/support/mod.rs"]
mod support;

use std::io::Read;
use std::os::unix::net::{UnixDatagram, UnixListener};
use std::path::Path;

#[test]
fn keld_dev_helper_process() {
    let Some(project) = std::env::var_os("KELD_T2_HELPER_PROJECT") else {
        return;
    };
    keld_cli::dev::run_dev(Path::new(&project)).expect("shipping keld dev helper");
}

/// Fixture child for the Unix-descriptor census tests.
///
/// Opens exactly one Unix listener of its own, then holds the Unix descriptor
/// the harness passed down as stdin until the harness closes the far end. That
/// read is the release signal, so the census always observes a live process
/// instead of racing its exit. Returns immediately when the harness did not
/// select it, like [`keld_dev_helper_process`].
///
/// `KELD_T2_CENSUS_ANONYMOUS_SOCKET` additionally opens an unbound socket, which
/// `lsof` reports without an identity. It is opened before the listener so that
/// the listener's path, which is what the harness waits on, still marks the point
/// where every descriptor this fixture owns is open.
///
/// The harness may also hand down the same inherited socket twice, on stdin and
/// stdout, to stand in for a `dup` of an inherited descriptor.
#[test]
fn unix_descriptor_census_fixture_process() {
    let Some(owned) = std::env::var_os("KELD_T2_CENSUS_OWNED_SOCKET") else {
        return;
    };
    let _anonymous = std::env::var_os("KELD_T2_CENSUS_ANONYMOUS_SOCKET")
        .map(|_| UnixDatagram::unbound().expect("census fixture unbound Unix socket"));
    let _owned = UnixListener::bind(Path::new(&owned)).expect("census fixture Unix listener");
    let mut released = Vec::new();
    std::io::stdin()
        .read_to_end(&mut released)
        .expect("hold the inherited Unix descriptor until the harness releases it");
}
