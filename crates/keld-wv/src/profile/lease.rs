//! In-process exclusive profile lease capability and exact-token generation accounting.
//! Native cross-process exclusion remains owned by each platform implementation.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::{ProfileError, ProfileErrorKind, ProfileIdentity, ProfilePlatform};

/// Exact common key whose native realization must be exclusively owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProfileLeaseKey {
    identity: ProfileIdentity,
    platform: ProfilePlatform,
    user_scope: [u8; 32],
    namespace_roots: [u8; 32],
}

impl ProfileLeaseKey {
    /// Creates a key from an identity and already validated user/root facts.
    ///
    /// The byte arrays are opaque stable fingerprints produced by the future
    /// platform owner after validating the current OS user and namespace roots.
    #[must_use]
    pub const fn from_host_validated_parts(
        identity: ProfileIdentity,
        platform: ProfilePlatform,
        user_scope: [u8; 32],
        namespace_roots: [u8; 32],
    ) -> Self {
        Self {
            identity,
            platform,
            user_scope,
            namespace_roots,
        }
    }
}

/// Opaque process-lifetime owner identity supplied by trusted host code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LeaseOwner([u8; 16]);

impl LeaseOwner {
    /// Validates an opaque host process identity for lease bookkeeping.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when the process identity is absent.
    pub fn from_host_process_identity(bytes: [u8; 16]) -> Result<Self, ProfileError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self(bytes))
    }
}

/// Unforgeable token returned by the common exclusive lease table.
#[derive(Debug)]
pub struct ProfileLease {
    key: ProfileLeaseKey,
    owner: LeaseOwner,
    generation: u64,
    capability: Arc<LeaseTableCapability>,
}

impl ProfileLease {
    /// Returns the exact validated lease key.
    #[must_use]
    pub const fn key(&self) -> ProfileLeaseKey {
        self.key
    }

    /// Returns the host process identity that acquired the token.
    #[must_use]
    pub const fn owner(&self) -> LeaseOwner {
        self.owner
    }
}

impl PartialEq for ProfileLease {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.owner == other.owner
            && self.generation == other.generation
            && Arc::ptr_eq(&self.capability, &other.capability)
    }
}

impl Eq for ProfileLease {}

/// In-process common exclusivity model used in addition to native platform locks.
///
/// This table cannot prove cross-process exclusion. T2, T3, and T4 must pair the
/// same key policy with a real OS lease and engine release evidence.
#[derive(Debug)]
pub struct ProfileLeaseTable {
    owners: BTreeMap<ProfileLeaseKey, (LeaseOwner, u64)>,
    next_generation: u64,
    capability: Arc<LeaseTableCapability>,
}

#[derive(Debug)]
struct LeaseTableCapability;

impl Default for ProfileLeaseTable {
    fn default() -> Self {
        Self {
            owners: BTreeMap::new(),
            next_generation: 0,
            capability: Arc::new(LeaseTableCapability),
        }
    }
}

impl ProfileLeaseTable {
    /// Acquires the exact key or rejects every same-key second owner.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when any owner already holds the key.
    pub fn acquire(
        &mut self,
        key: ProfileLeaseKey,
        owner: LeaseOwner,
    ) -> Result<ProfileLease, ProfileError> {
        if self.owners.contains_key(&key) {
            return Err(ProfileError::new(ProfileErrorKind::ProfileInUse));
        }
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or_else(|| ProfileError::new(ProfileErrorKind::LifecycleUnproven))?;
        let generation = self.next_generation;
        self.owners.insert(key, (owner, generation));
        Ok(ProfileLease {
            key,
            owner,
            generation,
            capability: Arc::clone(&self.capability),
        })
    }

    /// Releases only the exact token previously returned by this table.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when the token is absent or does not match.
    pub fn release(&mut self, lease: &ProfileLease) -> Result<(), ProfileError> {
        if !Arc::ptr_eq(&self.capability, &lease.capability)
            || self.owners.get(&lease.key) != Some(&(lease.owner, lease.generation))
        {
            return Err(ProfileError::new(ProfileErrorKind::ProfileInUse));
        }
        self.owners.remove(&lease.key);
        Ok(())
    }
}
