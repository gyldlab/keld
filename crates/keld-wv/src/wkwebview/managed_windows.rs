//! The shared native build and registry-window operations, always on the UI thread.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSApplication, NSScreen, NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSArray, NSNumber, NSRect, NSString};
use objc2_web_kit::{WKWebView, WKWebViewConfiguration};
use tao::dpi::{LogicalPosition, PhysicalPosition, PhysicalSize};
use tao::event_loop::EventLoopProxy;
use tao::event_loop::EventLoopWindowTarget;
use tao::window::WindowBuilder;
use wry::WebViewExtMacOS;

use super::macos_bridge::InstalledRendererBridge;
use super::macos_document::NativeDocumentOwner;
use super::macos_navigation_owner::KeldNavigationOwner;
use super::macos_resource_native::KeldAppSchemeHandler;
use super::macos_resource_source::AdmittedWebAppSource;
use super::{
    MacProfileOwner, MacRendererBridge, MacResourceView, RendererBridgeEndpoint, StartupPhase,
    StartupTrace, View, WkUserEvent, mark_startup, page_load_trace_handler,
};
use crate::WebviewId;
use crate::engine::{
    AppDesktopDisplay, AppDesktopFacts, AppDesktopRect, AppWindowCommand, AppWindowEvent,
    BuildStage, MacOriginalNavigationFact, NativeNavigationContext, NativeNavigationRefusal,
    OriginalEntryQuery, WebviewSpec, WindowSnapshot,
};
use crate::error::WvError;
use crate::media::guarded_default_media_builder;

/// Borrows the existing concrete UI owners for one native build. Both the
/// pre-loop renderer path and the in-loop registry path use this operation.
pub(super) struct NativeBuild<'a> {
    pub(super) target: &'a EventLoopWindowTarget<WkUserEvent>,
    pub(super) profile: &'a MacProfileOwner,
    pub(super) startup: &'a Arc<Mutex<StartupTrace>>,
    pub(super) media_seed_allow: bool,
    pub(super) renderer_endpoint: Option<&'a (
        std::sync::mpsc::SyncSender<super::RendererBridgeRequest>,
        u16,
    )>,
    /// One Core/Graph/Guard-qualified resource source for the first Legacy
    /// facade window only. No app-supplied URL or new filesystem principal.
    pub(super) resource_source: Option<&'a Arc<AdmittedWebAppSource>>,
    pub(super) resource_wake: EventLoopProxy<WkUserEvent>,
    /// The Host event coordinator receives real GH542 milestones without
    /// piggybacking on the original renderer `NavigationReady` callback.
    pub(super) resource_events: Option<Sender<AppWindowEvent>>,
}

type SharedNativeDocument = Rc<RefCell<NativeDocumentOwner>>;

struct OriginalBuild<'a> {
    source: &'a Arc<AdmittedWebAppSource>,
    image: Option<Arc<super::AppPreloadImage>>,
}

pub(super) struct BuildFailure {
    pub(super) stage: BuildStage,
    pub(super) source: WvError,
}

impl BuildFailure {
    fn window(source: WvError) -> Self {
        Self {
            stage: BuildStage::Window,
            source,
        }
    }

    fn webview(source: WvError) -> Self {
        Self {
            stage: BuildStage::Webview,
            source,
        }
    }
}

