//! Construction custodians preserve Close-before-HWND-drop on every failure.
#![deny(unsafe_op_in_unsafe_fn)]

use super::super::{ICoreWebView2Controller, ProfileReleaseWait, View, controller_custody};
use crate::engine::{BuildStage, WindowSnapshot};
use crate::{WebviewId, WvError};
use tao::window::Window;

/// Failed construction retains every resource until the caller can park its
/// native HWND in the existing release custodian, before posting destruction.
pub(in crate::webview2) enum BuildResources {
    Window(Window),
    Controller {
        window: Window,
        controller: ICoreWebView2Controller,
        release: Option<std::rc::Weak<ProfileReleaseWait>>,
    },
    View(View),
}

impl BuildResources {
    pub(in crate::webview2) fn window(&self) -> &Window {
        match self {
            Self::Window(window) | Self::Controller { window, .. } => window,
            Self::View(view) => &view.window,
        }
    }
}

impl Drop for BuildResources {
    fn drop(&mut self) {
        if let Self::Controller {
            controller,
            release,
            ..
        } = self
        {
            // This shared Close precedes dropping the standalone Tao Window.
            controller_custody::close_adopted(controller, release.as_ref());
        }
        // A View closes its own controller; Window has no controller.
    }
}

pub(in crate::webview2) struct BuildFailure {
    pub(in crate::webview2) stage: BuildStage,
    pub(in crate::webview2) source: WvError,
    pub(in crate::webview2) resources: Option<BuildResources>,
}

impl BuildFailure {
    pub(super) fn window(source: WvError) -> Self {
        Self {
            stage: BuildStage::Window,
            source,
            resources: None,
        }
    }

    pub(in crate::webview2) fn held(source: WvError, resources: BuildResources) -> Self {
        Self {
            stage: BuildStage::Webview,
            source,
            resources: Some(resources),
        }
    }
}

pub(in crate::webview2) struct BuiltView {
    pub(in crate::webview2) id: WebviewId,
    pub(in crate::webview2) view: View,
    pub(in crate::webview2) snapshot: Option<WindowSnapshot>,
}
