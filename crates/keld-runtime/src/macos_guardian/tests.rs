use super::{
    CONTROL_CLEAR, CONTROL_CLEARED, CONTROL_PREPARE, CONTROL_PREPARED, CONTROL_REVOKE,
    CONTROL_REVOKED, ChildPreparer, GENERATION_CONTROL_POLL, GUARDIAN_SHUTDOWN_DEADLINE,
    GenerationControlPeer, GenerationControlReader, GenerationControlRecord,
    GenerationProvisionFence, GuardedPrimaryLease, GuardedPrimaryPreparer, GuardedPrimaryUpdate,
    PreparedChild, RecoveryState, RegisteredPrimaryGeneration, RestartPolicy, RevocationCause,
    RoleEvent, RuntimeError, SUPERVISED_QUIT_ACCEPTED, Supervisor, SupervisorOutcome,
    is_owned_preparation_interruption, lifecycle_error, lock_generation, lock_recovery,
    observe_guarded_liveness, parse_app_link, run_generation_host, runtime_io_kind,
    set_liveness_nonblocking, validate_liveness_bootstrap, write_generation_control,
};

mod generation_control_wire;

mod recovery_and_revocation;

mod preparation_fixture;

mod generation_host;

mod supervised_bootstrap;

mod generic_pre_registration_crash;
mod guardian_crash_pre_registration;
mod initial_generation_adversaries;
mod liveness_and_shutdown;
mod pre_registration_host_death;
mod process_group_failsafe;
mod registration_record_adversaries;
mod rejected_registration_deadline;
pub(super) mod retained_ack;

mod active_generation_shutdown;
mod fatal_cleanup_custody;
mod guarded_primary_shutdown;
mod host_guardian_failed_custody;
mod worker_outcome;

pub(super) mod bootstrap_shutdown_race;
mod control_acknowledgement;

mod terminal_liveness;

// Existing approved KEL-143 §7 schedules. This test relay copies exact
// private records; run_generation_host remains the actual mint/lease owner.
mod runtime_race_validation {
    use super::{
        CONTROL_CLEAR, CONTROL_CLEARED, CONTROL_PREPARE, CONTROL_PREPARED, CONTROL_REVOKE,
        CONTROL_REVOKED, ChildPreparer, GENERATION_CONTROL_POLL, GUARDIAN_SHUTDOWN_DEADLINE,
        GenerationControlPeer, GenerationControlReader, GenerationControlRecord,
        GenerationProvisionFence, GuardedPrimaryLease, GuardedPrimaryPreparer,
        GuardedPrimaryUpdate, PreparedChild, RecoveryState, RegisteredPrimaryGeneration,
        RestartPolicy, RevocationCause, RoleEvent, RuntimeError, SUPERVISED_QUIT_ACCEPTED,
        Supervisor, SupervisorOutcome, assert_gone, is_owned_preparation_interruption,
        lifecycle_error, liveness_pipe, lock_generation, lock_recovery, observe_guarded_liveness,
        parse_app_link, run_generation_host, runtime_io_kind, set_liveness_nonblocking,
        validate_liveness_bootstrap, write_generation_control,
    };

    mod records_and_cut_types;
    use self::records_and_cut_types::{COLLECTION, receive_record, timed_pair};
    mod control_plane;
    mod delivery_assertions;
    mod delivery_cut_oracles;
    mod delivery_fixture;
    mod ownership_races;
    mod private_record_relay;
}

mod test_support;
use self::test_support::{assert_gone, liveness_pipe, long_running_command};
