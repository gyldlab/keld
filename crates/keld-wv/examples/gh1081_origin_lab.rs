//! GH-1081 round-4 research only (never merged): an isolated webview harness
//! that loads one in-memory page through a candidate app-origin transport
//! (KEL-79, owner decision K2 §B).
//!
//! The example is test evidence, not a shipping binary. The `origin_r4` tests
//! in `crates/keld-host/tests` spawn it, and it connects back to the test's
//! control socket, reports engine facts, page-load events and every request
//! its handler served, and answers `url` / `served` / `reload` / `quit`. No
//! production loader is touched: the scheme handler and the scheme name (a
//! placeholder the test passes; the owner has not named the final scheme) live
//! only here.
//!
//! Arms: `inline` (today's URL-less HTML string, the control), `scheme` (a
//! per-app custom scheme served from this process's memory), `scheme-noauth`
//! (Windows negative control: no authority component, expected opaque origin)
//! and `https` (Windows only: a host-backed `https://<host>.invalid/` URL
//! answered by this process through `WebResourceRequested`).

use std::io::Write as _;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

/// Control-plane commands the test sends, one per line.
#[derive(Debug)]
enum Command {
    Url,
    Served,
    Reload,
    Quit,
    Unknown(String),
}

impl Command {
    fn parse(line: &str) -> Self {
        match line.trim() {
            "url" => Self::Url,
            "served" => Self::Served,
            "reload" => Self::Reload,
            "quit" => Self::Quit,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

/// The load path under test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    Inline,
    Scheme,
    SchemeNoAuthority,
    Https,
}

impl Arm {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "inline" => Ok(Self::Inline),
            "scheme" => Ok(Self::Scheme),
            "scheme-noauth" => Ok(Self::SchemeNoAuthority),
            "https" => Ok(Self::Https),
            other => Err(format!("unknown arm `{other}`")),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Scheme => "scheme",
            Self::SchemeNoAuthority => "scheme-noauth",
            Self::Https => "https",
        }
    }
}

struct Args {
    arm: Arm,
    page: Vec<u8>,
    title: String,
    control: String,
    scheme: String,
    host: String,
    /// `WebView2` user data folder; the WebKit arms use an in-memory store.
    #[cfg_attr(
        not(target_os = "windows"),
        allow(
            dead_code,
            reason = "parsed on every OS, consumed by the WebView2 arm only"
        )
    )]
    user_data: Option<std::path::PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut arm = None;
    let mut page = None;
    let mut title = None;
    let mut control = None;
    let mut scheme = None;
    let mut host = None;
    let mut user_data = None;
    let mut iter = std::env::args().skip(1);
    while let Some(flag) = iter.next() {
        let value = iter
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--arm" => arm = Some(Arm::parse(&value)?),
            "--page" => {
                page =
                    Some(std::fs::read(&value).map_err(|error| format!("read {value}: {error}"))?);
            }
            "--title" => title = Some(value),
            "--control" => control = Some(value),
            "--scheme" => scheme = Some(value),
            "--host" => host = Some(value),
            "--user-data" => user_data = Some(std::path::PathBuf::from(value)),
            other => return Err(format!("unknown flag `{other}`")),
        }
    }
    Ok(Args {
        arm: arm.ok_or("--arm required")?,
        page: page.ok_or("--page required")?,
        title: title.ok_or("--title required")?,
        control: control.ok_or("--control required")?,
        scheme: scheme.unwrap_or_else(|| String::from("keld-exp-unnamed")),
        host: host.unwrap_or_else(|| String::from("app-a")),
        user_data,
    })
}

/// The test's control socket: event lines out, command lines in.
#[derive(Clone)]
struct Control {
    stream: Arc<Mutex<TcpStream>>,
}

impl Control {
    fn connect(address: &str) -> Result<Self, String> {
        let stream = TcpStream::connect(address)
            .map_err(|error| format!("connect control {address}: {error}"))?;
        stream
            .set_nodelay(true)
            .map_err(|error| format!("control nodelay: {error}"))?;
        Ok(Self {
            stream: Arc::new(Mutex::new(stream)),
        })
    }

