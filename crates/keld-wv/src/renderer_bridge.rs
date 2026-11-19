//! Shared observation of the authenticated renderer role and its final admission fence.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The canonical Core generation owner supplies the final native admission cut.
/// Implementations must acquire shutdown -> current role/link, validate the exact
/// authenticated caller, then invoke `admit` at most once while that fence is held.
/// The callback may only validate and commit local native admission state: no
/// allocation, COM, application callback, wait or I/O is permitted inside it.
///
/// Returning true fixes the operation to its already retained original target and
/// charge. Retirement ordered afterward may revoke future admission, but cannot
/// redirect this accepted operation to a successor or release it before completion.
pub trait RendererRoleFence: Send + Sync {
    /// Attempt an irreversible local admission under the existing Core owner.
    /// False means the callback was not accepted; it conveys no delivery receipt.
    fn try_admit_native(&self, admit: &mut dyn FnMut() -> bool) -> bool;
}

/// Native-only observation of the existing authenticated role's stop witness.
/// This retains the Core-owned identity; it mints no principal or wire token.
/// Windows native publication additionally requires its canonical Core fence.
#[derive(Clone)]
pub struct RendererRoleWitness {
    stopped: Arc<AtomicBool>,
    fence: Option<Arc<dyn RendererRoleFence>>,
}
impl std::fmt::Debug for RendererRoleWitness {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RendererRoleWitness")
            .field("stopped", &self.stopped)
            .field("has_canonical_fence", &self.fence.is_some())
            .finish_non_exhaustive()
    }
}
impl RendererRoleWitness {
    /// Capture the actual primary caller's existing stop flag, never page data.
    /// This preserves the existing macOS observation API. It does not qualify a
    /// Windows native publication: `try_admit_native` refuses without a fence.
    #[must_use]
    pub const fn new(stopped: Arc<AtomicBool>) -> Self {
        Self {
            stopped,
            fence: None,
        }
    }
    /// Retain the same actual caller together with its canonical Core owner.
    /// The fence must validate this exact stop allocation, identity and attempt;
    /// an independently copied Boolean is not an implementation of that contract.
    #[must_use]
    pub fn with_fence(stopped: Arc<AtomicBool>, fence: Arc<dyn RendererRoleFence>) -> Self {
        Self {
            stopped,
            fence: Some(fence),
        }
    }
    /// Commit only local native admission under Core's actual retirement fence.
    /// Call the native API on the retained original target only after this method
    /// returns true and every Core/native interior guard has been released.
    #[must_use]
    pub fn try_admit_native(&self, admit: &mut dyn FnMut() -> bool) -> bool {
        self.fence
            .as_ref()
            .is_some_and(|fence| fence.try_admit_native(admit))
    }
    /// Whether the captured role has not been retired by its owner.
    #[must_use]
    pub fn is_live(&self) -> bool {
        !self.stopped.load(Ordering::Acquire)
    }
    /// Require the exact existing role owner, not equal Boolean values.
    #[must_use]
    pub fn matches(&self, stopped: &Arc<AtomicBool>) -> bool {
        self.is_live() && Arc::ptr_eq(&self.stopped, stopped)
    }
    /// Compare two native observations without minting another generation.
    #[must_use]
    pub fn same_role(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stopped, &other.stopped)
    }
}

#[cfg(test)]
mod tests {
    use super::{RendererRoleFence, RendererRoleWitness};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn only_the_original_stop_allocation_matches_and_its_owner_retires_every_clone() {
        let stop = Arc::new(AtomicBool::new(false));
        let other_stop = Arc::new(AtomicBool::new(false));
        let witness = RendererRoleWitness::new(Arc::clone(&stop));
        let clone = witness.clone();
        let other = RendererRoleWitness::new(Arc::clone(&other_stop));
        assert!(witness.is_live());
        assert!(clone.matches(&stop));
        assert!(witness.same_role(&clone));
        assert!(!witness.same_role(&other));
        assert!(!witness.matches(&other_stop));

        stop.store(true, Ordering::Release);
        assert!(!witness.is_live());
        assert!(!clone.is_live());
        assert!(!clone.matches(&stop));
        assert!(witness.same_role(&clone));
        assert!(other.is_live());
    }

    struct TestFence {
        open: AtomicBool,
    }

    impl RendererRoleFence for TestFence {
        fn try_admit_native(&self, admit: &mut dyn FnMut() -> bool) -> bool {
            self.open.load(Ordering::Acquire) && admit()
        }
    }

    #[test]
    fn admission_uses_the_retained_fence_and_propagates_the_local_result() {
        let stop = Arc::new(AtomicBool::new(false));
        let unfenced = RendererRoleWitness::new(Arc::clone(&stop));
        let fence = Arc::new(TestFence {
            open: AtomicBool::new(false),
        });
        let fenced = RendererRoleWitness::with_fence(stop, fence.clone());
        assert!(unfenced.same_role(&fenced));
        assert!(!unfenced.try_admit_native(&mut || panic!("no fence can admit this operation")));
        assert!(!fenced.try_admit_native(&mut || panic!("closed fence invoked admission")));

        fence.open.store(true, Ordering::Release);
        let mut calls = 0;
        assert!(fenced.clone().try_admit_native(&mut || {
            calls += 1;
            true
        }));
        assert_eq!(calls, 1);
        assert!(!fenced.try_admit_native(&mut || false));
    }
}
