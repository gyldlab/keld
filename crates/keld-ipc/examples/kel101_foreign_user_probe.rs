//! Operator-launched KEL-101/T4 foreign-user DACL probe.
//!
//! Run this example under the pre-provisioned ordinary Windows account. It
//! receives only the public pipe name, never the HELLO token. Standard output
//! is the machine-readable receipt consumed by `kel101_t4_server`.

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io;

    use keld_ipc::WindowsNamedPipeBootstrapStream;
    use windows_permissions::utilities::current_process_sid;
    use windows_permissions::wrappers::ConvertSidToStringSid;

    const ABSENT_ENDPOINT: &str =
        r"\\.\pipe\keld-0000000000000000000000000000000000000000000000000000000000000000";

    let endpoint = std::env::args()
        .nth(1)
        .ok_or("usage: kel101_foreign_user_probe <live \\\\.\\pipe\\keld-... endpoint>")?;
    if !WindowsNamedPipeBootstrapStream::is_keld_endpoint(&endpoint) {
        return Err("live endpoint is not an exact Keld named-pipe endpoint".into());
    }
    if endpoint == ABSENT_ENDPOINT {
        return Err("live endpoint collided with the negative-control endpoint".into());
    }

    let Err(negative) = WindowsNamedPipeBootstrapStream::connect(ABSENT_ENDPOINT) else {
        return Err("the negative-control pipe unexpectedly opened".into());
    };
    if negative.raw_os_error() != Some(2) || negative.kind() != io::ErrorKind::NotFound {
        return Err(format!(
            "absent-pipe negative control returned raw={:?} kind={:?}, expected ERROR_FILE_NOT_FOUND (2)",
            negative.raw_os_error(),
            negative.kind()
        )
        .into());
    }

    let Err(denied) = WindowsNamedPipeBootstrapStream::connect(&endpoint) else {
        return Err("the foreign user unexpectedly opened the live pipe".into());
    };
    if denied.raw_os_error() != Some(5) || denied.kind() != io::ErrorKind::PermissionDenied {
        return Err(format!(
            "live foreign-user open returned raw={:?} kind={:?}, expected ERROR_ACCESS_DENIED (5)",
            denied.raw_os_error(),
            denied.kind()
        )
        .into());
    }

    let current_sid = current_process_sid()?;
    let sid = ConvertSidToStringSid(&current_sid)?;
    println!("schema=keld.kel101-foreign-user/v1");
    println!("sid={}", sid.to_string_lossy());
    println!("endpoint={endpoint}");
    println!("negative_raw_os_error=2");
    println!("negative_kind=NotFound");
    println!("live_raw_os_error=5");
    println!("live_kind=PermissionDenied");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("kel101_foreign_user_probe requires Windows");
    std::process::exit(2);
}