    /// One line to the test. A closed peer means the test is gone; the quit
    /// command (or the test killing this process) ends the run.
    fn send(&self, line: &str) {
        if let Ok(mut stream) = self.stream.lock() {
            let _ = stream.write_all(line.as_bytes());
            let _ = stream.write_all(b"\n");
            let _ = stream.flush();
        }
    }

    fn reader(&self) -> Result<TcpStream, String> {
        self.stream
            .lock()
            .map_err(|_| String::from("control lock poisoned"))?
            .try_clone()
            .map_err(|error| format!("control reader: {error}"))
    }
}

/// Host-owned bytes: the page at the root, a tiny text resource under `res/`,
/// and a 404 for anything else. Every request is recorded with its URI.
struct Table {
    page: Vec<u8>,
    served: Mutex<Vec<String>>,
}

impl Table {
    fn new(page: Vec<u8>) -> Self {
        Self {
            page,
            served: Mutex::new(Vec::new()),
        }
    }

    /// `(status, content type, body)` for one request URI.
    fn lookup(&self, uri: &str) -> (u16, &'static str, Vec<u8>) {
        if let Ok(mut served) = self.served.lock() {
            served.push(uri.to_owned());
        }
        let path = path_of(uri);
        if path.contains("res/") {
            return (
                200,
                "text/plain; charset=utf-8",
                format!("gh1081 resource {path}").into_bytes(),
            );
        }
        if path.is_empty() || path == "/" || path.ends_with("index.html") {
            return (200, "text/html; charset=utf-8", self.page.clone());
        }
        (404, "text/plain; charset=utf-8", Vec::new())
    }

    fn summary(&self) -> String {
        self.served.lock().map_or_else(
            |_| String::from("count=? uris=poisoned"),
            |served| format!("count={} uris={}", served.len(), served.join(",")),
        )
    }
}