impl NativeBuild<'_> {
    pub(super) fn build(
        &self,
        id: WebviewId,
        spec: &WebviewSpec,
        events: Option<Sender<AppWindowEvent>>,
        endpoint: Option<RendererBridgeEndpoint>,
        facade_empty: bool,
    ) -> Result<(View, Option<InstalledRendererBridge>), BuildFailure> {
        self.build_with_preload(
            id,
            spec,
            events,
            endpoint,
            facade_empty,
            facade_empty
                .then_some(self.resource_source)
                .flatten()
                .map(|source| OriginalBuild {
                    source,
                    image: None,
                }),
        )
    }
    fn build_with_preload(
        &self,
        id: WebviewId,
        spec: &WebviewSpec,
        events: Option<Sender<AppWindowEvent>>,
        endpoint: Option<RendererBridgeEndpoint>,
        facade_empty: bool,
        original: Option<OriginalBuild<'_>>,
    ) -> Result<(View, Option<InstalledRendererBridge>), BuildFailure> {
        let original_source = original.as_ref().map(|value| value.source);
        let preload_image = original.and_then(|value| value.image);
        let window = self.create_window(spec)?;
        mark_startup(self.startup, StartupPhase::WindowCreated);
        let configuration = self
            .profile
            .configuration_for_view()
            .map_err(BuildFailure::webview)?;
        // Facade S1: Handler installs before WK construction. The selected
        // WebViewConfiguration continues to own its profile store/media guard.
        // A source-less facade stays empty and cannot load original assets.
        let source = original_source;
        if preload_image.is_some() && source.is_none() {
            return Err(BuildFailure::webview(WvError::RendererBridge {
                detail: String::from("preload image has no same-build admitted original source"),
            }));
        }
        let (resources, document) = self.prepare_resources(id, &configuration, source)?;
        let installed = endpoint
            .map(|endpoint| match (&preload_image, &document) {
                (Some(image), Some(document)) => MacRendererBridge::install_original(
                    id,
                    &configuration,
                    endpoint,
                    Arc::clone(image),
                    Rc::clone(document),
                    self.resource_wake.clone(),
                ),
                (None, _) => MacRendererBridge::install(id, &configuration, endpoint),
                _ => Err(WvError::RendererBridge {
                    detail: String::from("preload image has no native document owner"),
                }),
            })
            .transpose()
            .map_err(BuildFailure::webview)?;
        let navigation = installed
            .as_ref()
            .map(|installed| installed.bridge.navigation_handle());
        let builder = guarded_default_media_builder(
            id,
            page_load_trace_handler(Arc::clone(self.startup), events, navigation),
            self.media_seed_allow,
        )
        .with_webview_configuration(configuration);
        let webview = match if facade_empty {
            builder.build_empty_facade_window(&window)
        } else {
            builder.build_initial_window(&spec.initial, &window)
        } {
            Ok(webview) => webview,
            Err(source) => {
                if let Some(handler) = resources.as_ref() {
                    handler.request_shutdown();
                }
                if let Some(installed) = installed {
                    installed.bridge.destroy();
                }
                return Err(BuildFailure::webview(source));
            }
        };
        let native = webview.webview();
        let verified = self.profile.verify_webview(&native).and_then(|()| {
            installed
                .as_ref()
                .map_or(Ok(()), |installed| installed.bridge.verify_webview(&native))
        });
        if let Err(source) = verified {
            if let Some(handler) = resources.as_ref() {
                handler.request_shutdown();
            }
            if let Some(installed) = installed {
                installed.bridge.destroy();
            }
            drop(native);
            drop(webview);
            drop(window);
            return Err(BuildFailure::webview(source));
        }
        let resources = self.bind_resources(
            id,
            &native,
            resources,
            document,
            original_source,
            installed.as_ref(),
        )?;
        drop(native);
        mark_startup(self.startup, StartupPhase::WebviewAttached);
        Ok((
            View {
                webview,
                window,
                registry_managed: false,
                _preload_image: preload_image,
                resource: resources,
            },
            installed,
        ))
    }
    fn create_window(&self, spec: &WebviewSpec) -> Result<tao::window::Window, BuildFailure> {
        WindowBuilder::new()
            .with_title(&spec.title)
            .with_inner_size(tao::dpi::LogicalSize::new(
                spec.size.width,
                spec.size.height,
            ))
            .with_resizable(true)
            .with_minimizable(true)
            .with_closable(true)
            .build(self.target)
            .map_err(|source| BuildFailure::window(WvError::Window(source.to_string())))
    }

    fn prepare_resources(
        &self,
        id: WebviewId,
        configuration: &WKWebViewConfiguration,
        source: Option<&Arc<AdmittedWebAppSource>>,
    ) -> Result<
        (
            Option<Retained<KeldAppSchemeHandler>>,
            Option<SharedNativeDocument>,
        ),
        BuildFailure,
    > {
        let prepared = if let Some(source) = source {
            let document = Rc::new(RefCell::new(NativeDocumentOwner::new(id)));
            let handler = KeldAppSchemeHandler::install(
                id,
                configuration,
                Arc::clone(source),
                Rc::clone(&document),
                self.resource_wake.clone(),
            )
            .map_err(BuildFailure::webview)?;
            let document = Rc::clone(handler.current_document());
            document.borrow_mut().bind_source(handler.selected_source()).map_err(|error| {
                BuildFailure::webview(WvError::Webview(format!("actual native source/document binding failed: {error:?}; use the same selected source before first load")))
            })?;
            (Some(handler), Some(document))
        } else {
            (None, None)
        };
        Ok(prepared)
    }

    fn bind_resources(
        &self,
        id: WebviewId,
        native: &WKWebView,
        resources: Option<Retained<KeldAppSchemeHandler>>,
        document: Option<SharedNativeDocument>,
        original_source: Option<&Arc<AdmittedWebAppSource>>,
        installed: Option<&InstalledRendererBridge>,
    ) -> Result<Option<MacResourceView>, BuildFailure> {
        let bound = match (resources, document) {
            (Some(handler), Some(document)) => {
                if let Err(error) = handler.bind_real_view(native) {
                    handler.request_shutdown();
                    return Err(BuildFailure::webview(error));
                }
                let selected = original_source.ok_or_else(|| {
                    BuildFailure::webview(WvError::Webview(String::from(
                        "native source disappeared during resource binding",
                    )))
                })?;
                let observations = self.resource_events.as_ref().ok_or_else(|| {
                    BuildFailure::webview(WvError::Webview(String::from(
                        "native original document lacks its Host event observer",
                    )))
                })?;
                let navigation = KeldNavigationOwner::install(
                    id,
                    native,
                    Arc::clone(selected),
                    document,
                    observations.clone(),
                    installed.and_then(|installed| installed.bridge.commit_hook()),
                    installed.and_then(|installed| installed.bridge.attempt_hook()),
                )
                .map_err(|error| {
                    handler.request_shutdown();
                    BuildFailure::webview(error)
                })?;
                Some(MacResourceView {
                    handler,
                    navigation,
                })
            }
            (None, None) => None,
            _ => {
                return Err(BuildFailure::webview(WvError::Webview(String::from(
                    "native resource document/source ownership is incomplete",
                ))));
            }
        };
        Ok(bound)
    }
}

