//! Unix primary-role coordinator names frozen by KEL-75 T1b.
//!
//! The implementation lives in [`crate::unix_role`]. T2 reuses that same
//! authenticated coordinator for an independent `app-bound` slot rather than
//! wrapping a second restart loop around [`crate::Supervisor`].

pub use crate::unix_role::{
    RoleConfig as PrimaryRoleConfig, RoleEvent as PrimaryRoleEvent, RoleGeneration, RoleOwner,
    RoleRevocationCause as PrimaryRoleRevocationCause, RoleSupervisor as PrimaryRoleSupervisor,
};
