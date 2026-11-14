//! The actual one worker state is staged before any fallible processing.
//! A returned error or unwind returns that SAME state through its `JoinHandle`.
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, TryLockError};

pub(super) enum RunFailure<E> {
    Busy,
    Poisoned,
    AlreadyTaken,
    Returned(E),
    Panicked,
}
pub(super) struct Returned<T, E> {
    pub(super) input: Option<T>,
    pub(super) result: Result<(), RunFailure<E>>,
}

pub(super) fn run<T, E>(
    source: &Mutex<Option<T>>,
    process: impl FnOnce(&mut T) -> Result<(), E>,
) -> Returned<T, E> {
    let mut input = None;
    let result = catch_unwind(AssertUnwindSafe(|| {
        {
            let mut source = match source.try_lock() {
                Ok(source) => source,
                Err(TryLockError::WouldBlock) => return Err(RunFailure::Busy),
                Err(TryLockError::Poisoned(_)) => return Err(RunFailure::Poisoned),
            };
            // Exact original ownership is attached before any processing/catch
            // boundary. There is no guard across a Core operation or wait.
            input = source.take();
        }
        let Some(input) = input.as_mut() else {
            return Err(RunFailure::AlreadyTaken);
        };
        process(input).map_err(RunFailure::Returned)
    }));
    Returned {
        input,
        result: result.unwrap_or(Err(RunFailure::Panicked)),
    }
}

/// Only the concrete std channel is called, never an opaque/native callback.
/// `SendError` returns the exact T. On success the existing receiver owns T;
/// this is transport handoff only, not a native or consumer receipt.
pub(super) fn send<T>(source: &mut Option<T>, destination: &Sender<T>) -> Result<(), ()> {
    let Some(value) = source.take() else {
        return Ok(());
    };
    match destination.send(value) {
        Ok(()) => Ok(()),
        Err(error) => {
            *source = Some(error.0);
            Err(())
        }
    }
}
