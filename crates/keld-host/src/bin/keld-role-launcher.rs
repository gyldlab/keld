//! Private Linux strict-profile launcher shipped beside `keld-host`.

#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = keld_runtime::linux_strict::run_linux_strict_launcher() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("keld-role-launcher is available only on Linux");
    std::process::exit(1);
}
