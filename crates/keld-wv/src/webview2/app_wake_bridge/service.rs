//! The endpoint uses Core's one original drain clock without renewal.
#![forbid(unsafe_code)]
use crate::WvError;
use std::time::Instant;

pub(in crate::webview2) fn poll(observed: Option<Instant>) -> Result<(), WvError> {
    if observed.is_some() {
        return Err(error(
            "renderer Poll cannot establish or renew a drain deadline",
        ));
    }
    Ok(())
}

pub(in crate::webview2) fn drain(
    retained: &mut Option<Instant>,
    observed: Option<Instant>,
    now: Instant,
) -> Result<(), WvError> {
    let deadline =
        observed.ok_or_else(|| error("renderer Drain did not return its original deadline"))?;
    if retained.is_some_and(|original| original != deadline) {
        return Err(error(
            "renderer Drain attempted to replace its original deadline",
        ));
    }
    if now >= deadline {
        return Err(error(
            "renderer endpoint exceeded its original drain deadline",
        ));
    }
    *retained = Some(deadline);
    Ok(())
}

pub(in crate::webview2) fn check(retained: Option<Instant>, now: Instant) -> Result<(), WvError> {
    let deadline =
        retained.ok_or_else(|| error("renderer endpoint has no original drain deadline"))?;
    if now >= deadline {
        return Err(error(
            "renderer endpoint exceeded its original drain deadline",
        ));
    }
    Ok(())
}
fn error(detail: &str) -> WvError {
    WvError::EventLoop(String::from(detail))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn actual_drain_clock_is_retained_and_never_renewed() {
        let now = Instant::now();
        let first = now + Duration::from_secs(5);
        let mut retained = None;
        drain(&mut retained, Some(first), now).unwrap();
        drain(&mut retained, Some(first), now + Duration::from_secs(1)).unwrap();
        assert!(drain(&mut retained, Some(first + Duration::from_secs(1)), now).is_err());
        assert_eq!(retained, Some(first));
        assert!(check(retained, first).is_err());
        assert!(check(retained, first + Duration::from_secs(1)).is_err());
        assert!(poll(Some(first)).is_err());
        poll(None).unwrap();
    }
    #[test]
    fn missing_or_expired_drain_never_establishes_release_clock() {
        let now = Instant::now();
        let mut retained = None;
        assert!(drain(&mut retained, None, now).is_err());
        assert!(retained.is_none());
        assert!(drain(&mut retained, Some(now), now).is_err());
        assert!(retained.is_none());
    }
}
