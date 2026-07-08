//! macOS hello-window vertical slice (`WKWebView` via wry scaffolding).
//!
// SAFETY: wry/tao platform backends call Objective-C WebKit APIs on the main thread.
// This module is macOS-only; all mutations happen inside tao's event loop on the UI thread.
#![allow(unsafe_code)]

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;

use crate::error::WvError;

/// Opens a titled window and renders `html` until the user closes it.
///
/// # Errors
///
/// Returns [`WvError`] if window or webview creation fails.
pub fn run(title: &str, html: &str) -> Result<(), WvError> {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(tao::dpi::LogicalSize::new(960.0, 640.0))
        .build(&event_loop)
        .map_err(|e| WvError::Window(e.to_string()))?;

    let _webview = wry::WebViewBuilder::new()
        .with_html(html)
        .build(&window)
        .map_err(|e| WvError::Webview(e.to_string()))?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });

    // `EventLoop::run` only returns on internal error (never on normal close).
    #[allow(unreachable_code)]
    Ok(())
}
