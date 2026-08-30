//! Platform primary-role coordinator names frozen by KEL-75 T1b/T8.
//!
//! The implementation lives in `role`. T2 reuses that same
//! authenticated coordinator for an independent `app-bound` slot rather than
//! wrapping a second restart loop around [`crate::Supervisor`].

pub use crate::role::{
    BoundRoleGeneration as BoundPrimaryGeneration, RoleConfig as PrimaryRoleConfig,
    RoleEvent as PrimaryRoleEvent, RoleGeneration, RoleOwner,
    RoleRevocationCause as PrimaryRoleRevocationCause, RoleSupervisor as PrimaryRoleSupervisor,
};