/// The path part of `scheme://host/path?query` or `scheme:path` (no authority).
fn path_of(uri: &str) -> String {
    let without_query = uri.split_once('?').map_or(uri, |(path, _)| path);
    let path = if let Some((_, rest)) = without_query.split_once("://") {
        rest.find('/').map_or("/", |index| &rest[index..])
    } else {
        without_query
            .split_once(':')
            .map_or(without_query, |(_, rest)| rest)
    };
    path.to_owned()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod wry_lab {
    //! `WKWebView` and `WebKitGTK` through wry 0.56.1, the same crate and
    //! version keld-wv's interim backends use. `with_html` is today's URL-less
    //! load (`loadHTMLString(nil)` / `load_html(NULL)`); `with_custom_protocol`
    //! is `setURLSchemeHandler` / `register_uri_scheme` (+ `as_secure`).

    use super::{Args, Arm, Command, Control, Table};
    use std::borrow::Cow;
    use std::sync::Arc;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::window::WindowBuilder;
    use wry::http::header::CONTENT_TYPE;
    use wry::http::{Request, Response};
    use wry::{PageLoadEvent, WebViewBuilder};

    fn respond(
        table: &Table,
        control: &Control,
        request: &Request<Vec<u8>>,
    ) -> Response<Cow<'static, [u8]>> {
        let uri = request.uri().to_string();
        let (status, mime, body) = table.lookup(&uri);
        control.send(&format!("EV req status={status} uri={uri}"));
        Response::builder()
            .status(status)
            .header(CONTENT_TYPE, mime)
            .header("Cache-Control", "no-store")
            .body(Cow::Owned(body))
            .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[])))
    }

    fn engine_facts() -> &'static str {
        if cfg!(target_os = "macos") {
            "wry=0.56.1;backend=wkwebview;store=nonPersistent(incognito)"
        } else {
            "wry=0.56.1;webkit2gtk-rs=2.0.2;backend=webkitgtk;context=ephemeral(incognito)"
        }
    }

    /// Runs the harness until `quit`; never returns on success (tao owns exit).
    pub(super) fn run(args: &Args, control: Control) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        keld_wv::webkitgtk::prepare_gpu_safe_mode_process()
            .map_err(|error| format!("gpu safe mode: {error}"))?;
        let event_loop = EventLoopBuilder::<Command>::with_user_event().build();
        let window = WindowBuilder::new()
            .with_title(&args.title)
            .with_inner_size(tao::dpi::LogicalSize::new(800.0, 600.0))
            .build(&event_loop)
            .map_err(|error| format!("window: {error}"))?;
        let table = Arc::new(Table::new(args.page.clone()));
        let load_control = control.clone();
        let builder = WebViewBuilder::new()
            .with_incognito(true)
            .with_on_page_load_handler(move |event, url| {
                let kind = match event {
                    PageLoadEvent::Started => "started",
                    PageLoadEvent::Finished => "finished",
                };
                load_control.send(&format!("EV load {kind} url={url}"));
            });
        let initial = match args.arm {
            Arm::Inline => String::from("inline-html"),
            Arm::Scheme => format!("{}://{}/", args.scheme, args.host),
            Arm::SchemeNoAuthority | Arm::Https => {
                return Err(format!("arm {} is Windows-only", args.arm.name()));
            }
        };
        let builder = if args.arm == Arm::Inline {
            builder.with_html(String::from_utf8_lossy(&table.page).into_owned())
        } else {
            let handler_table = Arc::clone(&table);
            let handler_control = control.clone();
            builder
                .with_custom_protocol(args.scheme.clone(), move |_id, request| {
                    respond(&handler_table, &handler_control, &request)
                })
                .with_url(initial.clone())
        };
        #[cfg(target_os = "macos")]
        let webview = builder
            .build(&window)
            .map_err(|error| format!("webview: {error}"))?;
        #[cfg(target_os = "linux")]
        let webview = {
            use tao::platform::unix::WindowExtUnix as _;
            use wry::WebViewBuilderExtUnix as _;
            let vbox = window
                .default_vbox()
                .ok_or("tao window has no default GTK vbox")?;
            builder
                .build_gtk(vbox)
                .map_err(|error| format!("webview: {error}"))?
        };
        control.send(&format!(
            "HELLO pid={} arm={} initial={initial} engine={}",
            std::process::id(),
            args.arm.name(),
            engine_facts()
        ));
        super::forward_commands(&control, event_loop.create_proxy())?;
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            let quit = match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => true,
                Event::UserEvent(command) => answer(command, &webview, &table, &control),
                _ => false,
            };
            if quit {
                *control_flow = ControlFlow::Exit;
            }
        })
    }

    /// Answers one control command; `true` means quit.
    fn answer(command: Command, webview: &wry::WebView, table: &Table, control: &Control) -> bool {
        match command {
            Command::Url => control.send(&format!(
                "RE url {}",
                webview.url().unwrap_or_else(|error| format!("err:{error}"))
            )),
            Command::Served => control.send(&format!("RE served {}", table.summary())),
            Command::Reload => control.send(&format!(
                "RE reload {}",
                webview
                    .reload()
                    .map_or_else(|error| format!("err:{error}"), |()| String::from("ok"))
            )),
            Command::Quit => {
                control.send("RE quit");
                return true;
            }
            Command::Unknown(text) => control.send(&format!("RE unknown {text}")),
        }
        false
    }
}

/// Forwards the test's command lines to the UI thread through tao's proxy.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn forward_commands(
    control: &Control,
    proxy: tao::event_loop::EventLoopProxy<Command>,
) -> Result<(), String> {
    use std::io::BufRead as _;
    let reader = control.reader()?;
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(reader).lines() {
            let Ok(line) = line else { break };
            if proxy.send_event(Command::parse(&line)).is_err() {
                break;
            }
        }
    });
    Ok(())
}

#[cfg(target_os = "windows")]
// Research-only harness, never merged: the same WebView2 COM surface the keld-wv
// backend owns, exercised from test code so no production loader changes. Every
// block carries its SAFETY proof.
#[allow(unsafe_code)]
mod webview2_lab {
    //! `WebView2` through webview2-com 0.38.2, the crate keld-wv's backend uses.
    //! `NavigateToString` is today's URL-less load; the custom scheme is
    //! `ICoreWebView2CustomSchemeRegistration` at environment creation plus a
    //! `WebResourceRequested` filter; the `https` arm is the same filter on a
    //! real `https://<host>.invalid/` URL.
    #![deny(unsafe_op_in_unsafe_fn)]

