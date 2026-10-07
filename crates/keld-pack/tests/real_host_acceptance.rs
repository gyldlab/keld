//! KEL-19 T2 rows on real Windows x64: AC2 loadability and the AC8 signed coverage
//! falsifier (container spec §3 AC2 and AC8, §7 rows 2 and 8).
//!
//! Both rows are ignored by default and fail, never skip, when a variable is absent.
//! `KELD_PACK_REAL_HOST` names the workspace-built release `keld-host.exe`, as for the
//! `real_host` row. AC2 writes the writer's output to `KELD_PACK_EMBEDDED_HOST`, a path
//! that must not exist yet, and launches it. AC8 reads `KELD_PACK_SIGNED_HOST`, a copy of
//! that output that the operator signed once on the KEL-135 signed-fixture path with the
//! program name `keld.app-id/v1:com.example.app`, and runs every row through the single
//! KEL-135 Authenticode verifier owner in `keld-guard`.
//!
//! The container spec's §1 helper amendment also runs these rows on the workspace-built
//! release `keld-updater-helper.exe` (KEL-270 T4d S9c). The helper's AC2 row reads
//! `KELD_PACK_REAL_HELPER` and writes `KELD_PACK_EMBEDDED_HELPER`. AC8 runs unchanged:
//! `KELD_PACK_REAL_HOST` names the release helper, and `KELD_PACK_SIGNED_HOST` names a
//! once-signed copy of its embedded output.
#![cfg(windows)]
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are assertion oracles

use keld_pack::{EXPECTED_APP_IDENTITY_KEY_BYTES, ExpectedAppIdentityPayload};
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;
use std::path::PathBuf;

#[path = "real_host_acceptance/coverage.rs"]
mod coverage;
#[path = "real_host_acceptance/launch.rs"]
mod launch;

/// `TRUST_E_NOSIGNATURE`: the subject carries no signature.
const TRUST_E_NOSIGNATURE: u32 = 0x800B_0100;
/// The `keld-guard` refusal text that carries the exact `WinVerifyTrust` status.
const WIN_VERIFY_TRUST_REJECTED: &str =
    "WinVerifyTrust rejected the current executable with status 0x";
/// The app id of the fixture payload and of the operator's signed program name.
const APP_ID: &str = "com.example.app";

fn env_path(name: &str) -> PathBuf {
    std::env::var_os(name).map_or_else(
        || panic!("{name} must be set; this row never skips"),
        PathBuf::from,
    )
}

fn fixture_payload() -> ExpectedAppIdentityPayload {
    let mut key = [0_u8; EXPECTED_APP_IDENTITY_KEY_BYTES];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = u8::try_from(index).expect("key index fits");
    }
    ExpectedAppIdentityPayload::new(APP_ID, "stable", "windows-x64", key)
        .expect("canonical payload")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            write!(hex, "{byte:02x}").expect("write to a String");
            hex
        })
}
