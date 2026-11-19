//! Window sources submit to the single session-wide primary FIFO.

mod common;
mod emitter;
mod input;
mod native;
mod original_navigation;
mod replay;

pub(in crate::app_session) use common::{
    NativePublicationDisposition, PublicationError, PublicationResult,
};

use std::sync::Arc;
use std::time::Instant;

use super::super::primary_writer::WriterLink;
use super::publication_turn::WindowPublicationTurn;
use super::{HostAppError, PrimaryRouterHandle};

struct WindowEmitter<'source, 'session> {
    router: &'source PrimaryRouterHandle,
    turn: &'source WindowPublicationTurn<'session>,
    target: &'source Arc<WriterLink>,
    deadline: Instant,
}

fn with_cleanup(
    result: PublicationResult<()>,
    cleanup: Result<(), HostAppError>,
) -> PublicationResult<()> {
    match result {
        Ok(()) => cleanup.map_err(PublicationError::from),
        Err(error) => Err(error.with_cleanup(cleanup)),
    }
}
