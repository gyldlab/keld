//! The one construction candidate stays in `NativeBuild` through every fallible stage.
//! Stage promotion has no independent Close-on-Drop action.
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::{BTreeMap, btree_map::Entry};
use std::rc::Rc;

use super::super::View;
use super::super::controller_custody::AdoptedControllerClose;
use super::super::controller_request::ControllerRequest;
use crate::engine::{BuildStage, WindowSnapshot};
use crate::{WebviewId, WvError};
use tao::window::Window;

/// Before controller adoption, this same candidate owns the real parent and
/// its original request/receiver. An expired wait does not remove either.
pub(in crate::webview2) struct WindowCandidate {
    pub(super) window: Rc<Window>,
    pub(super) request: Option<ControllerRequest>,
}

/// The controller is borrowed only through the same one-shot Close authority.
pub(in crate::webview2) struct ControllerCandidate {
    pub(super) window: Rc<Window>,
    pub(super) close: Rc<AdoptedControllerClose>,
}

/// Generic parameters expose the exact move/retain helpers to pure Drop
/// witnesses. Production uses only these concrete three native stages.
pub(in crate::webview2) enum BuildResources<W = WindowCandidate, C = ControllerCandidate, V = View>
{
    Window(W),
    Controller(C),
    View(V),
}

impl BuildResources {
    pub(in crate::webview2) fn window(&self) -> &Window {
        match self {
            Self::Window(candidate) => &candidate.window,
            Self::Controller(candidate) => &candidate.window,
            Self::View(view) => &view.window,
        }
    }

    pub(super) fn retire_on_ui(&mut self) -> bool {
        match self {
            Self::Window(candidate) => candidate
                .request
                .as_mut()
                .is_none_or(|request| request.retire_on_ui()),
            Self::Controller(candidate) => candidate.close.close_on_ui(),
            Self::View(view) => view.retire_on_ui(),
        }
    }
}

/// Install the prepared stage in the SAME slot before returning the old
/// stage to its caller. Dropping an old request/COM handler may reenter; that
/// Drop must observe the new native owner already retained by the builder.
pub(super) fn replace_candidate<T>(candidate: &mut Option<T>, prepared: T) -> Option<T> {
    candidate.replace(prepared)
}

/// False or panic retains the original value in its original slot. Removing it
/// is permitted only after the exact resource's retirement returned true.
pub(super) fn retire_candidate<T>(
    candidate: &mut Option<T>,
    retire: impl FnOnce(&mut T) -> bool,
) -> bool {
    if let Some(value) = candidate.as_mut()
        && !retire(value)
    {
        return false;
    }
    *candidate = None;
    true
}

/// The destination entry and every semantic/native identity check must already
/// be resolved before the actual View moves. In the success arm only an
/// infallible `BTreeMap` insertion follows the take; no native/fallible callback
/// or conversion is invoked with the View in a temporary local.
pub(super) fn insert_view_candidate<W, C, V>(
    candidate: &mut Option<BuildResources<W, C, V>>,
    views: &mut BTreeMap<u32, V>,
    id: u32,
) -> bool {
    let Entry::Vacant(destination) = views.entry(id) else {
        return false;
    };
    match candidate.take() {
        Some(BuildResources::View(view)) => {
            destination.insert(view);
            true
        }
        other => {
            // Restore before even constructing an error. This branch transfers
            // no native resource and never drops a different candidate stage.
            *candidate = other;
            false
        }
    }
}

/// The caller has already installed `NativeBuild` in its durable owner. Inner
/// unwind stops here, before `create_app`'s typed Err can transfer that engine to
/// Core's retained failure owner. Native resources never enter this result.
pub(super) fn contain_build<T>(
    build: impl FnOnce() -> Result<T, BuildFailure>,
) -> Result<T, BuildFailure> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(build)).unwrap_or_else(|_| {
        Err(BuildFailure::unwound(super::candidate::candidate_error(
            "native construction unwound; its original candidate remains retained",
        )))
    })
}

/// Observed error disposition only; no resource or completion authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::webview2) enum BuildFailureDisposition {
    NotStarted,
    Returned,
    Unwound,
}

/// Errors carry no native resource out of the retained builder.
pub(in crate::webview2) struct BuildFailure {
    pub(in crate::webview2) disposition: BuildFailureDisposition,
    pub(in crate::webview2) stage: BuildStage,
    pub(in crate::webview2) source: WvError,
}

impl BuildFailure {
    pub(super) fn blocked(source: WvError) -> Self {
        Self {
            disposition: BuildFailureDisposition::NotStarted,
            stage: BuildStage::Window,
            source,
        }
    }

    pub(super) fn window(source: WvError) -> Self {
        Self {
            disposition: BuildFailureDisposition::Returned,
            stage: BuildStage::Window,
            source,
        }
    }

    pub(in crate::webview2) fn webview(source: WvError) -> Self {
        Self {
            disposition: BuildFailureDisposition::Returned,
            stage: BuildStage::Webview,
            source,
        }
    }

    pub(super) fn unwound(source: WvError) -> Self {
        Self {
            disposition: BuildFailureDisposition::Unwound,
            stage: BuildStage::Webview,
            source,
        }
    }

    pub(in crate::webview2) fn collision(source: WvError) -> Self {
        Self::window(source)
    }
}

/// This non-clone receipt is metadata for the one stored completed candidate.
/// Its private fields cannot mint a native owner or choose a replacement View.
pub(in crate::webview2) struct CompletedBuild {
    pub(super) id: WebviewId,
    pub(super) snapshot: Option<WindowSnapshot>,
}

impl CompletedBuild {
    pub(in crate::webview2) fn snapshot(&self) -> Option<WindowSnapshot> {
        self.snapshot
    }
}

#[cfg(test)]
#[path = "tests/candidate.rs"]
mod tests;
