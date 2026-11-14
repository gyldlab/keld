//! Final endpoint service uses the same source owners, outside UI/Core guards.
use super::{
    APP_LINK_IO_DEADLINE, APP_LINK_READER_POLL, HostAppError, Instant, PrimaryRouterHandle,
    RendererDispatch, app_detail, thread,
};
use keld_wv::renderer_bridge::RendererBridgeServiceTurn;

impl RendererDispatch {
    pub(in crate::app_session) fn service(
        &mut self,
        router: &PrimaryRouterHandle,
        turn: RendererBridgeServiceTurn,
    ) -> Result<Option<Instant>, HostAppError> {
        self.poll_failure()?;
        if turn == RendererBridgeServiceTurn::Poll {
            return Ok(None);
        }
        self.refuse_retained_source_on_drain()?;
        let deadline = original_deadline(&mut self.drain_deadline, Instant::now());
        loop {
            // Native views/callbacks have drained before this call. Service the
            // existing retained source receipts directly: waiting for a Tao
            // HostWriterProgress event here would block that same UI loop.
            router.service_compat_writer_progress()?;
            router.service_dialog_writer_progress()?;
            router.service_numeric_echo_progress()?;
            self.poll_failure()?;
            self.refuse_retained_source_on_drain()?;
            if router.compat_endpoint_is_quiescent()? && router.numeric_echo_is_quiescent()? {
                break;
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(app_detail(
                    "renderer endpoint drain",
                    "actual Core protocol or numeric source remains retained at the original deadline",
                ));
            }
            thread::park_timeout(APP_LINK_READER_POLL.min(deadline.saturating_duration_since(now)));
        }
        self.finish(deadline)?;
        if Instant::now() >= deadline {
            return Err(app_detail(
                "renderer endpoint drain",
                "original stop deadline expired before actual endpoint completion",
            ));
        }
        Ok(Some(deadline))
    }
}

fn original_deadline(retained: &mut Option<Instant>, now: Instant) -> Instant {
    *retained.get_or_insert(now + APP_LINK_IO_DEADLINE)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_dispatch_owner_clock_does_not_renew_on_repeated_drain() {
        let now = Instant::now();
        let mut retained = None;
        let first = original_deadline(&mut retained, now);
        assert_eq!(first, now + APP_LINK_IO_DEADLINE);
        assert_eq!(
            original_deadline(&mut retained, first + APP_LINK_IO_DEADLINE),
            first
        );
        assert_eq!(retained, Some(first));
    }
}
