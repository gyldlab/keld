//! Read-only AC24 hold observations on the existing facade control owner.

use super::FacadeRun;
use std::io::ErrorKind;
use std::thread;
use std::time::{Duration, Instant};

impl FacadeRun {
    /// Keep the approved two-second hold even after an early publication so the
    /// caller can release it, finish ordinary cleanup, then assert the violation.
    /// Transport failures are invalid observations, separate from publication.
    pub(crate) fn observe_release_during_hold(
        &self,
        mut reply_seen: impl FnMut(),
    ) -> Result<Option<String>, String> {
        assert!(
            self.control_capture.is_none(),
            "hold requires exclusive synchronous control reader"
        );
        self.reader
            .get_ref()
            .set_nonblocking(true)
            .expect("nonblocking held reply probe");
        let until = Instant::now() + Duration::from_secs(2);
        let mut publication = None;
        let mut transport_failure = None;
        let mut observed_reply = false;
        while Instant::now() < until {
            let mut reply_available = !self.reader.buffer().is_empty();
            if !reply_available {
                let mut byte = [0_u8; 1];
                match self.reader.get_ref().peek(&mut byte) {
                    Ok(0) => {
                        transport_failure.get_or_insert_with(|| {
                            "control EOF while DestroyWindow remained held".to_owned()
                        });
                    }
                    Ok(_) => reply_available = true,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                    Err(error) => {
                        transport_failure.get_or_insert_with(|| {
                            format!("held control observation failed: {error}")
                        });
                    }
                }
            }
            if reply_available && !observed_reply {
                observed_reply = true;
                // Capture native liveness at first actual reply availability,
                // before the release file exists and before consuming the reply.
                reply_seen();
                publication.get_or_insert_with(|| {
                    "facade reply available while DestroyWindow remained held".to_owned()
                });
            }
            let trace = self.trace();
            if let Some(line) = trace.lines().find(|line| {
                line.starts_with("RELEASED ") || line.starts_with("FAILED_BUILD_ANSWERED ")
            }) {
                publication
                    .get_or_insert_with(|| format!("release published inside WM_DESTROY: {line}"));
            }
            thread::park_timeout(Duration::from_millis(10));
        }
        self.reader
            .get_ref()
            .set_nonblocking(false)
            .expect("restore control reads");
        match transport_failure {
            Some(error) => Err(error),
            None => Ok(publication),
        }
    }
}
