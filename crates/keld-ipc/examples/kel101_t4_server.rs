//! Interactive KEL-101/T4 server-side acceptance fixture.
//!
//! This fixture prints only the public named-pipe endpoint. After the operator
//! runs `kel101_foreign_user_probe` under the approved ordinary account, press
//! Enter. The receipt is validated before a same-user authenticated echo and a
//! fresh next generation are exercised with the secret kept in this process.

#[cfg(windows)]
#[derive(Clone)]
struct RecordingObserver(std::sync::Arc<std::sync::Mutex<Vec<keld_ipc::BootstrapRejection>>>);

#[cfg(windows)]
impl keld_ipc::BootstrapRejectionObserver for RecordingObserver {
    fn rejected(&self, rejection: keld_ipc::BootstrapRejection) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(rejection);
    }
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::io;
    use std::path::Path;
    use std::sync::Arc;

    use keld_ipc::{WindowsNamedPipeBootstrapListener, parse_app_link};
    use windows_permissions::utilities::current_process_sid;
    use windows_permissions::wrappers::ConvertSidToStringSid;

    let mut args = std::env::args().skip(1);
    let receipt_path = args
        .next()
        .ok_or("usage: kel101_t4_server <receipt-path> <foreign-user-sid>")?;
    let foreign_sid = args
        .next()
        .ok_or("usage: kel101_t4_server <receipt-path> <foreign-user-sid>")?;
    if args.next().is_some() {
        return Err("kel101_t4_server accepts exactly two arguments".into());
    }
    let receipt_path = Path::new(&receipt_path);
    if receipt_path.exists() {
        return Err("receipt path already exists; choose a fresh path".into());
    }

    let current_process_sid = current_process_sid()?;
    let current_sid = ConvertSidToStringSid(&current_process_sid)?
        .to_string_lossy()
        .into_owned();
    if current_sid.eq_ignore_ascii_case(&foreign_sid) {
        return Err("foreign-user SID equals the current host TokenUser SID".into());
    }

    let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind()?);
    let link = listener.app_link();
    let (endpoint, token) = parse_app_link(&link)?;
    println!("KELD_T4_ENDPOINT={endpoint}");
    println!("KELD_T4_HOST_SID={current_sid}");
    println!("KELD_T4_FOREIGN_SID={foreign_sid}");
    println!("KELD_T4_RECEIPT={}", receipt_path.display());
    println!(
        "Run kel101_foreign_user_probe under the foreign account, redirect its stdout to the receipt, then press Enter."
    );

    let mut confirmation = String::new();
    io::stdin().read_line(&mut confirmation)?;
    validate_receipt(receipt_path, endpoint, &foreign_sid)?;
    run_authorized_and_successor(&listener, endpoint, &token)?;

    println!("KELD_T4_RESULT=passed");
    println!("KELD_T4_FOREIGN_OPEN=ERROR_ACCESS_DENIED(5)");
    println!("KELD_T4_AUTHORIZED_ECHO=passed");
    println!("KELD_T4_STALE_LOCATOR=ERROR_FILE_NOT_FOUND(2)");
    println!("KELD_T4_TOKEN_ROTATION=passed");
    println!("KELD_T4_NEXT_GENERATION=passed");
    Ok(())
}

#[cfg(windows)]
fn run_authorized_and_successor(
    listener: &std::sync::Arc<keld_ipc::WindowsNamedPipeBootstrapListener>,
    endpoint: &str,
    token: &keld_ipc::SessionToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use keld_ipc::{
        WindowsNamedPipeBootstrapListener, WindowsNamedPipeBootstrapStream, parse_app_link,
    };

    let first_rejections = authenticated_echo(
        std::sync::Arc::clone(listener),
        endpoint,
        token,
        "kel101-t4-authorized-after-foreign-denial",
    )?;
    if !first_rejections.is_empty() {
        return Err(format!(
            "foreign DACL denial must not reach admission; observed host rejections: {first_rejections:?}"
        )
        .into());
    }
    let Err(stale_error) = WindowsNamedPipeBootstrapStream::connect(endpoint) else {
        return Err("consumed pipe locator unexpectedly reopened".into());
    };
    if stale_error.raw_os_error() != Some(2) {
        return Err(format!(
            "consumed pipe locator returned raw={:?}, expected ERROR_FILE_NOT_FOUND (2)",
            stale_error.raw_os_error()
        )
        .into());
    }
    let successor = WindowsNamedPipeBootstrapListener::bind()?;
    let successor_link = successor.app_link();
    let (successor_endpoint, successor_token) = parse_app_link(&successor_link)?;
    if successor_endpoint == endpoint
        || !WindowsNamedPipeBootstrapStream::is_keld_endpoint(successor_endpoint)
    {
        return Err("successor did not mint a fresh exact Keld pipe endpoint".into());
    }
    if successor_token == *token {
        return Err("successor reused the consumed generation's HELLO token".into());
    }
    let successor_rejections = authenticated_echo(
        std::sync::Arc::new(successor),
        successor_endpoint,
        &successor_token,
        "kel101-t4-successor-authorized",
    )?;
    if !successor_rejections.is_empty() {
        return Err(format!(
            "successor authorized echo observed unexpected rejections: {successor_rejections:?}"
        )
        .into());
    }
    Ok(())
}