/// Route one already-authorized native menu Close activation into the same
/// Core close/veto path as the native close button. A retained click-time
/// `NSWindow` is matched only to a current registry-owned view; it is never
/// adopted as a handle, physically closed or retargeted to a successor.
pub(super) fn request_native_close(
    views: &BTreeMap<u32, View>,
    events: &Sender<AppWindowEvent>,
    window: &NSWindow,
    stopping: bool,
) -> Result<(), WvError> {
    if stopping || !window.isKeyWindow() {
        return Err(WvError::Window(String::from(
            "menu close requires the current focused window before shutdown",
        )));
    }
    let id = views
        .iter()
        .find_map(|(id, view)| {
            view.registry_managed
                .then(|| view.webview.webview().window())
                .flatten()
                .filter(|current| std::ptr::eq(current.as_ref(), window))
                .map(|_| *id)
        })
        .ok_or_else(|| {
            WvError::Window(String::from(
                "menu close target is not a current registry-owned window",
            ))
        })?;
    send(
        events,
        AppWindowEvent::WindowCloseRequested {
            view: WebviewId(id),
        },
    )
}

pub(super) fn command(
    command: AppWindowCommand,
    native: &NativeBuild<'_>,
    next_id: &mut u32,
    views: &mut BTreeMap<u32, View>,
    bridges: &mut BTreeMap<u32, MacRendererBridge>,
    events: &Sender<AppWindowEvent>,
) -> Result<(), WvError> {
    match command {
        AppWindowCommand::CreateWindow {
            build,
            width,
            height,
            x,
            y,
            original_source,
            preload_image,
        } => {
            let result = next_view_id(next_id)
                .map_err(BuildFailure::window)
                .and_then(|id| {
                    let spec = WebviewSpec {
                        size: crate::engine::LogicalSize {
                            width: f64::from(width),
                            height: f64::from(height),
                        },
                        ..WebviewSpec::default()
                    };
                    let endpoint=if preload_image.is_some(){
                        let (requests,channel)=native.renderer_endpoint.ok_or_else(||BuildFailure::webview(WvError::RendererBridge{detail:String::from("existing Core renderer endpoint is not installed")}))?;
                        Some(RendererBridgeEndpoint::shared_view(requests.clone(),*channel))
                    }else{None};
                    let (mut view, installed) = native.build_with_preload(id,&spec,None,endpoint,true,match (original_source.as_ref(),preload_image) {
    (Some(source),image)=>Some(OriginalBuild{source,image}),
    (None,None)=>None,
    _=>return Err(BuildFailure::webview(WvError::RendererBridge{detail:String::from("Create source/image pair did not share one verified selection")})),
})?;
                    view.registry_managed = true;
                    configure_registry_frame(&view, width, height, x, y)
                        .map_err(BuildFailure::window)?;
                    let state = snapshot(&view).map_err(BuildFailure::window)?;
                    if let Some(installed)=installed {bridges.insert(id.0,installed.bridge);}
                    views.insert(id.0, view);
                    Ok((id, state))
                });
            match result {
                Ok((view, snapshot)) => send(
                    events,
                    AppWindowEvent::WindowBuilt {
                        build,
                        view,
                        snapshot,
                    },
                ),
                Err(failure) => {
                    // The Copy event preserves stage/correlation; the full
                    // existing typed error remains in the host's UI-thread log.
                    eprintln!("registry window build {build} failed: {}", failure.source);
                    send(
                        events,
                        AppWindowEvent::WindowBuildFailed {
                            build,
                            stage: failure.stage,
                        },
                    )
                }
            }
        }
        AppWindowCommand::SetMaximized { view, maximized } => {
            managed_view(views, view)?.window.set_maximized(maximized);
            Ok(())
        }
        AppWindowCommand::SetFullScreen { view, full_screen } => {
            managed_view(views, view)?
                .window
                .set_fullscreen(full_screen.then_some(tao::window::Fullscreen::Borderless(None)));
            Ok(())
        }
        AppWindowCommand::DestroyWindow { view } => {
            let _ = release(views, bridges, events, view)?;
            Ok(())
        }
        AppWindowCommand::DestroyOriginalDocument {
            view,
            expected,
            admission,
        } => destroy_original_document(views, bridges, events, view, &expected, admission),
        AppWindowCommand::LoadAdmittedOriginal { context, query } => {
            load_admitted_original(views, events, context, &query)
        }
        // Session control retains its existing loop owner and service drive.
        AppWindowCommand::Quit
        | AppWindowCommand::Fatal
        | AppWindowCommand::MacAuthWake
        | AppWindowCommand::MacDialogWake => Ok(()),
    }
}

