//! Synthetic KEL-78 T1 probe binary.
//!
//! Runs in-process hostile attempts and prints JSON. Exit 0 always: a
//! contained platform is **not** claimed. Direct net is recorded as an OS
//! deny only on `PermissionDenied`, never on `ConnectionRefused`.

#![allow(missing_docs)] // binary entrypoint

use std::io::{self, Write};

use keld_guard::run_synthetic_probes;

fn main() -> io::Result<()> {
    let report = run_synthetic_probes();
    let json = report.to_json();
    let mut stdout = io::stdout().lock();
    stdout.write_all(json.as_bytes())?;
    stdout.write_all(b"\n")?;
    Ok(())
}
