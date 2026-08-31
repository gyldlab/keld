//! Operator-launched KEL-101/T4 foreign-user DACL probe.
//!
//! Run this example under the pre-provisioned ordinary Windows account. It
//! receives only the public pipe name, never the HELLO token. Standard output
//! is the machine-readable receipt consumed by `kel101_t4_server`.

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{self, Write as _};
    use std::os::windows::fs::OpenOptionsExt as _;

    use keld_ipc::WindowsNamedPipeBootstrapStream;
    use windows_permissions::constants::{SeObjectType, SecurityInformation};
    use windows_permissions::utilities::current_process_sid;
    use windows_permissions::wrappers::{ConvertSidToStringSid, SetSecurityInfo};
    use windows_permissions::{LocalBox, SecurityDescriptor};
    use windows_sys::Win32::Foundation::GENERIC_WRITE;
    use windows_sys::Win32::Storage::FileSystem::WRITE_DAC;

    const ABSENT_ENDPOINT: &str =
        r"\\.\pipe\keld-0000000000000000000000000000000000000000000000000000000000000000";

    let mut args = std::env::args().skip(1);
    let endpoint = args
        .next()
        .ok_or("usage: kel101_foreign_user_probe <live \\\\.\\pipe\\keld-... endpoint>")?;
    let receipt_path = args.next().ok_or(
        "usage: kel101_foreign_user_probe <live \\\\.\\pipe\\keld-... endpoint> <receipt-path>",
    )?;
    let host_sid = args.next().ok_or(
        "usage: kel101_foreign_user_probe <live \\\\.\\pipe\\keld-... endpoint> <receipt-path> <host-sid>",
    )?;
    if args.next().is_some() {
        return Err("kel101_foreign_user_probe accepts exactly three arguments".into());
    }
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
    let sid_text = sid.to_string_lossy();
    if sid_text.eq_ignore_ascii_case(&host_sid) {
        return Err("foreign probe SID equals the supplied host SID".into());
    }
    let executable = std::env::current_exe()?;
    let receipt = format!(
        "schema=keld.kel101-foreign-user/v1\nsid={}\nendpoint={endpoint}\nnegative_raw_os_error=2\nnegative_kind=NotFound\nlive_raw_os_error=5\nlive_kind=PermissionDenied\npid={}\nexecutable={}\n",
        sid_text,
        std::process::id(),
        executable.display()
    );
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .access_mode(GENERIC_WRITE | WRITE_DAC)
        .create_new(true)
        .open(&receipt_path)?;
    let descriptor: LocalBox<SecurityDescriptor> =
        format!("O:{sid_text}D:P(A;;FRFW;;;{sid_text})(A;;FR;;;{host_sid})").parse()?;
    SetSecurityInfo(
        &mut file,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
        None,
        None,
        descriptor.dacl(),
        None,
    )?;
    file.write_all(receipt.as_bytes())?;
    file.sync_all()?;
    println!("KELD_T4_PROBE_PID={}", std::process::id());
    println!("KELD_T4_PROBE_EXE={}", executable.display());
    println!("KELD_T4_RECEIPT={receipt_path}");
    println!("Leave this process running until the server verifies its Windows identity.");
    let mut release = String::new();
    io::stdin().read_line(&mut release)?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("kel101_foreign_user_probe requires Windows");
    std::process::exit(2);
}