fn load_admitted_original(
    views: &BTreeMap<u32, View>,
    events: &Sender<AppWindowEvent>,
    context: NativeNavigationContext,
    query: &OriginalEntryQuery,
) -> Result<(), WvError> {
    let refuse = |error| {
        send(
            events,
            AppWindowEvent::MacOriginalNavigation {
                context,
                fact: MacOriginalNavigationFact::Refused { error },
            },
        )
    };
    let Ok(native) = managed_view(views, context.view()) else {
        return refuse(NativeNavigationRefusal::UnknownView);
    };
    let Some(owner) = native.resource.as_ref() else {
        return refuse(NativeNavigationRefusal::MissingSource);
    };
    let Ok(query) = query.as_str() else {
        return refuse(NativeNavigationRefusal::InvalidQuery);
    };
    // This command reports operation outcomes through the existing
    // captured-context event. Ok means handled, never Loaded.
    owner
        .navigation
        .load_verified_entry(&native.webview.webview(), context, query);
    Ok(())
}

fn configure_registry_frame(
    view: &View,
    width: u32,
    height: u32,
    x: Option<i32>,
    y: Option<i32>,
) -> Result<(), WvError> {
    let native =
        view.webview.webview().window().ok_or_else(|| {
            WvError::Window(String::from("the built webview has no native window"))
        })?;
    let primary_frame = if x.is_some() || y.is_some() {
        Some(primary_appkit_screen_frame()?)
    } else {
        None
    };
    let frame = registry_frame(native.frame(), width, height, x, y, primary_frame)?;
    // BrowserWindow's default useContentSize=false means OUTER size. The
    // existing renderer builder remains inner-sized. Tao's setters enqueue
    // even on the UI thread; this existing safe native primitive applies the
    // complete frame before the WindowBuilt snapshot is read.
    native.setFrame_display(frame, true);
    Ok(())
}

/// Read the actual primary screen's `AppKit` *point-space* frame on the UI
/// thread. Tao 0.35.3's physical monitor size scales `CGDisplay` pixel height a
/// second time, so undoing scale does NOT yield `NSScreen` frame points on a 2x
/// Retina display. Never use that self-consistent but offscreen coordinate.
fn primary_appkit_screen_frame() -> Result<NSRect, WvError> {
    let mtm = MainThreadMarker::new().ok_or_else(|| {
        WvError::Window(String::from(
            "native window placement requires the AppKit main thread",
        ))
    })?;
    let screens = NSScreen::screens(mtm);
    let primary = screens
        .firstObject()
        .ok_or_else(|| WvError::Window(String::from("AppKit primary NSScreen is unavailable")))?;
    Ok(primary.frame())
}

fn registry_frame(
    mut frame: NSRect,
    width: u32,
    height: u32,
    x: Option<i32>,
    y: Option<i32>,
    primary_frame: Option<NSRect>,
) -> Result<NSRect, WvError> {
    frame.origin.x = match x {
        Some(x) => {
            primary_frame
                .ok_or_else(|| {
                    WvError::Window(String::from(
                        "the AppKit primary x reference is unavailable",
                    ))
                })?
                .origin
                .x
                + f64::from(x)
        }
        None => frame.origin.x,
    };
    frame.origin.y = match y {
        Some(y) => {
            let primary = primary_frame.ok_or_else(|| {
                WvError::Window(String::from(
                    "the AppKit primary coordinate reference is unavailable",
                ))
            })?;
            primary.origin.y + primary.size.height - f64::from(y) - f64::from(height)
        }
        None => frame.origin.y + frame.size.height - f64::from(height),
    };
    frame.size.width = f64::from(width);
    frame.size.height = f64::from(height);
    if ![
        frame.origin.x,
        frame.origin.y,
        frame.size.width,
        frame.size.height,
    ]
    .into_iter()
    .all(f64::is_finite)
    {
        return Err(WvError::Window(String::from(
            "the native window frame is not finite",
        )));
    }
    Ok(frame)
}

fn appkit_registry_bounds(frame: NSRect, primary: NSRect) -> Result<(i32, i32, u32, u32), WvError> {
    // AppKit NSWindow.frame and NSScreen.frame are both point-space values.
    // The native frame (not Tao.outer_position) owns the registry's observed
    // result so a Retina-specific Tao conversion cannot conceal offscreen Y.
    let values = [
        frame.origin.x - primary.origin.x,
        primary.origin.y + primary.size.height - frame.origin.y - frame.size.height,
        frame.size.width,
        frame.size.height,
    ];
    if !values.iter().all(|v| v.is_finite())
        || values[2] <= 0.0
        || values[3] <= 0.0
        || values[0] < f64::from(i32::MIN)
        || values[0] > f64::from(i32::MAX)
        || values[1] < f64::from(i32::MIN)
        || values[1] > f64::from(i32::MAX)
        || values[2] > f64::from(u32::MAX)
        || values[3] > f64::from(u32::MAX)
    {
        return Err(WvError::Window(String::from(
            "the AppKit native window frame cannot be represented",
        )));
    }
    // The same audited `dpi::cast` rounding used in checked_logical_bounds,
    // but only after finite/sign/range checks have succeeded above.
    let position: LogicalPosition<i32> = LogicalPosition::new(values[0], values[1]).cast();
    let size: tao::dpi::LogicalSize<u32> = tao::dpi::LogicalSize::new(values[2], values[3]).cast();
    Ok((position.x, position.y, size.width, size.height))
}

