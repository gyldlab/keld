//! macOS host-death guardian for supervised Bun process groups.
//!
//! This is supervisor cleanup, not App Sandbox containment. The caller owns
//! the guardian process and the only liveness writer. The guardian process
//! calls [`run`] for the generic one-group API or [`run_guarded_primary`] for
//! KEL-96's persistent fresh-generation primary owner.

#![deny(unsafe_op_in_unsafe_fn)]

mod guarded_primary;
pub use self::guarded_primary::{GuardedPrimary, GuardedPrimaryUpdate};
use self::guarded_primary::{RegisteredPrimaryGeneration, lock_generation};

mod error;
use self::error::{collapse_failures, lifecycle_error, nix_io_error};

mod recovery_state;
use self::recovery_state::{RecoveryState, lock_recovery};

mod host_liveness;
#[cfg(test)]
use self::host_liveness::observe_guarded_liveness;

mod generation_wire;

mod host_control;

mod process_group;
#[cfg(test)]
use self::process_group::terminate_registered_group;
use self::process_group::{
    reap_pending_guardian, reject_host_registration, validate_group_leader, validate_group_pid,
};

mod spawn_admission;
use self::spawn_admission::SpawnAdmission;

mod registration_ack;
use self::registration_ack::{AwaitHostGroupRetention, HOST_GROUP_RETAINED_ACK};

mod authenticated_runner;
use self::authenticated_runner::run_with_authenticated_registration;

mod guarded_lease;
#[cfg(test)]
use self::guarded_lease::GuardedPrimaryLease;
use self::guarded_lease::GuardedPrimaryPreparer;

mod supervised_lease;
use self::supervised_lease::SupervisedGuardianPreparer;

mod registered_guardian;
pub use self::registered_guardian::HostGuardian;

mod bootstrap;
pub use self::bootstrap::GuardianBootstrap;

mod supervision_runner;
pub use self::supervision_runner::run_guarded_primary;
#[cfg(test)]
use self::supervision_runner::run_with_registration;
use self::supervision_runner::{
    connect_guardian_registration, fail_before_child, run_supervised_with_registration,
    write_group_registration,
};

mod generation_host;
use self::generation_host::run_generation_host;
#[cfg(test)]
use self::generation_host::{
    GenerationProvisionFence, HostGenerationState, check_initial_recovery_decision,
    finish_generation_control_eof, runtime_io_kind,
};

mod supervised_ack;
use self::supervised_ack::{
    SUPERVISED_ORDERLY_ACK, SUPERVISED_ORDERLY_SHUTDOWN, SUPERVISED_QUIT_ACCEPTED,
    SUPERVISED_QUIT_ACK, SUPERVISED_QUIT_ACK_DEADLINE, read_supervised_orderly_ack,
    read_supervised_quit_ack,
};

#[cfg(test)]
use self::host_control::interrupted_successor_preparation;
pub(crate) use self::host_control::is_owned_preparation_interruption;
use self::host_control::{
    GenerationControlPeer, control_exchange, control_exchange_with_cancellation,
};

use self::generation_wire::{
    CONTROL_CLEAR, CONTROL_CLEARED, CONTROL_PREPARE, CONTROL_PREPARED, CONTROL_REGISTERED,
    CONTROL_REVOKE, CONTROL_REVOKED, CONTROL_SPAWNED, GenerationControlReader,
    GenerationControlRecord, decode_revocation_cause, validate_control_response,
    write_generation_control,
};
#[cfg(test)]
use self::generation_wire::{GENERATION_CONTROL_RECORD_LEN, read_generation_control};

use self::host_liveness::{
    await_guarded_primary_liveness, await_host_death, observe_supervised_liveness,
    require_close_on_exec, set_liveness_nonblocking, validate_liveness_bootstrap,
};

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use keld_ipc::link::handshake_client;
use keld_ipc::{
    BootstrapAdmission, BootstrapListener, BootstrapRejection, BootstrapRejectionObserver,
    parse_app_link,
};

use crate::role::{
    BoundRoleGeneration, DEFAULT_ADMISSION_TIMEOUT, RoleEvent, RoleGenerationLease,
    RoleGenerationOwner, RoleOwner,
};
use crate::{
    CapturedOutput, ChildPreparer, CrashLedger, GenerationLease, PreparedChild, RestartPolicy,
    RevocationCause, RuntimeError, Supervisor, SupervisorOutcome,
};

