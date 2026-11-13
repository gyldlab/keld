//! Pure Windows named-pipe deadline policy and timeout error mapping.
//!
//! The overlapped IO owner keeps every Win32 syscall, event, borrowed pointer
//! and cancellation decision. Only bounded clock/timeout arithmetic lives here.

use std::io;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::ERROR_SEM_TIMEOUT;
use windows_sys::Win32::System::Threading::INFINITE;

/// The wait timeout for `timeout`, in whole milliseconds rounded up, so a wait
/// bounded by a deadline never reports that deadline elapsed before it has
/// (a truncated fraction would end the wait early by up to one millisecond);
/// `None` is `INFINITE`.
pub(super) fn duration_to_wait_ms(timeout: Option<Duration>) -> u32 {
    let Some(timeout) = timeout else {
        return INFINITE;
    };
    let fraction = u128::from(timeout.subsec_nanos() % 1_000_000 != 0);
    let millis = (timeout.as_millis() + fraction)
        .max(1)
        .min(u128::from(INFINITE - 1));
    u32::try_from(millis).unwrap_or(INFINITE - 1)
}

pub(super) fn connect_deadline_error() -> io::Error {
    io::Error::from_raw_os_error(ERROR_SEM_TIMEOUT.cast_signed())
}

pub(super) fn validate_timeout(timeout: Option<Duration>) -> io::Result<()> {
    if timeout == Some(Duration::ZERO) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "zero named-pipe I/O timeout is invalid",
        ));
    }
    Ok(())
}

pub(super) fn effective_timeout(
    configured: Option<Duration>,
    absolute_deadline: Option<Instant>,
) -> io::Result<Option<Duration>> {
    let Some(deadline) = absolute_deadline else {
        return Ok(configured);
    };
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| io::Error::from_raw_os_error(ERROR_SEM_TIMEOUT.cast_signed()))?;
    Ok(Some(
        configured.map_or(remaining, |timeout| timeout.min(remaining)),
    ))
}
