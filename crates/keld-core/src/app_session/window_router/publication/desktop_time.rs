//! The first admitted desktop Subscribe owns each absolute startup cut.

use std::time::{Duration, Instant};

const CALLER_LIMIT: Duration = Duration::from_secs(30);
const UI_ENTRY_LIMIT: Duration = Duration::from_secs(10);
const SETTLEMENT_LIMIT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy)]
pub(super) struct DesktopSubscribeClock {
    admitted: Instant,
}

impl DesktopSubscribeClock {
    pub(super) const fn new(admitted: Instant) -> Self {
        Self { admitted }
    }

    pub(super) fn caller_deadline(self) -> Instant {
        self.admitted + CALLER_LIMIT
    }

    pub(super) fn ui_entry_expired(self, entered: Option<Instant>, now: Instant) -> bool {
        let limit = self.admitted + UI_ENTRY_LIMIT;
        entered.is_some_and(|entry| entry >= limit) || (entered.is_none() && now >= limit)
    }

    pub(super) fn init_deadline(self, entered: Instant) -> Instant {
        (entered + keld_wv::INITIAL_MILESTONE_DEADLINE).min(self.caller_deadline())
    }

    pub(super) fn settlement_deadline(self, first_sample_validated: Instant) -> Instant {
        (first_sample_validated + SETTLEMENT_LIMIT).min(self.caller_deadline())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_entry_has_its_own_cut_even_without_a_native_callback() {
        let start = Instant::now();
        let clock = DesktopSubscribeClock::new(start);
        let just_before = start + Duration::from_millis(9_999);
        let cut = start + Duration::from_secs(10);
        assert!(!clock.ui_entry_expired(None, just_before));
        assert!(clock.ui_entry_expired(None, cut));
        assert!(!clock.ui_entry_expired(Some(just_before), cut));
        assert!(clock.ui_entry_expired(Some(cut), cut));
    }

    #[test]
    fn init_starts_at_the_actual_entry_and_retains_its_existing_limit() {
        let start = Instant::now();
        let clock = DesktopSubscribeClock::new(start);
        assert_eq!(clock.caller_deadline(), start + Duration::from_secs(30));
        assert_eq!(
            clock.init_deadline(start + Duration::from_secs(7)),
            start + Duration::from_secs(22)
        );
    }

    #[test]
    fn the_whole_replay_uses_one_first_sample_cut_capped_by_caller_deadline() {
        let start = Instant::now();
        let clock = DesktopSubscribeClock::new(start);
        assert_eq!(
            clock.settlement_deadline(start + Duration::from_secs(21)),
            start + Duration::from_secs(26)
        );
        assert_eq!(
            clock.settlement_deadline(start + Duration::from_secs(28)),
            start + Duration::from_secs(30)
        );
    }
}