/// Actual first-Init macOS desktop observation. Every `AppKit` object stays on
/// the Tao UI thread; the event carries only an owned checked value graph.
/// Full primary NSScreen.frame is the single origin for both bounds and work.
pub(super) fn capture_desktop() -> Result<AppDesktopFacts, WvError> {
    let mtm = MainThreadMarker::new().ok_or_else(|| {
        WvError::Window(String::from(
            "desktop capture requires the AppKit main thread",
        ))
    })?;
    let screens = NSScreen::screens(mtm);
    if screens.count() == 0 || screens.count() > 32 {
        return Err(WvError::Window(String::from(
            "native desktop must have between one and 32 complete displays",
        )));
    }
    let primary = screens
        .firstObject()
        .ok_or_else(|| WvError::Window(String::from("AppKit primary NSScreen is absent")))?;
    let primary_full = primary.frame();
    let display_key = NSString::from_str("NSScreenNumber");
    let mut ids = std::collections::BTreeSet::new();
    let mut displays = Vec::with_capacity(screens.count());
    for screen in &screens {
        let description = screen.deviceDescription();
        let opaque_number = description.objectForKey(&display_key).ok_or_else(|| {
            WvError::Window(String::from(
                "NSScreenNumber missing from native deviceDescription",
            ))
        })?;
        let number = opaque_number.downcast_ref::<NSNumber>().ok_or_else(|| {
            WvError::Window(String::from("NSScreenNumber native value is not NSNumber"))
        })?;
        // NSNumber::unsignedLongLongValue may truncate a fractional NSNumber.
        // Require an exact, positive, finite native u32 integer first. The
        // captured value is still a platform observation, never a role ID.
        let numeric = number.doubleValue();
        if !numeric.is_finite()
            || numeric.fract() != 0.0
            || !(1.0..=f64::from(u32::MAX)).contains(&numeric)
        {
            return Err(WvError::Window(String::from(
                "NSScreenNumber is not a positive integral u32",
            )));
        }
        let id = u32::try_from(number.unsignedLongLongValue())
            .map_err(|_| WvError::Window(String::from("NSScreenNumber exceeds u32")))?;
        // Every positive u32 is exactly representable as f64. This identity
        // check must not introduce an epsilon that admits a truncated ID.
        if f64::from(id).to_bits() != numeric.to_bits() {
            return Err(WvError::Window(String::from(
                "NSScreenNumber numeric conversion is not exact",
            )));
        }
        if id == 0 || !ids.insert(id) {
            return Err(WvError::Window(String::from(
                "native display identities must be unique and nonzero",
            )));
        }
        let scale_factor = screen.backingScaleFactor();
        if !scale_factor.is_finite() || scale_factor <= 0.0 {
            return Err(WvError::Window(String::from(
                "AppKit returned invalid native backing scale",
            )));
        }
        let (x, y, width, height) = appkit_registry_bounds(screen.frame(), primary_full)?;
        let (work_x, work_y, work_width, work_height) =
            appkit_registry_bounds(screen.visibleFrame(), primary_full)?;
        displays.push(AppDesktopDisplay {
            id,
            bounds: AppDesktopRect {
                x,
                y,
                width,
                height,
            },
            work_area: AppDesktopRect {
                x: work_x,
                y: work_y,
                width: work_width,
                height: work_height,
            },
            scale_factor,
        });
    }
    let primary_id = displays
        .first()
        .ok_or_else(|| WvError::Window(String::from("no native primary display captured")))?
        .id;
    Ok(AppDesktopFacts {
        primary_id,
        displays,
        should_use_dark_colors: capture_effective_appearance(mtm)?,
    })
}

/// Read the current effective application appearance on the same UI thread.
fn capture_effective_appearance(mtm: MainThreadMarker) -> Result<bool, WvError> {
    // The pinned objc2 constants NSAppearanceNameAqua/DarkAqua are extern
    // statics and would need unsafe. These exact NSString values are safe
    // and were paired against Electron 44.4.5 on the same macOS machine.
    let aqua = NSString::from_str("NSAppearanceNameAqua");
    let dark = NSString::from_str("NSAppearanceNameDarkAqua");
    let choices = NSArray::from_slice(&[&*aqua, &*dark]);
    let application = NSApplication::sharedApplication(mtm);
    let selected = application
        .effectiveAppearance()
        .bestMatchFromAppearancesWithNames(&choices)
        .ok_or_else(|| {
            WvError::Window(String::from(
                "native effective appearance has no Aqua/DarkAqua match",
            ))
        })?;
    if selected.isEqualToString(&dark) {
        Ok(true)
    } else if selected.isEqualToString(&aqua) {
        Ok(false)
    } else {
        Err(WvError::Window(String::from(
            "native effective appearance selected an unknown name",
        )))
    }
}