#[cfg(windows)]
fn authenticated_echo(
    listener: std::sync::Arc<keld_ipc::WindowsNamedPipeBootstrapListener>,
    endpoint: &str,
    token: &keld_ipc::SessionToken,
    message: &str,
) -> Result<Vec<keld_ipc::BootstrapRejection>, Box<dyn std::error::Error + Send + Sync>> {
    use keld_ipc::link::AppLinkDeadlines as _;
    use keld_ipc::{
        APP_LINK_IO_DEADLINE, EchoRequest, WindowsNamedPipeBootstrapStream, echo_call,
        serve_echo_requests,
    };

    let rejections = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recording_observer = RecordingObserver(std::sync::Arc::clone(&rejections));
    let worker = std::thread::spawn(
        move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let Some(mut stream) = listener.accept_authenticated(&recording_observer)? else {
                return Err("T4 listener stopped before authorized authentication".into());
            };
            serve_echo_requests(&mut stream)?;
            Ok(())
        },
    );

    let mut client = WindowsNamedPipeBootstrapStream::connect(endpoint)?;
    client.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))?;
    let request = EchoRequest {
        message: message.to_owned(),
        count: 1,
    };
    let response = echo_call(&mut client, &request, token)?;
    if response.message != request.message || response.count != request.count {
        return Err("authorized echo response did not preserve the request".into());
    }
    drop(client);
    worker
        .join()
        .map_err(|_| "T4 authorized server worker panicked")??;
    let records = rejections
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    Ok(records)
}

#[cfg(windows)]
fn validate_receipt(
    path: &std::path::Path,
    endpoint: &str,
    foreign_sid: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use windows_permissions::constants::{SeObjectType, SecurityInformation};
    use windows_permissions::wrappers::GetNamedSecurityInfo;

    let expected_owner =
        foreign_sid.parse::<windows_permissions::LocalBox<windows_permissions::Sid>>()?;
    let descriptor = GetNamedSecurityInfo(
        path,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner,
    )?;
    if descriptor.owner() != Some(&expected_owner) {
        return Err("foreign-user receipt file owner does not match the expected SID".into());
    }
    let body = std::fs::read_to_string(path)?;
    let mut fields = std::collections::BTreeMap::new();
    for line in body.lines() {
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("foreign-user receipt line has no separator: {line:?}").into());
        };
        if fields.insert(key, value).is_some() {
            return Err(format!("foreign-user receipt duplicates field {key:?}").into());
        }
    }
    let required = [
        ("schema", "keld.kel101-foreign-user/v1"),
        ("sid", foreign_sid),
        ("endpoint", endpoint),
        ("negative_raw_os_error", "2"),
        ("negative_kind", "NotFound"),
        ("live_raw_os_error", "5"),
        ("live_kind", "PermissionDenied"),
    ];
    for (key, expected) in required {
        if fields.get(key).copied() != Some(expected) {
            return Err(format!(
                "foreign-user receipt field {key:?} was {:?}, expected {expected:?}",
                fields.get(key)
            )
            .into());
        }
    }
    if fields.len() != required.len() {
        return Err(format!(
            "foreign-user receipt has {} fields, expected exactly {}",
            fields.len(),
            required.len()
        )
        .into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("kel101_t4_server requires Windows");
    std::process::exit(2);
}