const REGISTRATION_ENV: &str = "KELD_INTERNAL_MACOS_GUARDIAN_REGISTRATION";
const REGISTRATION_MAGIC: [u8; 4] = *b"KGR1";
const REGISTRATION_LEN: usize = 8;
const GUARDIAN_SHUTDOWN_DEADLINE: Duration = Duration::from_secs(5);
const GENERATION_CONTROL_POLL: Duration = Duration::from_millis(20);

/// Private argv discriminator for the KEL-96 supervised guardian re-exec.
///
/// It carries no authority: authenticated registration and the inherited
/// liveness descriptor must still validate before Bun can spawn.
pub const SUPERVISED_GUARDIAN_ARG: &str = "--keld-internal-macos-guardian-v1";

/// Observable completion of one guardian-owned Bun process group.
#[derive(Debug)]
pub struct GuardianReport {
    leader_pid: u32,
    leader_status: ExitStatus,
}

/// Runs one guardian-owned child group until the host-liveness reader reaches
/// EOF, then revokes registered link resources, signals the enrolled process
/// group, and waits the direct child.
///
/// `command` is placed in a new process group before spawn. Descendants inherit
/// that group unless they explicitly break away; a strict-profile consumer
/// must separately prove that attempted `setpgid`/`setsid` escape fails.
/// `liveness` must carry no bytes: EOF is the only accepted host-death signal.
/// The revocation callback runs before group termination, and an error from it
/// blocks successful cleanup even though the group is still terminated and
/// waited.
/// The module authenticates to the host-minted private registration link before
/// child creation and emits the fixed `KGR1` group record after spawn. Callers
/// cannot substitute a numeric group id.
///
/// # Errors
///
/// Returns a typed [`RuntimeError`] when the command is empty, spawning fails,
/// the liveness reader carries bytes or errors, revocation fails, the group
/// cannot be signaled, or the direct child cannot be waited.
pub fn run<R, F>(
    command: Command,
    liveness: R,
    revoke_registered_resources: F,
) -> Result<GuardianReport, RuntimeError>
where
    R: Read + AsFd,
    F: FnOnce() -> io::Result<()>,
{
    let registration = match connect_guardian_registration() {
        Ok(registration) => registration,
        Err(error) => return Err(fail_before_child(error, revoke_registered_resources)),
    };
    run_with_authenticated_registration(
        command,
        liveness,
        AwaitHostGroupRetention::new(registration),
        revoke_registered_resources,
    )
}

/// Runs one non-restarting guardian-owned [`Supervisor`] until host EOF.
///
/// The guardian authenticates its private group-registration link and validates
/// the live host-liveness pipe before starting the supervisor. The supervisor
/// remains the sole Bun spawn/capture/KEL-116-ledger/wait owner. Its prepared
/// generation lease registers the exact Bun group and revokes resources plus
/// signals that group before supervisor kill/wait. A first unrequested child
/// termination is fatal; [`run_guarded_primary`] owns the KEL-96/T3
/// multi-generation path.
/// `quit_ack` is a private dedicated writer: after the guardian reads the
/// accepted-Quit control and updates Supervisor attribution, it writes fixed
/// `KQA`. The host must observe that ack before publishing the Quit reply.
///
/// # Errors
///
/// Returns [`RuntimeError`] for invalid private bootstrap, initial spawn or
/// registration failure, host-liveness failure, revocation/group cleanup
/// failure, or any unrequested Bun self-termination.
pub fn run_supervised<R, C, A, F>(
    liveness: R,
    command_factory: C,
    quit_ack: A,
    revoke_registered_resources: F,
) -> Result<CapturedOutput, RuntimeError>
where
    R: Read + AsFd,
    C: FnOnce() -> Result<Command, RuntimeError> + Send + 'static,
    A: Write,
    F: FnOnce() -> io::Result<()> + Send + 'static,
{
    let registration = match connect_guardian_registration() {
        Ok(registration) => registration,
        Err(error) => return Err(fail_before_child(error, revoke_registered_resources)),
    };
    run_supervised_with_registration(
        liveness,
        AwaitHostGroupRetention::new(registration),
        quit_ack,
        command_factory,
        |_| Ok(()),
        revoke_registered_resources,
    )
    .map(|(_, _, output)| output)
}

#[cfg(test)]
mod tests;
