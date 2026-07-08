//! keld-runtime — the app-process supervisor.
//!
//! Spawns the developer's JS/TS main under a pinned Bun, supervises it
//! (exponential backoff, crash-loop breaker), and hands it the kipc link and
//! shared-memory handles at spawn. The renderer outlives app-process restarts
//! because the host owns all windows. Normative spec:
//! `docs/architecture/06-runtime-and-tooling.md` §1.

/// Restart policy defaults; tuned via `keld.config.ts` `runtime.supervision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestartPolicy {
    /// Maximum crashes tolerated inside `window_secs` before giving up.
    pub max_crashes: u8,
    /// Sliding window for the crash-loop breaker, in seconds.
    pub window_secs: u16,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_crashes: 3,
            window_secs: 30,
        }
    }
}
