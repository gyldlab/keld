//! An original input error is not authority to terminate today's caller.
//! Core alone supplies the exact-source `FatalQueued` disposition. A retained
//! ambiguous/retired source remains unqualified through final stop, with its
//! actual Input and cause owned by the same dispatcher.
use super::{HostAppError, Returned, RunFailure, app_detail, compat_router};

pub(super) enum DispatchFailure {
    RetiredSource(HostAppError),
    FatalQueued(HostAppError),
}
impl DispatchFailure {
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Consume the exact-source failure disposition once while retaining the original cause."
    )]
    pub(super) fn from_source(
        cause: HostAppError,
        disposition: compat_router::RendererFailureDisposition,
    ) -> Self {
        match disposition {
            compat_router::RendererFailureDisposition::RetiredSource => Self::RetiredSource(cause),
            compat_router::RendererFailureDisposition::FatalQueued => Self::FatalQueued(cause),
        }
    }
}
impl From<HostAppError> for DispatchFailure {
    fn from(cause: HostAppError) -> Self {
        // Numeric and channel errors retain actual custody but do not fabricate
        // a writer fact, reader EOF, or exact current-source Fatal authority.
        Self::RetiredSource(cause)
    }
}

pub(super) enum Completion {
    Succeeded,
    Failed(Option<HostAppError>),
    RetiredSource(Option<HostAppError>),
}
impl Completion {
    pub(super) fn poll_failure(&mut self) -> Result<(), HostAppError> {
        match self {
            Self::Failed(error) => Err(error.take().unwrap_or_else(|| {
                app_detail(
                    "renderer dispatch thread",
                    "retained worker failure remains unresolved",
                )
            })),
            // Poll is not drain or success. The same actual input/cause remains
            // attached, and release_ready requires Succeeded separately.
            Self::Succeeded | Self::RetiredSource(_) => Ok(()),
        }
    }

    pub(super) fn retained_refusal(&mut self) -> Option<HostAppError> {
        match self {
            Self::RetiredSource(error) => Some(error.take().unwrap_or_else(|| {
                app_detail(
                    "renderer dispatch drain",
                    "original retained source remains unresolved",
                )
            })),
            Self::Succeeded | Self::Failed(_) => None,
        }
    }
}

pub(super) fn returned_completion<T>(
    returned: Returned<T, DispatchFailure>,
) -> (Option<T>, Completion) {
    let Returned { input, result } = returned;
    let completion = match result {
        Ok(()) => Completion::Succeeded,
        Err(RunFailure::Returned(DispatchFailure::RetiredSource(cause))) => {
            Completion::RetiredSource(Some(cause))
        }
        Err(RunFailure::Returned(DispatchFailure::FatalQueued(cause))) => {
            Completion::Failed(Some(cause))
        }
        Err(RunFailure::Panicked) if input.is_some() => {
            // The catch retained the same actual input. An unwind supplies no
            // exact source decision, so it cannot fatal a replacement caller.
            Completion::RetiredSource(Some(app_detail(
                "renderer dispatch thread",
                "worker panicked with its actual input retained",
            )))
        }
        Err(other) => Completion::Failed(Some(app_detail(
            "renderer dispatch thread",
            match other {
                RunFailure::Busy => "retained input owner was busy",
                RunFailure::Poisoned => "retained input owner was poisoned",
                RunFailure::AlreadyTaken => "retained input was already taken",
                RunFailure::Panicked => "worker panicked before actual input attachment",
                RunFailure::Returned(_) => "worker failed",
            },
        ))),
    };
    (input, completion)
}
