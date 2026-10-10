//! Independent read-only macOS process-image census for the product fixture.
use std::fs;
use std::path::PathBuf;

pub(super) fn process_image_path(pid: u32) -> PathBuf {
    use std::os::unix::ffi::OsStringExt as _;
    unsafe extern "C" {
        fn proc_pidpath(
            pid: std::ffi::c_int,
            buffer: *mut std::ffi::c_void,
            size: u32,
        ) -> std::ffi::c_int;
    }
    // Xcode macOS SDK 26.5: PROC_PIDPATHINFO_MAXSIZE=4*MAXPATHLEN=4096.
    const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;
    let queried_pid = std::ffi::c_int::try_from(pid).expect("libproc PID domain");
    assert!(
        queried_pid > 0,
        "process-image census needs a live positive PID"
    );
    let mut bytes = [0_u8; PROC_PIDPATHINFO_MAXSIZE];
    // SAFETY: the private test census passes a positive OS-attributed PID and
    // one live, exclusively borrowed writable array of exactly the declared
    // size. libproc writes at most that bounded size and retains no pointer.
    // No ownership transfer, lifetime extension or target-process mutation occurs.
    let returned = unsafe {
        proc_pidpath(
            queried_pid,
            bytes.as_mut_ptr().cast(),
            u32::try_from(bytes.len()).expect("bounded libproc buffer"),
        )
    };
    let error = (returned <= 0).then(std::io::Error::last_os_error);
    let prefix_end =
        usize::try_from(returned).map_or(0, |length| length.saturating_add(1).min(bytes.len()));
    eprintln!(
        "KELD_KEL140_PROCESS_IMAGE_CENSUS pid={pid} returned={returned} error={error:?} raw_bytes={:?}",
        &bytes[..prefix_end]
    );
    assert!(
        returned > 0,
        "kernel process-image query failed for {pid}: {error:?}"
    );
    let length = usize::try_from(returned).expect("positive kernel path length");
    assert!(
        length < bytes.len(),
        "kernel image path has no bounded terminator"
    );
    assert_eq!(bytes[length], 0, "kernel path length must exclude its NUL");
    assert!(
        !bytes[..length].contains(&0),
        "kernel path has an interior NUL"
    );
    let image = PathBuf::from(std::ffi::OsString::from_vec(bytes[..length].to_vec()));
    assert!(
        image.is_absolute(),
        "kernel image path must be absolute: {}",
        image.display()
    );
    image
}

#[test]
fn kernel_process_image_matches_live_test_executable() {
    assert_eq!(
        fs::canonicalize(process_image_path(std::process::id())).expect("kernel test image"),
        fs::canonicalize(std::env::current_exe().expect("known running test executable"))
            .expect("canonical test image")
    );
}
