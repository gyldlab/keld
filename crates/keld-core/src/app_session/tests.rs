#[cfg(windows)]
use std::cell::Cell;
use std::path::Path;

use std::fs;
#[cfg(windows)]
use std::io::{BufRead as _, BufReader};
#[cfg(target_os = "macos")]
use std::os::unix::fs::{PermissionsExt, symlink};
#[cfg(windows)]
use std::process::{Command, Stdio};

use super::*;

include!("tests/app_identity_and_shared_probes.rs");
include!("tests/session_handover.rs");
include!("tests/windows_profile_and_boot.rs");
include!("tests/windows_signature_and_lease.rs");
include!("tests/boot_schema_and_preflight.rs");
include!("tests/filesystem_guard_and_recovery.rs");
include!("tests/guarded_router_and_quit.rs");
include!("tests/role_shutdown_and_worker.rs");
include!("tests/successor_single_flight.rs");
include!("tests/generation_correlation_and_auth.rs");
include!("tests/filesystem_quiesce.rs");
include!("tests/router_lifecycle.rs");
include!("tests/recovered_events_and_echo.rs");
include!("tests/ready_recovery.rs");
include!("tests/failed_ready_and_cli_lease.rs");
include!("tests/terminal_cleanup_and_windows_shutdown.rs");
