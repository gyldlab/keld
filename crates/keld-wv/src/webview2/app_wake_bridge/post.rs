//! Post only a unit hint while the actual delivery stays in its attached owner.
#![forbid(unsafe_code)]
use super::Failure;
use std::sync::Mutex;

pub(super) trait PostState {
    fn begin_post(&mut self) -> Result<bool, Failure>;
    fn post_failed(&mut self, fault: Failure);
}

/// Neither this helper nor its zero-argument native closure can own the payload.
/// Any returned failure/unwind is recorded before it is reported to the loop.
pub(super) fn post_staged<S: PostState>(
    state: &Mutex<S>,
    post: impl FnOnce() -> Result<(), ()>,
) -> Result<bool, Failure> {
    let should_post = {
        let mut owner = state.lock().map_err(|_| Failure::InputPoisoned)?;
        owner.begin_post()?
    };
    if !should_post {
        return Ok(false);
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(post))
        .map_err(|_| Failure::PostUnwound)
        .and_then(|result| result.map_err(|()| Failure::PostRejected));
    if let Err(fault) = result {
        state
            .lock()
            .map_err(|_| Failure::InputPoisoned)?
            .post_failed(fault);
        return Err(fault);
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
