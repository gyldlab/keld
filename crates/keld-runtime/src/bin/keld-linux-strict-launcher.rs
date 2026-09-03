//! Private in-sandbox launcher for KEL-78/T4 Linux strict roles.

#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = keld_runtime::linux_strict::run_linux_strict_launcher() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("KELD-RUNTIME-016: the Linux strict launcher is unavailable on this platform.");
    std::process::exit(1);
}
