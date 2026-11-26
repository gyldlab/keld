//! Main-thread native Quit event coalescing. This never grants termination.

use std::cell::Cell;

#[derive(Default)]
pub(super) struct QueuedNativeQuit {
    // Core publishes increasing generation identities; zero is unarmed.
    // The most recent queued identity is sufficient: older queued events
    // retain their own origin and cannot clear a newer pending intent.
    pending: Cell<u32>,
}

impl QueuedNativeQuit {
    pub(super) fn try_enqueue(&self, source_attempt: u32) -> bool {
        source_attempt != 0 && self.pending.replace(source_attempt) != source_attempt
    }

    pub(super) fn retire(&self, source_attempt: u32) {
        if self.pending.get() == source_attempt {
            self.pending.set(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::QueuedNativeQuit;

    #[test]
    fn same_source_coalesces_until_its_event_drains() {
        let queue = QueuedNativeQuit::default();
        assert!(!queue.try_enqueue(0));
        assert!(queue.try_enqueue(41));
        assert!(!queue.try_enqueue(41));
        queue.retire(41);
        assert!(queue.try_enqueue(41), "a later OS Quit may retry");
    }

    #[test]
    fn fresh_source_posts_before_old_event_drains() {
        let queue = QueuedNativeQuit::default();
        assert!(queue.try_enqueue(41));
        assert!(!queue.try_enqueue(0), "unarmed recovery must cancel");
        assert!(queue.try_enqueue(42), "fresh generation must post");
        assert!(!queue.try_enqueue(42), "fresh duplicate must coalesce");
        queue.retire(41);
        assert!(
            !queue.try_enqueue(42),
            "old drain must not clear fresh claim"
        );
        queue.retire(42);
        assert!(queue.try_enqueue(42), "fresh drain permits retry");
    }

    #[test]
    fn unmatched_retirement_does_not_clear_current_claim() {
        let queue = QueuedNativeQuit::default();
        assert!(queue.try_enqueue(u32::MAX));
        queue.retire(0);
        queue.retire(u32::MAX - 1);
        assert!(!queue.try_enqueue(u32::MAX));
        queue.retire(u32::MAX);
        assert!(queue.try_enqueue(u32::MAX));
    }
}
