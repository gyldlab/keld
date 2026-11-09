//! Platform primary-role coordinator names frozen by KEL-75 T1b/T8.
//!
//! The implementation lives in `role`. T2 reuses that same
//! authenticated coordinator for an independent `app-bound` slot rather than
//! wrapping a second restart loop around [`crate::Supervisor`].

pub use crate::role::{
    BUN_OPTIONS_ENV, BoundRoleGeneration as BoundPrimaryGeneration,
    RoleConfig as PrimaryRoleConfig, RoleEvent as PrimaryRoleEvent, RoleGeneration, RoleOwner,
    RolePrincipal, RoleRecoveryGate as PrimaryRecoveryGate,
    RoleRevocationCause as PrimaryRoleRevocationCause, RoleSupervisor as PrimaryRoleSupervisor,
    admit_staged_transport_sidecar, staged_bun_launch_args, strip_verbatim_prefix,
};