    use super::{Args, Arm, Command, Control, Table};
    use std::sync::{Arc, mpsc};
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::platform::windows::WindowExtWindows as _;
    use tao::window::{Window, WindowBuilder};
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        CORE_WEBVIEW_TARGET_PRODUCT_VERSION, COREWEBVIEW2_WEB_ERROR_STATUS,
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL, COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
        CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2_11,
        ICoreWebView2_22, ICoreWebView2Controller, ICoreWebView2CustomSchemeRegistration,
        ICoreWebView2Environment, ICoreWebView2EnvironmentOptions,
    };
    use webview2_com::{
        ContextMenuRequestedEventHandler, CoreWebView2CustomSchemeRegistration,
        CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
        CreateCoreWebView2EnvironmentCompletedHandler, NavigationCompletedEventHandler,
        NavigationStartingEventHandler, WebResourceRequestedEventHandler, wait_with_pump,
    };
    use windows::Win32::Foundation::{E_POINTER, E_UNEXPECTED, HWND, RECT};
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
    use windows::Win32::UI::Shell::SHCreateMemStream;
    use windows::core::{BOOL, HSTRING, Interface as _, PCWSTR, PWSTR, w};

    /// Reads and frees one COM-allocated string.
    fn take_pwstr(text: PWSTR) -> String {
        if text.is_null() {
            return String::new();
        }
        // SAFETY: WebView2 returned a NUL-terminated UTF-16 string allocated
        // with the COM task allocator; it is read once here and freed once below.
        let value = unsafe { text.to_string() }.unwrap_or_default();
        // SAFETY: `text` came from the COM task allocator and has not been freed.
        unsafe { CoTaskMemFree(Some(text.as_ptr().cast())) };
        value
    }

    fn pointer_error() -> windows::core::Error {
        windows::core::Error::from(E_POINTER)
    }

    fn environment(args: &Args) -> Result<ICoreWebView2Environment, String> {
        let options = CoreWebView2EnvironmentOptions::default();
        if matches!(args.arm, Arm::Scheme | Arm::SchemeNoAuthority) {
            let registration = CoreWebView2CustomSchemeRegistration::new(args.scheme.clone());
            // SAFETY: the registration is exclusively owned here, before COM
            // sees it; these setters write its UnsafeCell fields on one thread.
            unsafe {
                registration.set_treat_as_secure(true);
                registration.set_has_authority_component(args.arm == Arm::Scheme);
                registration.set_allowed_origins(vec![format!("{}://{}", args.scheme, args.host)]);
            }
            // SAFETY: same exclusive ownership of `options` before COM sees it.
            unsafe {
                options.set_scheme_registrations(vec![Some(
                    ICoreWebView2CustomSchemeRegistration::from(registration),
                )]);
            }
        }
        let user_data = args
            .user_data
            .as_deref()
            .ok_or("--user-data is required on Windows")?;
        let user_data = HSTRING::from(user_data.as_os_str());
        let (tx, rx) = mpsc::channel();
        // SAFETY: the STA is initialised on this thread; the one-shot
        // completion handler is delivered through this thread's message pump
        // while `rx` waits below, so `tx` outlives the call.
        unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                &user_data,
                &ICoreWebView2EnvironmentOptions::from(options),
                &CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
                    move |result, environment| {
                        let environment = result.and(environment.ok_or_else(pointer_error));
                        tx.send(environment)
                            .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
                    },
                )),
            )
        }
        .map_err(|error| format!("environment launch: {error}"))?;
        wait_with_pump(rx)
            .map_err(|error| format!("environment pump: {error}"))?
            .map_err(|error| format!("environment: {error}"))
    }

    fn controller(
        environment: &ICoreWebView2Environment,
        hwnd: HWND,
    ) -> Result<ICoreWebView2Controller, String> {
        let (tx, rx) = mpsc::channel();
        // SAFETY: `environment` was created on this STA thread and `hwnd` is a
        // live tao window owned by the caller; the one-shot handler is
        // delivered through this thread's pump while `rx` waits.
        unsafe {
            environment.CreateCoreWebView2Controller(
                hwnd,
                &CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
                    move |result, controller| {
                        let controller = result.and(controller.ok_or_else(pointer_error));
                        tx.send(controller)
                            .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
                    },
                )),
            )
        }
        .map_err(|error| format!("controller launch: {error}"))?;
        wait_with_pump(rx)
            .map_err(|error| format!("controller pump: {error}"))?
            .map_err(|error| format!("controller: {error}"))
    }

    /// Answers every request matching `filter` from the table; returns the
    /// filter API that was available.
    fn install_resource_handler(
        webview: &ICoreWebView2,
        environment: ICoreWebView2Environment,
        filter: &str,
        table: Arc<Table>,
        control: Control,
    ) -> Result<&'static str, String> {
        let filter = HSTRING::from(filter);
        let method = match webview.cast::<ICoreWebView2_22>() {
            // SAFETY: live webview on its STA; the filter string outlives the call.
            Ok(v22) => unsafe {
                v22.AddWebResourceRequestedFilterWithRequestSourceKinds(
                    &filter,
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
                )
            }
            .map(|()| "ICoreWebView2_22::AddWebResourceRequestedFilterWithRequestSourceKinds"),
            // SAFETY: same as above on the base interface.
            Err(_) => unsafe {
                webview
                    .AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
            }
            .map(|()| "ICoreWebView2::AddWebResourceRequestedFilter"),
        }
        .map_err(|error| format!("resource filter: {error}"))?;
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_sender, args| {
            let args = args.ok_or_else(pointer_error)?;
            // SAFETY: the event args are live for this callback on the STA;
            // the URI out-parameter is a local freed by `take_pwstr`.
            let uri = unsafe {
                let request = args.Request()?;
                let mut uri = PWSTR::null();
                request.Uri(&raw mut uri)?;
                take_pwstr(uri)
            };
            let (status, mime, body) = table.lookup(&uri);
            control.send(&format!("EV req status={status} uri={uri}"));
            let headers = HSTRING::from(format!("Content-Type: {mime}\r\nCache-Control: no-store"));
            let reason = if status == 200 {
                w!("OK")
            } else {
                w!("Not Found")
            };
            // SAFETY: the memory stream copies `body`; the response is created
            // on the environment that owns this webview and handed to the live
            // event args before the callback returns.
            unsafe {
                let stream = SHCreateMemStream(Some(&body));
                let response = environment.CreateWebResourceResponse(
                    stream.as_ref(),
                    i32::from(status),
                    reason,
                    &headers,
                )?;
                args.SetResponse(&response)
            }
        }));
        let mut token = 0_i64;
        // SAFETY: live webview on its STA; `token` is a local out-parameter.
        unsafe { webview.add_WebResourceRequested(&handler, &raw mut token) }
            .map_err(|error| format!("add_WebResourceRequested: {error}"))?;
        Ok(method)
    }

    fn install_navigation_observers(
        webview: &ICoreWebView2,
        control: &Control,
    ) -> Result<(), String> {
        let mut token = 0_i64;
        let starting = control.clone();
        let handler = NavigationStartingEventHandler::create(Box::new(move |_sender, args| {
            let args = args.ok_or_else(pointer_error)?;
            // SAFETY: live event args on the STA; locals are the out-parameters.
            let (uri, redirected, id) = unsafe {
                let mut uri = PWSTR::null();
                args.Uri(&raw mut uri)?;
                let mut redirected = BOOL::default();
                args.IsRedirected(&raw mut redirected)?;
                let mut id = 0_u64;
                args.NavigationId(&raw mut id)?;
                (take_pwstr(uri), redirected.as_bool(), id)
            };
            starting.send(&format!(
                "EV navstart id={id} redirected={redirected} uri={uri}"
            ));
            Ok(())
        }));
        // SAFETY: live webview on its STA; `token` is a local out-parameter.
        unsafe { webview.add_NavigationStarting(&handler, &raw mut token) }
            .map_err(|error| format!("add_NavigationStarting: {error}"))?;
        let completed = control.clone();
        let source_view = webview.clone();
        let handler = NavigationCompletedEventHandler::create(Box::new(move |_sender, args| {
            let args = args.ok_or_else(pointer_error)?;
            // SAFETY: live event args and webview on the STA; locals are the
            // out-parameters and the source string is freed by `take_pwstr`.
            let (success, status, id, source) = unsafe {
                let mut success = BOOL::default();
                args.IsSuccess(&raw mut success)?;
                let mut status = COREWEBVIEW2_WEB_ERROR_STATUS(0);
                args.WebErrorStatus(&raw mut status)?;
                let mut id = 0_u64;
                args.NavigationId(&raw mut id)?;
                let mut source = PWSTR::null();
                source_view.Source(&raw mut source)?;
                (success.as_bool(), status.0, id, take_pwstr(source))
            };
            completed.send(&format!(
                "EV navdone id={id} success={success} status={status} source={source}"
            ));
            Ok(())
        }));
        // SAFETY: live webview on its STA; `token` is a local out-parameter.
        unsafe { webview.add_NavigationCompleted(&handler, &raw mut token) }
            .map_err(|error| format!("add_NavigationCompleted: {error}"))?;
        match webview.cast::<ICoreWebView2_11>() {
            Ok(v11) => {
                let menu = control.clone();
                let handler =
                    ContextMenuRequestedEventHandler::create(Box::new(move |_sender, args| {
                        let args = args.ok_or_else(pointer_error)?;
                        // SAFETY: live event args on the STA; each label is a
                        // COM string freed by `take_pwstr`.
                        let labels = unsafe {
                            let items = args.MenuItems()?;
                            let mut count = 0_u32;
                            items.Count(&raw mut count)?;
                            let mut labels = Vec::new();
                            for index in 0..count {
                                let item = items.GetValueAtIndex(index)?;
                                let mut label = PWSTR::null();
                                item.Label(&raw mut label)?;
                                labels.push(take_pwstr(label).replace(' ', "_"));
                            }
                            labels
                        };
                        menu.send(&format!("EV ctxmenu items={}", labels.join("|")));
                        Ok(())
                    }));
                // SAFETY: live webview on its STA; `token` is a local out-parameter.
                unsafe { v11.add_ContextMenuRequested(&handler, &raw mut token) }
                    .map_err(|error| format!("add_ContextMenuRequested: {error}"))
            }
            Err(error) => {
                control.send(&format!("EV ctxmenu unavailable=ICoreWebView2_11:{error}"));
                Ok(())
            }
        }
    }

    struct View {
        webview: ICoreWebView2,
        /// Owns the browser attachment; dropping it closes the webview.
        _controller: ICoreWebView2Controller,
        initial: String,
        engine: String,
    }

    fn create_view(
        args: &Args,
        window: &Window,
        control: &Control,
        table: &Arc<Table>,
    ) -> Result<View, String> {
        let environment = environment(args)?;
        let hwnd = HWND(window.hwnd() as *mut core::ffi::c_void);
        let controller = controller(&environment, hwnd)?;
        let size = window.inner_size();
        let rect = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(size.width).unwrap_or(i32::MAX),
            bottom: i32::try_from(size.height).unwrap_or(i32::MAX),
        };
        // SAFETY: live controller on its STA.
        unsafe { controller.SetBounds(rect) }.map_err(|error| format!("SetBounds: {error}"))?;
        // SAFETY: live controller on its STA.
        unsafe { controller.SetIsVisible(true) }
            .map_err(|error| format!("SetIsVisible: {error}"))?;
        // SAFETY: live controller on its STA.
        let webview = unsafe { controller.CoreWebView2() }
            .map_err(|error| format!("CoreWebView2: {error}"))?;
        install_navigation_observers(&webview, control)?;
        let host_filter = |prefix: &str| {
            install_resource_handler(
                &webview,
                environment.clone(),
                &format!("{prefix}*"),
                Arc::clone(table),
                control.clone(),
            )
        };
        let (initial, filter) = match args.arm {
            Arm::Inline => (String::from("inline-html"), "none"),
            Arm::Scheme => {
                let url = format!("{}://{}/", args.scheme, args.host);
                let filter = host_filter(&url)?;
                (url, filter)
            }
            Arm::SchemeNoAuthority => {
                let filter = host_filter(&format!("{}:", args.scheme))?;
                (format!("{}:index.html", args.scheme), filter)
            }
            Arm::Https => {
                let url = format!("https://{}.invalid/", args.host);
                let filter = host_filter(&url)?;
                (url, filter)
            }
        };
        // SAFETY: live environment on its STA; the version string is freed by `take_pwstr`.
        let runtime = unsafe {
            let mut version = PWSTR::null();
            environment
                .BrowserVersionString(&raw mut version)
                .map(|()| take_pwstr(version))
        }
        .map_err(|error| format!("BrowserVersionString: {error}"))?;
        // SAFETY: a static NUL-terminated UTF-16 constant from the SDK bindings.
        let sdk_target =
            unsafe { CORE_WEBVIEW_TARGET_PRODUCT_VERSION.to_string() }.unwrap_or_default();
        let navigated = if args.arm == Arm::Inline {
            let html = HSTRING::from(String::from_utf8_lossy(&table.page).as_ref());
            // SAFETY: live webview on its STA; `html` outlives the call.
            unsafe { webview.NavigateToString(&html) }
        } else {
            let url = HSTRING::from(initial.as_str());
            // SAFETY: live webview on its STA; `url` outlives the call.
            unsafe { webview.Navigate(&url) }
        };
        navigated.map_err(|error| format!("navigate: {error}"))?;
        Ok(View {
            webview,
            _controller: controller,
            initial,
            engine: format!("webview2-runtime={runtime};sdk-target={sdk_target};filter={filter}"),
        })
    }

    fn source(webview: &ICoreWebView2) -> String {
        // SAFETY: live webview on its STA; the source string is freed by `take_pwstr`.
        unsafe {
            let mut source = PWSTR::null();
            webview.Source(&raw mut source).map(|()| take_pwstr(source))
        }
        .unwrap_or_else(|error| format!("err:{error}"))
    }

    /// Runs the harness until `quit`; never returns on success (tao owns exit).
    pub(super) fn run(args: &Args, control: Control) -> Result<(), String> {
        // SAFETY: the first COM call on this thread, before any thread-affine
        // WebView2 object exists (S_FALSE if tao already initialised the STA).
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|error| format!("CoInitializeEx: {error}"))?;
        let event_loop = EventLoopBuilder::<Command>::with_user_event().build();
        let window = WindowBuilder::new()
            .with_title(&args.title)
            .with_inner_size(tao::dpi::LogicalSize::new(800.0, 600.0))
            .build(&event_loop)
            .map_err(|error| format!("window: {error}"))?;
        let table = Arc::new(Table::new(args.page.clone()));
        let view = create_view(args, &window, &control, &table)?;
        control.send(&format!(
            "HELLO pid={} arm={} initial={} engine={}",
            std::process::id(),
            args.arm.name(),
            view.initial,
            view.engine
        ));
        super::forward_commands(&control, event_loop.create_proxy())?;
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            let quit = match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => true,
                Event::UserEvent(command) => answer(command, &view, &table, &control),
                _ => false,
            };
            if quit {
                *control_flow = ControlFlow::Exit;
            }
        })
    }

    /// Answers one control command; `true` means quit.
    fn answer(command: Command, view: &View, table: &Table, control: &Control) -> bool {
        match command {
            Command::Url => control.send(&format!("RE url {}", source(&view.webview))),
            Command::Served => control.send(&format!("RE served {}", table.summary())),
            Command::Reload => {
                // SAFETY: live webview on its STA.
                let reloaded = unsafe { view.webview.Reload() };
                let outcome =
                    reloaded.map_or_else(|error| format!("err:{error}"), |()| String::from("ok"));
                control.send(&format!("RE reload {outcome}"));
            }
            Command::Quit => {
                control.send("RE quit");
                return true;
            }
            Command::Unknown(text) => control.send(&format!("RE unknown {text}")),
        }
        false
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let control = Control::connect(&args.control)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        wry_lab::run(&args, control)
    }
    #[cfg(target_os = "windows")]
    {
        webview2_lab::run(&args, control)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = (args, control);
        Err(String::from(
            "gh1081_origin_lab supports macOS, Windows and Linux",
        ))
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("KELD_GH1081_ORIGIN_LAB_FAIL {error}");
        std::process::exit(1);
    }
}