pub(super) fn next_view_id(next: &mut u32) -> Result<WebviewId, WvError> {
    let id = *next;
    let successor = id.checked_add(1).filter(|_| id != 0).ok_or_else(|| {
        WvError::EventLoop(String::from(
            "native webview id counter is exhausted; end this host session",
        ))
    })?;
    *next = successor;
    Ok(WebviewId(id))
}

fn checked_logical_bounds(
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    scale: f64,
) -> Result<(LogicalPosition<i32>, tao::dpi::LogicalSize<u32>), WvError> {
    // dpi 0.1.2 to_logical asserts a valid scale, and integer Pixel casts round
    // then saturate. Reject invalid/unrepresentable data BEFORE either operation.
    if !scale.is_normal() || scale <= 0.0 {
        return Err(WvError::Window(String::from(
            "native window scale factor is invalid",
        )));
    }
    let position = position.to_logical::<f64>(scale);
    let size = size.to_logical::<f64>(scale);
    if ![position.x, position.y, size.width, size.height]
        .into_iter()
        .all(f64::is_finite)
        || [position.x, position.y]
            .into_iter()
            .any(|value| value.round() < f64::from(i32::MIN) || value.round() > f64::from(i32::MAX))
        || [size.width, size.height]
            .into_iter()
            .any(|value| value.round() < 0.0 || value.round() > f64::from(u32::MAX))
    {
        return Err(WvError::Window(String::from(
            "native logical window bounds cannot be represented",
        )));
    }
    Ok((position.cast(), size.cast()))
}

fn logical_bounds(
    window: &tao::window::Window,
) -> Result<(LogicalPosition<i32>, tao::dpi::LogicalSize<u32>), WvError> {
    let position = window
        .outer_position()
        .map_err(|source| WvError::Window(source.to_string()))?;
    checked_logical_bounds(position, window.outer_size(), window.scale_factor())
}

pub(super) fn snapshot(view: &View) -> Result<WindowSnapshot, WvError> {
    let native =
        view.webview.webview().window().ok_or_else(|| {
            WvError::Window(String::from("the built webview has no native window"))
        })?;
    let (x, y, width, height) = if view.registry_managed {
        // Read actual AppKit point-space frame instead of Tao's CoreGraphics
        // pixel-height based outer_position() conversion. The latter can be
        // self-consistent with a wrong placement and mask a 2x Retina defect.
        appkit_registry_bounds(native.frame(), primary_appkit_screen_frame()?)?
    } else {
        let (position, size) = logical_bounds(&view.window)?;
        (position.x, position.y, size.width, size.height)
    };
    Ok(WindowSnapshot {
        x,
        y,
        width,
        height,
        focused: native.isKeyWindow(),
        maximized: native.isZoomed(),
        // Tao's fullscreen() returns requested shared state. AppKit's observed
        // style mask is the confirmation, never the setter's requested value.
        full_screen: native.styleMask().contains(NSWindowStyleMask::FullScreen),
    })
}

pub(super) fn send(events: &Sender<AppWindowEvent>, event: AppWindowEvent) -> Result<(), WvError> {
    events.send(event).map_err(|source| {
        WvError::EventLoop(format!("native window fact receiver ended: {source}"))
    })
}

pub(super) fn emit_state(
    events: &Sender<AppWindowEvent>,
    id: WebviewId,
    view: &View,
) -> Result<(), WvError> {
    let state = snapshot(view)?;
    // Core owns changed-only filtering and last-sent state. WV sends native
    // observations in one FIFO and keeps no competing window-state mirror.
    for event in [
        AppWindowEvent::WindowBounds {
            view: id,
            x: state.x,
            y: state.y,
            width: state.width,
            height: state.height,
        },
        AppWindowEvent::WindowFocus {
            view: id,
            focused: state.focused,
        },
        AppWindowEvent::WindowMaximized {
            view: id,
            maximized: state.maximized,
        },
        AppWindowEvent::WindowFullScreen {
            view: id,
            full_screen: state.full_screen,
        },
    ] {
        send(events, event)?;
    }
    Ok(())
}

pub(super) fn managed_view(views: &BTreeMap<u32, View>, id: WebviewId) -> Result<&View, WvError> {
    views
        .get(&id.0)
        .filter(|view| view.registry_managed)
        .ok_or(WvError::UnknownWebview { id: id.0 })
}

