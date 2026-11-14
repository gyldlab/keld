//! GH-1185 Q22 native feasibility observations, not Keld file-navigation acceptance.
//!
//! Native selectors are opt-in because the one Windows desktop must be claimed first.
//! The observer tests also run without a Windows desktop or `profile-test-hooks`.

#[path = "fixtures/windows_file_native_observation.rs"]
mod observation;

#[cfg(all(target_os = "windows", feature = "profile-test-hooks"))]
#[path = "fixtures/windows_file_native_probe.rs"]
mod native;

#[cfg(all(target_os = "windows", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires the claimed Windows native slot and real WebView2; creates one owned window/profile"]
fn windows_file_request_header_attribution() {
    native::run_in_child().expect("complete native observations and normal resource cleanup");
}

#[cfg(all(target_os = "windows", feature = "profile-test-hooks"))]
#[test]
#[ignore = "private exact-selector child for windows_file_request_header_attribution"]
fn windows_file_native_probe_child() {
    native::child_entry().expect("native helper records its own receipt, including any failure");
}
