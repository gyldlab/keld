//! Canonical profile lock acquisition and reverse-release order.
//! Pure validation model; it does not itself acquire an operating-system lock.

use super::{ProfileError, ProfileErrorKind};

/// Global lock levels ordered from outer lifecycle authority to engine action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProfileLockLevel {
    /// Shared ordinary-start or exclusive update/rollback/uninstall lock.
    PackageLifecycle,
    /// Platform reverse-index registry lock.
    PlatformRegistry,
    /// Exact profile-identity control lease.
    ProfileLease,
    /// Platform engine/store operation and durable intent.
    EngineStoreIntent,
}

/// Pure acquisition/release-order model; it does not implement an OS lock.
#[derive(Debug, Default)]
pub struct ProfileLockOrder {
    held: Vec<ProfileLockLevel>,
}

impl ProfileLockOrder {
    /// Records an acquisition only when its level strictly follows held locks.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for inversion or same-level re-entry.
    pub fn acquire(&mut self, level: ProfileLockLevel) -> Result<(), ProfileError> {
        let allowed = matches!(
            (self.held.as_slice(), level),
            ([], ProfileLockLevel::PackageLifecycle)
                | (
                    [ProfileLockLevel::PackageLifecycle],
                    ProfileLockLevel::PlatformRegistry | ProfileLockLevel::ProfileLease
                )
                | (
                    [
                        ProfileLockLevel::PackageLifecycle,
                        ProfileLockLevel::PlatformRegistry
                    ],
                    ProfileLockLevel::ProfileLease
                )
                | (
                    [.., ProfileLockLevel::ProfileLease],
                    ProfileLockLevel::EngineStoreIntent
                )
        );
        if !allowed {
            return Err(ProfileError::new(ProfileErrorKind::LockOrderViolation));
        }
        self.held.push(level);
        Ok(())
    }

    /// Records a release only in exact reverse acquisition order.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for a non-LIFO or unheld release.
    pub fn release(&mut self, level: ProfileLockLevel) -> Result<(), ProfileError> {
        if self.held.last() != Some(&level) {
            return Err(ProfileError::new(ProfileErrorKind::LockOrderViolation));
        }
        self.held.pop();
        Ok(())
    }
}