// The native check/claim/irreversible retirement is complete before this
// function observes Accepted. Refusals never call the unconditional owner.
fn destroy_original_document(
    views: &mut BTreeMap<u32, View>,
    bridges: &mut BTreeMap<u32, MacRendererBridge>,
    events: &Sender<AppWindowEvent>,
    id: WebviewId,
    expected: &super::MacNativeDocumentLease,
    admission: Arc<super::OriginalDocumentDestroyAdmission>,
) -> Result<(), WvError> {
    let accepted = managed_view(views, id)
        .ok()
        .and_then(|view| {
            view.resource.as_ref().map(|resource| {
                resource.navigation.try_admit_current_document_on_ui(
                    &view.webview.webview(),
                    resource.handler.selected_source(),
                    expected,
                    &admission,
                )
            })
        })
        .unwrap_or(false);
    if !accepted {
        admission.refuse_on_ui();
    }
    // Preserve event ordering on success. A closed observer does not abandon
    // the already accepted physical custody; the existing release still runs.
    let decision = send(
        events,
        AppWindowEvent::OriginalDocumentDestroyDecision {
            view: id,
            admission,
        },
    );
    let physical = if accepted {
        release(views, bridges, events, id).map(|_| ())
    } else {
        Ok(())
    };
    match (decision, physical) {
        (Err(first), Err(second)) => {
            eprintln!("{second}");
            Err(first)
        }
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

pub(super) fn release(
    views: &mut BTreeMap<u32, View>,
    bridges: &mut BTreeMap<u32, MacRendererBridge>,
    events: &Sender<AppWindowEvent>,
    id: WebviewId,
) -> Result<bool, WvError> {
    let view = managed_view(views, id)?;
    if let Some(bridge) = bridges.get(&id.0) {
        bridge.destroy();
    }
    if let Some(resource) = view.resource.as_ref() {
        // The native document and every task become invalid immediately.
        // Preserve the concrete View, WK handler and retained stage FD while
        // the worker is still in a regular-file kernel read.
        resource.navigation.destroy();
        resource.handler.request_shutdown();
    }
    let bridge_drained = bridges
        .get(&id.0)
        .map_or(Ok(true), MacRendererBridge::try_finish_retirement)?;
    let resource_drained = view
        .resource
        .as_ref()
        .map_or(Ok(true), |resource| resource.handler.try_finish_shutdown())?;
    if !bridge_drained || !resource_drained {
        return Ok(false);
    }
    if let Some(bridge) = bridges.remove(&id.0) {
        bridge.destroy();
    }
    let released = views
        .remove(&id.0)
        .ok_or(WvError::UnknownWebview { id: id.0 })?;
    // View drops WebKit before the native window. Tao 0.35.3's close_async
    // calls run_on_main, which executes close inline on this UI thread
    // (platform_impl/macos/util/async.rs). No queued request is a receipt here.
    drop(released);
    send(events, AppWindowEvent::WindowReleased { view: id })?;
    Ok(true)
}

/// Finish only windows whose Destroy has already retired the native document.
/// No second command/queue is introduced and repeated calls remain idempotent.
pub(super) fn finish_pending_releases(
    views: &mut BTreeMap<u32, View>,
    bridges: &mut BTreeMap<u32, MacRendererBridge>,
    events: &Sender<AppWindowEvent>,
) -> Result<bool, WvError> {
    let pending: Vec<_> = views
        .iter()
        .filter_map(|(id, view)| {
            view.resource
                .as_ref()
                .filter(|resource| resource.handler.shutdown_requested())
                .map(|_| WebviewId(*id))
                .or_else(|| {
                    bridges
                        .get(id)
                        .filter(|bridge| bridge.retirement_requested())
                        .map(|_| WebviewId(*id))
                })
        })
        .collect();
    let mut drained = true;
    for id in pending {
        drained &= release(views, bridges, events, id)?;
    }
    Ok(drained)
}

/// The earliest original reader deadline is a failure guard, never progress.
pub(super) fn resource_shutdown_deadline(
    views: &BTreeMap<u32, View>,
) -> Option<std::time::Instant> {
    views
        .values()
        .filter_map(|view| {
            view.resource
                .as_ref()
                .and_then(|resource| resource.handler.shutdown_deadline())
        })
        .min()
}

pub(super) fn release_all(
    views: &mut BTreeMap<u32, View>,
    bridges: &mut BTreeMap<u32, MacRendererBridge>,
    events: &Sender<AppWindowEvent>,
) -> Result<bool, WvError> {
    let ids: Vec<_> = views
        .iter()
        .filter_map(|(id, view)| view.registry_managed.then_some(WebviewId(*id)))
        .collect();
    let mut drained = true;
    for id in ids {
        drained &= release(views, bridges, events, id)?;
    }
    Ok(drained)
}

#[cfg(test)]
mod tests {
    use super::{appkit_registry_bounds, checked_logical_bounds, next_view_id, registry_frame};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use tao::dpi::{PhysicalPosition, PhysicalSize};

    #[test]
    fn native_bounds_use_logical_outer_geometry_and_checked_dpi_rounding() {
        let (position, size) = checked_logical_bounds(
            PhysicalPosition::new(-75, 183),
            PhysicalSize::new(1600, 1221),
            2.0,
        )
        .expect("representable native bounds");
        assert_eq!(
            (position.x, position.y, size.width, size.height),
            (-38, 92, 800, 611)
        );
        for scale in [
            0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::from_bits(1),
            f64::MIN_POSITIVE,
        ] {
            assert!(
                checked_logical_bounds(
                    PhysicalPosition::new(i32::MIN, 1),
                    PhysicalSize::new(2, u32::MAX),
                    scale
                )
                .is_err()
            );
        }
        let (position, size) = checked_logical_bounds(
            PhysicalPosition::new(i32::MIN, i32::MAX),
            PhysicalSize::new(0, u32::MAX),
            1.0,
        )
        .expect("exact integer bounds");
        assert_eq!(
            (position.x, position.y, size.width, size.height),
            (i32::MIN, i32::MAX, 0, u32::MAX)
        );
    }

    #[test]
    fn registry_dimensions_size_the_outer_frame_and_preserve_unspecified_top() {
        let initial = NSRect::new(NSPoint::new(100.0, 100.0), NSSize::new(700.0, 500.0));
        let frame = registry_frame(initial, 800, 600, None, None, None)
            .expect("OS placement with outer-size request");
        assert_eq!(frame.origin.x.to_bits(), 100.0_f64.to_bits());
        assert_eq!(frame.origin.y.to_bits(), 0.0_f64.to_bits());
        assert_eq!(frame.size.width.to_bits(), 800.0_f64.to_bits());
        assert_eq!(frame.size.height.to_bits(), 600.0_f64.to_bits());
        let primary = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1920.0, 1200.0));
        let frame = registry_frame(initial, 800, 600, Some(37), Some(91), Some(primary))
            .expect("primary top-left coordinates");
        assert_eq!(frame.origin.x.to_bits(), 37.0_f64.to_bits());
        assert_eq!(frame.origin.y.to_bits(), 509.0_f64.to_bits());
        assert!(registry_frame(initial, 800, 600, None, Some(91), None).is_err());
        let invalid_primary = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1000.0, f64::NAN));
        assert!(registry_frame(initial, 800, 600, None, Some(91), Some(invalid_primary)).is_err());
    }

    #[test]
    fn retina_and_offset_primary_place_native_frame_in_appkit_points_not_display_pixels()
    -> Result<(), super::WvError> {
        // Independent arithmetic discriminator: the physical CG display may
        // report 1800 pixels while NSScreen is only 900 AppKit points high.
        // The wrong height yields bottom 1109, entirely offscreen; point-space
        // coordinates yield bottom 209 and a visible frame intersection.
        let retina = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0));
        let initial = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(700.0, 500.0));
        let frame = registry_frame(initial, 800, 600, Some(37), Some(91), Some(retina))?;
        assert_eq!((frame.origin.x, frame.origin.y), (37.0, 209.0));
        assert!(frame.origin.y >= retina.origin.y);
        assert!(frame.origin.y + frame.size.height <= retina.origin.y + retina.size.height);
        assert_eq!(appkit_registry_bounds(frame, retina)?, (37, 91, 800, 600));
        let pixel_height = 1800.0;
        assert_eq!(pixel_height - 91.0 - 600.0, 1109.0);
        assert!(1109.0 > retina.origin.y + retina.size.height);

        // Primary screens need not have zero global AppKit origin when the
        // desktop arrangement includes monitors to the left or below them.
        let shifted = NSRect::new(NSPoint::new(-1920.0, 120.0), NSSize::new(1440.0, 900.0));
        let frame = registry_frame(initial, 800, 600, Some(37), Some(91), Some(shifted))?;
        assert_eq!((frame.origin.x, frame.origin.y), (-1883.0, 329.0));
        assert_eq!(appkit_registry_bounds(frame, shifted)?, (37, 91, 800, 600));
        Ok(())
    }

    #[test]
    fn work_areas_share_the_full_primary_point_space_anchor() -> Result<(), super::WvError> {
        // Hypothetical arithmetic fixtures only: no OS capture or DPI equivalence claim.
        let primary = NSRect::new(NSPoint::new(100.0, 200.0), NSSize::new(1440.0, 900.0));
        let primary_work = NSRect::new(NSPoint::new(100.0, 200.0), NSSize::new(1440.0, 876.0));
        let left_work = NSRect::new(NSPoint::new(-1160.0, 300.0), NSSize::new(1260.0, 800.0));
        let above_work = NSRect::new(NSPoint::new(100.0, 1100.0), NSSize::new(1024.0, 768.0));
        assert_eq!(appkit_registry_bounds(primary, primary)?, (0, 0, 1440, 900));
        assert_eq!(
            appkit_registry_bounds(primary_work, primary)?,
            (0, 24, 1440, 876)
        );
        assert_eq!(
            appkit_registry_bounds(left_work, primary)?,
            (-1260, 0, 1260, 800)
        );
        assert_eq!(
            appkit_registry_bounds(above_work, primary)?,
            (0, -768, 1024, 768)
        );
        for invalid in [
            NSRect::new(NSPoint::new(100.0, 200.0), NSSize::new(1440.0, 0.0)),
            NSRect::new(NSPoint::new(f64::NAN, 200.0), NSSize::new(1440.0, 876.0)),
        ] {
            assert!(appkit_registry_bounds(invalid, primary).is_err());
        }
        Ok(())
    }

    #[test]
    fn native_view_ids_are_checked_and_never_wrap_or_mint_zero() {
        let mut next = 7;
        assert_eq!(next_view_id(&mut next).expect("first view").0, 7);
        assert_eq!(next_view_id(&mut next).expect("next view").0, 8);
        for mut exhausted in [0, u32::MAX] {
            let original = exhausted;
            assert!(next_view_id(&mut exhausted).is_err());
            assert_eq!(exhausted, original);
        }
    }
}
