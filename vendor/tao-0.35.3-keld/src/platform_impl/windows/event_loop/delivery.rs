// Copyright 2026 Keld contributors
// SPDX-License-Identifier: Apache-2.0

//! The existing user-event queue must return ownership when its receiver closed.
//! This runs only after the unchanged native wake has succeeded.

use crate::event_loop::EventLoopClosed;
use crossbeam_channel::Sender;

pub(super) fn send<T>(sender: &Sender<T>, event: T) -> Result<(), EventLoopClosed<T>> {
  sender
    .send(event)
    .map_err(|failure| EventLoopClosed(failure.0))
}

#[cfg(test)]
mod tests;
