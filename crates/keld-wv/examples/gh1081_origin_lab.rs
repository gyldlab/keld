//! GH-1081 rounds 4 and 5 research only (never merged): an isolated webview
//! harness that loads one in-memory page through a candidate app-origin
//! transport (KEL-79, owner decision K2 §B).
//!
//! The example is test evidence, not a shipping binary. The `origin_r4` and
//! `origin_r5` tests in `crates/keld-host/tests` spawn it, and it connects back
//! to the test's control socket, reports engine facts, page-load events and
//! every request its handler served, and answers `url` / `served` / `reload` /
//! `second` / `procs` / `quit`. No production loader is touched: the scheme
//! handler and the scheme name (a placeholder the test passes; the owner has
//! not named the final scheme) live only here.
//!
//! Arms: `inline` (today's URL-less HTML string, the control), `scheme` (a
//! per-app custom scheme served from this process's memory), `scheme-noauth`
//! (Windows negative control: no authority component, expected opaque origin)
//! and `https` (Windows only: a host-backed `https://<host>.invalid/` URL
//! answered by this process through `WebResourceRequested`).
//!
//! Round 5 adds, all test-selected: a second view (`second`) with its own host
//! in a shared or separate store (X4); store modes (X5: ephemeral, persistent,
//! Linux explicit directory, Windows `InPrivate`); handler modes (X6: the
//! strict in-memory table, a deliberately naive disk path-join, the host check
//! switched off) and a deliberately loose Windows request filter; worker,
//! service-worker, POST and image resources (X7); 3xx answers (X8); and the
//! engine's content-process failure events plus a `WebView2` process census (X9).

use std::io::Write as _;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Control-plane commands the test sends, one per line.
#[derive(Debug)]
enum Command {
    Url,
    Served,
    Reload,
    Second,
    Procs,
    Quit,
    Unknown(String),
}

impl Command {
    fn parse(line: &str) -> Self {
        match line.trim() {
            "url" => Self::Url,
            "served" => Self::Served,
            "reload" => Self::Reload,
            "second" => Self::Second,
            "procs" => Self::Procs,
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

/// Where the engine keeps this launch's web data.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Store {
    /// In memory: `nonPersistent` (`WKWebView`), an ephemeral context
    /// (`WebKitGTK`), the test-owned fresh user data folder (`WebView2`).
    Ephemeral,
    /// On disk: the identified store `--store-id` (`WKWebView`, macOS 14+),
    /// the default context under the XDG directories (`WebKitGTK`), the given
    /// user data folder (`WebView2`).
    Persistent,
    /// Linux only: an explicit base data directory (wry `WebContext::new`).
    Directory(PathBuf),
    /// Windows only: an `InPrivate` controller profile in the given folder.
    InPrivate,
}

impl Store {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "ephemeral" => Ok(Self::Ephemeral),
            "persistent" => Ok(Self::Persistent),
            "inprivate" => Ok(Self::InPrivate),
            other => other.strip_prefix("dir:").map_or_else(
                || Err(format!("unknown store `{other}`")),
                |directory| Ok(Self::Directory(PathBuf::from(directory))),
            ),
        }
    }

    fn name(&self) -> String {
        match self {
            Self::Ephemeral => String::from("ephemeral"),
            Self::Persistent => String::from("persistent"),
            Self::Directory(directory) => format!("dir:{}", directory.display()),
            Self::InPrivate => String::from("inprivate"),
        }
    }
}

/// Whether the second view shares the first view's store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SecondStore {
    Shared,
    /// The second view gets an ephemeral store of its own.
    Separate,
}

/// How requests are answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Handler {
    /// Exact-match in-memory table plus a host check (the candidate shape).
    Strict,
    /// Mutation control: percent-decode the path and join it under `--root`.
    Naive,
    /// Mutation control: the strict table with the host check switched off.
    NoHost,
}

impl Handler {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "strict" => Ok(Self::Strict),
            "naive" => Ok(Self::Naive),
            "nohost" => Ok(Self::NoHost),
            other => Err(format!("unknown handler `{other}`")),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Naive => "naive",
            Self::NoHost => "nohost",
        }
    }
}

/// The `WebView2` request filter shape (ignored elsewhere).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Filter {
    /// `<origin>/*`, anchored at the scheme.
    Anchored,
    /// Mutation control: `*<host>*`, which matches the host anywhere in the URL.
    Loose,
}

struct Args {
    arm: Arm,
    page: Vec<u8>,
    second_page: Vec<u8>,
    title: String,
    control: String,
    scheme: String,
    host: String,
    second_host: String,
    store: Store,
    /// Identifier of the macOS identified store (`--store-id`, 32 hex digits).
    #[cfg_attr(
        not(target_os = "macos"),
        allow(dead_code, reason = "parsed on every OS, consumed by WKWebView only")
    )]
    store_id: [u8; 16],
    second_store: SecondStore,
    handler: Handler,
    root: Option<PathBuf>,
    /// `WebView2` request filter shape.
    #[cfg_attr(
        not(target_os = "windows"),
        allow(
            dead_code,
            reason = "parsed on every OS, consumed by the WebView2 arms only"
        )
    )]
    filter: Filter,
    /// Target of the `redir/ext` 3xx answer.
    ext_url: String,
    /// `WebView2` user data folders; the `WebKit` arms use their store modes.
    #[cfg_attr(
        not(target_os = "windows"),
        allow(
            dead_code,
            reason = "parsed on every OS, consumed by the WebView2 arms only"
        )
    )]
    user_data: Option<PathBuf>,
    #[cfg_attr(
        not(target_os = "windows"),
        allow(
            dead_code,
            reason = "parsed on every OS, consumed by the WebView2 arms only"
        )
    )]
    second_user_data: Option<PathBuf>,
}

fn parse_store_id(value: &str) -> Result<[u8; 16], String> {
    if value.len() != 32 {
        return Err(format!(
            "--store-id needs 32 hex digits, got {}",
            value.len()
        ));
    }
    let mut id = [0_u8; 16];
    for (index, slot) in id.iter_mut().enumerate() {
        let hex = value
            .get(index * 2..index * 2 + 2)
            .ok_or("--store-id is not ASCII")?;
        *slot = u8::from_str_radix(hex, 16).map_err(|error| format!("--store-id: {error}"))?;
    }
    Ok(id)
}

fn parse_args() -> Result<Args, String> {
    let mut arm = None;
    let mut page = None;
    let mut second_page = None;
    let mut title = None;
    let mut control = None;
    let mut scheme = None;
    let mut host = None;
    let mut second_host = None;
    let mut store = Store::Ephemeral;
    let mut store_id = [0_u8; 16];
    let mut second_store = SecondStore::Shared;
    let mut handler = Handler::Strict;
    let mut root = None;
    let mut filter = Filter::Anchored;
    let mut ext_url = None;
    let mut user_data = None;
    let mut second_user_data = None;
    let mut iter = std::env::args().skip(1);
    while let Some(flag) = iter.next() {
        let value = iter
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--arm" => arm = Some(Arm::parse(&value)?),
            "--page" => page = Some(read_file(&value)?),
            "--page-b" => second_page = Some(read_file(&value)?),
            "--title" => title = Some(value),
            "--control" => control = Some(value),
            "--scheme" => scheme = Some(value),
            "--host" => host = Some(value),
            "--host-b" => second_host = Some(value),
            "--store" => store = Store::parse(&value)?,
            "--store-id" => store_id = parse_store_id(&value)?,
            "--second-store" => {
                second_store = match value.as_str() {
                    "shared" => SecondStore::Shared,
                    "separate" => SecondStore::Separate,
                    other => return Err(format!("unknown second store `{other}`")),
                }
            }
            "--handler" => handler = Handler::parse(&value)?,
            "--root" => root = Some(PathBuf::from(value)),
            "--filter" => {
                filter = match value.as_str() {
                    "anchored" => Filter::Anchored,
                    "loose" => Filter::Loose,
                    other => return Err(format!("unknown filter `{other}`")),
                }
            }
            "--ext-url" => ext_url = Some(value),
            "--user-data" => user_data = Some(PathBuf::from(value)),
            "--user-data-b" => second_user_data = Some(PathBuf::from(value)),
            other => return Err(format!("unknown flag `{other}`")),
        }
    }
    let page: Vec<u8> = page.ok_or("--page required")?;
    let second_page = second_page.unwrap_or_else(|| page.clone());
    let host: String = host.unwrap_or_else(|| String::from("app-a"));
    let second_host = second_host.unwrap_or_else(|| host.clone());
    if handler == Handler::Naive && root.is_none() {
        return Err(String::from("--handler naive needs --root"));
    }
    Ok(Args {
        arm: arm.ok_or("--arm required")?,
        page,
        second_page,
        title: title.ok_or("--title required")?,
        control: control.ok_or("--control required")?,
        scheme: scheme.unwrap_or_else(|| String::from("keld-exp-unnamed")),
        host,
        second_host,
        store,
        store_id,
        second_store,
        handler,
        root,
        filter,
        ext_url: ext_url.unwrap_or_else(|| String::from("about:blank")),
        user_data,
        second_user_data,
    })
}

fn read_file(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| format!("read {path}: {error}"))
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

const TEXT: &str = "text/plain; charset=utf-8";
const HTML: &str = "text/html; charset=utf-8";
const JS: &str = "text/javascript; charset=utf-8";
const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1" fill="#000"/></svg>"##;
/// Dedicated, module and shared workers all load this script.
const WORKER_JS: &[u8] = b"if (typeof postMessage === 'function') { postMessage('hi'); }\nself.onconnect = event => { event.ports[0].postMessage('shared'); };\n";
const SW_JS: &[u8] = b"self.addEventListener('install', () => self.skipWaiting());\n";

/// One answered request.
struct Served {
    status: u16,
    mime: &'static str,
    body: Vec<u8>,
    location: Option<String>,
}

impl Served {
    fn ok(mime: &'static str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            mime,
            body,
            location: None,
        }
    }

    fn empty(status: u16) -> Self {
        Self {
            status,
            mime: TEXT,
            body: Vec::new(),
            location: None,
        }
    }

    fn redirect(location: String) -> Self {
        Self {
            status: 302,
            mime: TEXT,
            body: Vec::new(),
            location: Some(location),
        }
    }

    /// The `WebView2` response needs a reason phrase; wry's builder does not.
    #[cfg_attr(
        not(target_os = "windows"),
        allow(dead_code, reason = "consumed by the WebView2 response only")
    )]
    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            302 => "Found",
            _ => "Not Found",
        }
    }
}

/// Host-owned bytes. The strict table answers exact paths only and refuses
/// hosts outside `allowed_hosts`; the naive mode (a mutation control) reads
/// whatever the decoded path names under `root`. Every request is recorded.
struct Table {
    page: Vec<u8>,
    /// Empty means the host check is switched off (`nohost`).
    allowed_hosts: Vec<String>,
    root: Option<PathBuf>,
    ext_url: String,
    served: Mutex<Vec<String>>,
}

impl Table {
    fn new(args: &Args) -> Self {
        let allowed_hosts = if args.handler == Handler::NoHost {
            Vec::new()
        } else {
            let mut allowed_hosts = vec![args.host.clone()];
            if args.second_host != args.host {
                allowed_hosts.push(args.second_host.clone());
            }
            allowed_hosts
        };
        Self {
            page: args.page.clone(),
            allowed_hosts,
            root: if args.handler == Handler::Naive {
                args.root.clone()
            } else {
                None
            },
            ext_url: args.ext_url.clone(),
            served: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, entry: &str) {
        if let Ok(mut served) = self.served.lock() {
            served.push(entry.replace([' ', ','], "_"));
        }
    }

    /// Answers one request and records `method:status:len:host_ok:uri`.
    fn lookup(&self, method: &str, uri: &str) -> Served {
        let scheme = uri.split_once("://").map_or("", |(scheme, _)| scheme);
        let host = authority_of(uri);
        let host_ok = self.allowed_hosts.is_empty()
            || host
                .as_deref()
                .is_some_and(|host| self.allowed_hosts.iter().any(|allowed| allowed == host));
        let path = path_of(uri);
        let served = if !host_ok {
            Served::empty(404)
        } else if let Some(root) = &self.root {
            self.naive(root, &path)
        } else {
            self.strict(scheme, host.as_deref().unwrap_or(""), &path, method)
        };
        self.record(&format!(
            "{method}:{}:{}:{host_ok}:{uri}",
            served.status,
            served.body.len()
        ));
        served
    }

    fn strict(&self, scheme: &str, host: &str, path: &str, method: &str) -> Served {
        match path {
            "" | "/" | "/index.html" | "index.html" => Served::ok(HTML, self.page.clone()),
            "/res/post" => Served::ok(TEXT, format!("gh1081 post method={method}").into_bytes()),
            "/res/px.svg" => Served::ok("image/svg+xml", SVG.to_vec()),
            "/w.js" => Served::ok(JS, WORKER_JS.to_vec()),
            "/sw.js" => Served::ok(JS, SW_JS.to_vec()),
            "/redir/same" => Served::redirect(format!("{scheme}://{host}/res/7")),
            "/redir/ext" => Served::redirect(self.ext_url.clone()),
            "/redir/other" => Served::redirect(format!("{scheme}-other://x/")),
            _ => {
                let numbered = path.strip_prefix("/res/").is_some_and(|index| {
                    !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit())
                });
                if numbered {
                    Served::ok(TEXT, format!("gh1081 resource {path}").into_bytes())
                } else {
                    Served::empty(404)
                }
            }
        }
    }

    /// Mutation control: the classic bug (decode, then join under the root,
    /// following whatever the filesystem resolves).
    fn naive(&self, root: &Path, path: &str) -> Served {
        let decoded = percent_decode_lossy(path);
        let relative = decoded.trim_start_matches('/');
        let file = if relative.is_empty() {
            root.join("index.html")
        } else {
            root.join(relative)
        };
        match std::fs::read(&file) {
            Ok(body) => {
                self.record(&format!("naive-open:{}", file.display()));
                let mime = if file
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("html"))
                {
                    HTML
                } else {
                    TEXT
                };
                Served::ok(mime, body)
            }
            Err(_) => Served::empty(404),
        }
    }

    fn summary(&self) -> String {
        self.served.lock().map_or_else(
            |_| String::from("count=? uris=poisoned"),
            |served| format!("count={} uris={}", served.len(), served.join(",")),
        )
    }
}

/// The authority of `scheme://authority/...`, or `None` without one.
fn authority_of(uri: &str) -> Option<String> {
    let (_, rest) = uri.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(rest[..end].to_owned())
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

/// Lossy `%XX` decoding, exactly what a naive handler does before joining.
fn percent_decode_lossy(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(hex) = text.get(index + 1..index + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod wry_lab {
    //! `WKWebView` and `WebKitGTK` through wry 0.56.1, the same crate and
    //! version keld-wv's interim backends use. `with_html` is today's URL-less
    //! load (`loadHTMLString(nil)` / `load_html(NULL)`); `with_custom_protocol`
    //! is `setURLSchemeHandler` / `register_uri_scheme` (+ `as_secure`).

    use super::{Args, Arm, Command, Control, SecondStore, Store, Table};
    use std::borrow::Cow;
    use std::sync::Arc;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopWindowTarget};
    use tao::window::{Window, WindowBuilder};
    use wry::http::header::CONTENT_TYPE;
    use wry::http::{Request, Response};
    use wry::{PageLoadEvent, WebView, WebViewBuilder};

    fn respond(
        table: &Table,
        control: &Control,
        view: &'static str,
        request: &Request<Vec<u8>>,
    ) -> Response<Cow<'static, [u8]>> {
        let uri = request.uri().to_string();
        let method = request.method().as_str().to_owned();
        let served = table.lookup(&method, &uri);
        control.send(&format!(
            "EV req view={view} method={method} status={} len={} uri={uri}",
            served.status,
            served.body.len()
        ));
        let mut builder = Response::builder()
            .status(served.status)
            .header(CONTENT_TYPE, served.mime)
            .header("Cache-Control", "no-store");
        if let Some(location) = served.location {
            builder = builder.header("Location", location);
        }
        builder
            .body(Cow::Owned(served.body))
            .unwrap_or_else(|_| Response::new(Cow::Owned(Vec::new())))
    }

    fn engine_facts() -> &'static str {
        if cfg!(target_os = "macos") {
            "wry=0.56.1;backend=wkwebview"
        } else {
            "wry=0.56.1;webkit2gtk-rs=2.0.2;backend=webkitgtk"
        }
    }

    /// One window with one webview.
    struct View {
        _window: Window,
        webview: WebView,
        initial: String,
    }

    /// State every view shares: the table, the control plane and, on Linux,
    /// the web contexts that outlive the views built on them.
    struct Shared {
        table: Arc<Table>,
        control: Control,
        #[cfg(target_os = "linux")]
        contexts: Vec<wry::WebContext>,
    }

    /// The store selection per engine: `(builder, whether this view registers
    /// the scheme)`. The second view shares a persistent store or gets an
    /// ephemeral one of its own; ephemeral stores are per view on both engines.
    fn store_builder<'a>(
        args: &Args,
        shared: &'a mut Shared,
        second: bool,
        view: &'static str,
    ) -> Result<(WebViewBuilder<'a>, bool), String> {
        let separate = second && args.second_store == SecondStore::Separate;
        let ephemeral = args.store == Store::Ephemeral || separate;
        if second && ephemeral && !separate {
            return Err(String::from(
                "a shared second view needs a persistent store (ephemeral stores are per view)",
            ));
        }
        #[cfg(target_os = "macos")]
        let selected = {
            use wry::WebViewBuilderExtDarwin as _;
            if !matches!(args.store, Store::Ephemeral | Store::Persistent) {
                return Err(format!(
                    "store {} is not available on WKWebView",
                    args.store.name()
                ));
            }
            let mut builder = WebViewBuilder::new().with_incognito(ephemeral);
            if !ephemeral {
                builder = builder.with_data_store_identifier(args.store_id);
            }
            let terminated = shared.control.clone();
            (
                builder.with_on_web_content_process_terminate_handler(move || {
                    terminated.send(&format!("EV webcontent terminated view={view}"));
                }),
                true,
            )
        };
        #[cfg(target_os = "linux")]
        let selected = if ephemeral {
            (WebViewBuilder::new().with_incognito(true), true)
        } else {
            let directory = match &args.store {
                Store::Directory(directory) => Some(directory.clone()),
                Store::Persistent => None,
                Store::Ephemeral | Store::InPrivate => {
                    return Err(format!(
                        "store {} is not available on WebKitGTK",
                        args.store.name()
                    ));
                }
            };
            if !second {
                shared.contexts.push(wry::WebContext::new(directory));
            }
            let context = shared
                .contexts
                .last_mut()
                .ok_or("no web context for the shared store")?;
            // The scheme is registered once per context (WebKitGTK refuses a
            // second registration), so a shared second view reuses it.
            (
                WebViewBuilder::new_with_web_context(context).with_incognito(false),
                !second,
            )
        };
        Ok(selected)
    }

    /// Builds the first (`second == false`) or the second view: its own window,
    /// store selection, handler registration and initial load.
    fn build_view(
        args: &Args,
        shared: &mut Shared,
        second: bool,
        target: &EventLoopWindowTarget<Command>,
    ) -> Result<View, String> {
        let view: &'static str = if second { "b" } else { "a" };
        let host = if second {
            &args.second_host
        } else {
            &args.host
        };
        let page = if second {
            &args.second_page
        } else {
            &args.page
        };
        let title = if second {
            format!("{} B", args.title)
        } else {
            args.title.clone()
        };
        let window = WindowBuilder::new()
            .with_title(&title)
            .with_inner_size(tao::dpi::LogicalSize::new(800.0, 600.0))
            .build(target)
            .map_err(|error| format!("window: {error}"))?;
        let initial = match args.arm {
            Arm::Inline => String::from("inline-html"),
            Arm::Scheme => format!("{}://{host}/", args.scheme),
            Arm::SchemeNoAuthority | Arm::Https => {
                return Err(format!("arm {} is Windows-only", args.arm.name()));
            }
        };
        let table = Arc::clone(&shared.table);
        let control = shared.control.clone();
        let (builder, register) = store_builder(args, shared, second, view)?;
        let load_control = control.clone();
        let builder = builder.with_on_page_load_handler(move |event, url| {
            let kind = match event {
                PageLoadEvent::Started => "started",
                PageLoadEvent::Finished => "finished",
            };
            load_control.send(&format!("EV load {kind} view={view} url={url}"));
        });
        let builder = if args.arm == Arm::Inline {
            builder.with_html(String::from_utf8_lossy(page).into_owned())
        } else {
            let builder = if register {
                builder.with_custom_protocol(args.scheme.clone(), move |_id, request| {
                    respond(&table, &control, view, &request)
                })
            } else {
                builder
            };
            builder.with_url(initial.clone())
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
        Ok(View {
            _window: window,
            webview,
            initial,
        })
    }

    /// Runs the harness until `quit`; never returns on success (tao owns exit).
    pub(super) fn run(args: Args, control: Control) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        keld_wv::webkitgtk::prepare_gpu_safe_mode_process()
            .map_err(|error| format!("gpu safe mode: {error}"))?;
        let event_loop = EventLoopBuilder::<Command>::with_user_event().build();
        let mut shared = Shared {
            table: Arc::new(Table::new(&args)),
            control: control.clone(),
            #[cfg(target_os = "linux")]
            contexts: Vec::new(),
        };
        let primary = build_view(&args, &mut shared, false, &event_loop)?;
        control.send(&format!(
            "HELLO pid={} arm={} initial={} store={} handler={} engine={}",
            std::process::id(),
            args.arm.name(),
            primary.initial,
            args.store.name(),
            args.handler.name(),
            engine_facts()
        ));
        super::forward_commands(&control, event_loop.create_proxy())?;
        // Second views stay alive (window and webview) until the harness exits.
        let mut secondary: Vec<View> = Vec::new();
        event_loop.run(move |event, target, control_flow| {
            *control_flow = ControlFlow::Wait;
            let quit = match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => true,
                Event::UserEvent(Command::Second) => {
                    let reply = match build_view(&args, &mut shared, true, target) {
                        Ok(view) => {
                            let initial = view.initial.clone();
                            secondary.push(view);
                            format!("ok initial={initial}")
                        }
                        Err(error) => format!("err:{error}"),
                    };
                    control.send(&format!("RE second {reply}"));
                    false
                }
                Event::UserEvent(command) => answer(command, &primary, &shared, &control),
                _ => false,
            };
            if quit {
                *control_flow = ControlFlow::Exit;
            }
        })
    }

    /// Answers one control command; `true` means quit.
    fn answer(command: Command, view: &View, shared: &Shared, control: &Control) -> bool {
        match command {
            Command::Url => control.send(&format!(
                "RE url {}",
                view.webview
                    .url()
                    .unwrap_or_else(|error| format!("err:{error}"))
            )),
            Command::Served => control.send(&format!("RE served {}", shared.table.summary())),
            Command::Reload => control.send(&format!(
                "RE reload {}",
                view.webview
                    .reload()
                    .map_or_else(|error| format!("err:{error}"), |()| String::from("ok"))
            )),
            Command::Second => control.send("RE second err:handled-by-the-event-loop"),
            Command::Procs => control.send("RE procs unavailable=wry-has-no-process-census"),
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

    use super::{Args, Arm, Command, Control, Filter, SecondStore, Store, Table};
    use std::path::Path;
    use std::sync::{Arc, mpsc};
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopWindowTarget};
    use tao::platform::windows::WindowExtWindows as _;
    use tao::window::{Window, WindowBuilder};
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        CORE_WEBVIEW_TARGET_PRODUCT_VERSION, COREWEBVIEW2_PROCESS_FAILED_KIND,
        COREWEBVIEW2_PROCESS_FAILED_REASON, COREWEBVIEW2_PROCESS_KIND,
        COREWEBVIEW2_WEB_ERROR_STATUS, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
        COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
        CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2_11,
        ICoreWebView2_22, ICoreWebView2Controller, ICoreWebView2CustomSchemeRegistration,
        ICoreWebView2Environment, ICoreWebView2Environment8, ICoreWebView2Environment10,
        ICoreWebView2EnvironmentOptions, ICoreWebView2ProcessFailedEventArgs2,
    };
    use webview2_com::{
        ContextMenuRequestedEventHandler, CoreWebView2CustomSchemeRegistration,
        CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
        CreateCoreWebView2EnvironmentCompletedHandler, NavigationCompletedEventHandler,
        NavigationStartingEventHandler, ProcessFailedEventHandler,
        WebResourceRequestedEventHandler, wait_with_pump,
    };
    use windows::Win32::Foundation::{E_POINTER, E_UNEXPECTED, HWND, RECT};
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
    use windows::Win32::UI::Shell::SHCreateMemStream;
    use windows::core::{BOOL, HSTRING, Interface as _, PCWSTR, PWSTR};

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

    fn environment(args: &Args, user_data: &Path) -> Result<ICoreWebView2Environment, String> {
        let options = CoreWebView2EnvironmentOptions::default();
        if matches!(args.arm, Arm::Scheme | Arm::SchemeNoAuthority) {
            let registration = CoreWebView2CustomSchemeRegistration::new(args.scheme.clone());
            let mut origins = vec![format!("{}://{}", args.scheme, args.host)];
            if args.second_host != args.host {
                origins.push(format!("{}://{}", args.scheme, args.second_host));
            }
            // SAFETY: the registration is exclusively owned here, before COM
            // sees it; these setters write its UnsafeCell fields on one thread.
            unsafe {
                registration.set_treat_as_secure(true);
                registration.set_has_authority_component(args.arm == Arm::Scheme);
                registration.set_allowed_origins(origins);
            }
            // SAFETY: same exclusive ownership of `options` before COM sees it.
            unsafe {
                options.set_scheme_registrations(vec![Some(
                    ICoreWebView2CustomSchemeRegistration::from(registration),
                )]);
            }
        }
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
        inprivate: bool,
    ) -> Result<ICoreWebView2Controller, String> {
        let (tx, rx) = mpsc::channel();
        let handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
            move |result, controller| {
                let controller = result.and(controller.ok_or_else(pointer_error));
                tx.send(controller)
                    .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
            },
        ));
        if inprivate {
            let env10 = environment
                .cast::<ICoreWebView2Environment10>()
                .map_err(|error| format!("ICoreWebView2Environment10: {error}"))?;
            // SAFETY: live environment on this STA thread.
            let options = unsafe { env10.CreateCoreWebView2ControllerOptions() }
                .map_err(|error| format!("controller options: {error}"))?;
            // SAFETY: the options object is exclusively owned here on the STA.
            unsafe { options.SetIsInPrivateModeEnabled(true) }
                .map_err(|error| format!("InPrivate option: {error}"))?;
            // SAFETY: live environment and options on this STA thread; `hwnd`
            // is a live tao window owned by the caller; the one-shot handler
            // is delivered through this thread's pump while `rx` waits.
            unsafe { env10.CreateCoreWebView2ControllerWithOptions(hwnd, &options, &handler) }
                .map_err(|error| format!("InPrivate controller launch: {error}"))?;
        } else {
            // SAFETY: `environment` was created on this STA thread and `hwnd` is a
            // live tao window owned by the caller; the one-shot handler is
            // delivered through this thread's pump while `rx` waits.
            unsafe { environment.CreateCoreWebView2Controller(hwnd, &handler) }
                .map_err(|error| format!("controller launch: {error}"))?;
        }
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
        view: &'static str,
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
            // the out-parameters are locals freed by `take_pwstr`.
            let (uri, request_method) = unsafe {
                let request = args.Request()?;
                let mut uri = PWSTR::null();
                request.Uri(&raw mut uri)?;
                let mut request_method = PWSTR::null();
                request.Method(&raw mut request_method)?;
                (take_pwstr(uri), take_pwstr(request_method))
            };
            let served = table.lookup(&request_method, &uri);
            control.send(&format!(
                "EV req view={view} method={request_method} status={} len={} uri={uri}",
                served.status,
                served.body.len()
            ));
            let mut headers = format!("Content-Type: {}\r\nCache-Control: no-store", served.mime);
            if let Some(location) = &served.location {
                headers.push_str("\r\nLocation: ");
                headers.push_str(location);
            }
            let headers = HSTRING::from(headers);
            let reason = HSTRING::from(served.reason());
            // SAFETY: the memory stream copies `body`; the response is created
            // on the environment that owns this webview and handed to the live
            // event args before the callback returns.
            unsafe {
                let stream = SHCreateMemStream(Some(&served.body));
                let response = environment.CreateWebResourceResponse(
                    stream.as_ref(),
                    i32::from(served.status),
                    &reason,
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
        view: &'static str,
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
                "EV navstart view={view} id={id} redirected={redirected} uri={uri}"
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
                "EV navdone view={view} id={id} success={success} status={status} source={source}"
            ));
            Ok(())
        }));
        // SAFETY: live webview on its STA; `token` is a local out-parameter.
        unsafe { webview.add_NavigationCompleted(&handler, &raw mut token) }
            .map_err(|error| format!("add_NavigationCompleted: {error}"))?;
        install_process_failed(webview, control, view)?;
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
                        menu.send(&format!(
                            "EV ctxmenu view={view} items={}",
                            labels.join("|")
                        ));
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

    /// Round 5 X9: the engine's own report of a lost process.
    fn install_process_failed(
        webview: &ICoreWebView2,
        control: &Control,
        view: &'static str,
    ) -> Result<(), String> {
        let failed = control.clone();
        let handler = ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
            let args = args.ok_or_else(pointer_error)?;
            // SAFETY: live event args on the STA; locals are the out-parameters
            // and the description string is freed by `take_pwstr`.
            let (kind, detail) = unsafe {
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND(0);
                args.ProcessFailedKind(&raw mut kind)?;
                let detail = match args.cast::<ICoreWebView2ProcessFailedEventArgs2>() {
                    Ok(failure) => {
                        let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON(0);
                        failure.Reason(&raw mut reason)?;
                        let mut exit_code = 0_i32;
                        failure.ExitCode(&raw mut exit_code)?;
                        let mut description = PWSTR::null();
                        failure.ProcessDescription(&raw mut description)?;
                        format!(
                            "reason={} exit={exit_code} desc={}",
                            reason.0,
                            take_pwstr(description).replace(' ', "_")
                        )
                    }
                    Err(error) => format!("details=unavailable:{error}"),
                };
                (kind.0, detail)
            };
            failed.send(&format!("EV procfail view={view} kind={kind} {detail}"));
            Ok(())
        }));
        let mut token = 0_i64;
        // SAFETY: live webview on its STA; `token` is a local out-parameter.
        unsafe { webview.add_ProcessFailed(&handler, &raw mut token) }
            .map_err(|error| format!("add_ProcessFailed: {error}"))
    }

    /// `pid:kind` for every process the environment reports (kind 1 = renderer).
    fn process_infos(environment: &ICoreWebView2Environment) -> String {
        match environment.cast::<ICoreWebView2Environment8>() {
            Ok(env8) => list_processes(&env8).unwrap_or_else(|error| format!("err:{error}")),
            Err(error) => format!("unavailable=ICoreWebView2Environment8:{error}"),
        }
    }

    fn list_processes(env8: &ICoreWebView2Environment8) -> windows::core::Result<String> {
        // SAFETY: live environment on its STA; every local is an out-parameter
        // of the live collection or one of its items.
        unsafe {
            let infos = env8.GetProcessInfos()?;
            let mut count = 0_u32;
            infos.Count(&raw mut count)?;
            let mut rows = Vec::new();
            for index in 0..count {
                let info = infos.GetValueAtIndex(index)?;
                let mut pid = 0_i32;
                info.ProcessId(&raw mut pid)?;
                let mut kind = COREWEBVIEW2_PROCESS_KIND(0);
                info.Kind(&raw mut kind)?;
                rows.push(format!("{pid}:{}", kind.0));
            }
            Ok(rows.join(","))
        }
    }

    struct View {
        webview: ICoreWebView2,
        /// Owns the browser attachment; dropping it closes the webview.
        _controller: ICoreWebView2Controller,
        _window: Window,
        initial: String,
        engine: String,
    }

    /// State every view shares: the table, the control plane and the first
    /// view's environment (a shared second view reuses it).
    struct Shared {
        table: Arc<Table>,
        control: Control,
        environment: Option<ICoreWebView2Environment>,
    }

    /// The first view's environment (created once and shared), or a second
    /// environment on the second user data folder for a separate store.
    fn select_environment(
        args: &Args,
        shared: &mut Shared,
        separate: bool,
    ) -> Result<ICoreWebView2Environment, String> {
        if let Store::Directory(_) = &args.store {
            return Err(String::from("store dir: is not available on WebView2"));
        }
        if separate {
            let user_data = args
                .second_user_data
                .as_deref()
                .ok_or("--user-data-b is required for a separate second store")?;
            return environment(args, user_data);
        }
        if let Some(existing) = &shared.environment {
            return Ok(existing.clone());
        }
        let user_data = args
            .user_data
            .as_deref()
            .ok_or("--user-data is required on Windows")?;
        let created = environment(args, user_data)?;
        shared.environment = Some(created.clone());
        Ok(created)
    }

    /// Installs the arm's request filter and returns `(initial URL, filter API)`.
    fn install_arm(
        args: &Args,
        shared: &Shared,
        view: &'static str,
        host: &str,
        webview: &ICoreWebView2,
        environment: &ICoreWebView2Environment,
    ) -> Result<(String, &'static str), String> {
        let install = |pattern: &str| {
            install_resource_handler(
                webview,
                environment.clone(),
                pattern,
                Arc::clone(&shared.table),
                shared.control.clone(),
                view,
            )
        };
        match args.arm {
            Arm::Inline => Ok((String::from("inline-html"), "none")),
            Arm::Scheme => {
                let url = format!("{}://{host}/", args.scheme);
                let filter = install(&filter_pattern(args, host, &url))?;
                Ok((url, filter))
            }
            Arm::SchemeNoAuthority => {
                let filter = install(&format!("{}:*", args.scheme))?;
                Ok((format!("{}:index.html", args.scheme), filter))
            }
            Arm::Https => {
                let url = format!("https://{host}.invalid/");
                let filter = install(&filter_pattern(args, &format!("{host}.invalid"), &url))?;
                Ok((url, filter))
            }
        }
    }

    fn create_view(
        args: &Args,
        shared: &mut Shared,
        second: bool,
        target: &EventLoopWindowTarget<Command>,
    ) -> Result<View, String> {
        let view: &'static str = if second { "b" } else { "a" };
        let host = if second {
            &args.second_host
        } else {
            &args.host
        };
        let page = if second {
            &args.second_page
        } else {
            &args.page
        };
        let title = if second {
            format!("{} B", args.title)
        } else {
            args.title.clone()
        };
        let window = WindowBuilder::new()
            .with_title(&title)
            .with_inner_size(tao::dpi::LogicalSize::new(800.0, 600.0))
            .build(target)
            .map_err(|error| format!("window: {error}"))?;
        let separate = second && args.second_store == SecondStore::Separate;
        let environment = select_environment(args, shared, separate)?;
        let hwnd = HWND(window.hwnd() as *mut core::ffi::c_void);
        let controller = controller(
            &environment,
            hwnd,
            args.store == Store::InPrivate && !separate,
        )?;
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
        install_navigation_observers(&webview, &shared.control, view)?;
        let (initial, filter) = install_arm(args, shared, view, host, &webview, &environment)?;
        let engine = engine_facts(&environment, filter, args.filter)?;
        navigate(&webview, args.arm, &initial, page)?;
        Ok(View {
            webview,
            _controller: controller,
            _window: window,
            initial,
            engine,
        })
    }

    /// The anchored candidate filter, or the deliberately loose mutation control.
    fn filter_pattern(args: &Args, host: &str, url: &str) -> String {
        match args.filter {
            Filter::Anchored => format!("{url}*"),
            Filter::Loose => format!("*{host}*"),
        }
    }

    fn engine_facts(
        environment: &ICoreWebView2Environment,
        filter: &str,
        shape: Filter,
    ) -> Result<String, String> {
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
        let shape = match shape {
            Filter::Anchored => "anchored",
            Filter::Loose => "loose",
        };
        Ok(format!(
            "webview2-runtime={runtime};sdk-target={sdk_target};filter={filter};filter-shape={shape}"
        ))
    }

    fn navigate(
        webview: &ICoreWebView2,
        arm: Arm,
        initial: &str,
        page: &[u8],
    ) -> Result<(), String> {
        let navigated = if arm == Arm::Inline {
            let html = HSTRING::from(String::from_utf8_lossy(page).as_ref());
            // SAFETY: live webview on its STA; `html` outlives the call.
            unsafe { webview.NavigateToString(&html) }
        } else {
            let url = HSTRING::from(initial);
            // SAFETY: live webview on its STA; `url` outlives the call.
            unsafe { webview.Navigate(&url) }
        };
        navigated.map_err(|error| format!("navigate: {error}"))
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
    pub(super) fn run(args: Args, control: Control) -> Result<(), String> {
        // SAFETY: the first COM call on this thread, before any thread-affine
        // WebView2 object exists (S_FALSE if tao already initialised the STA).
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|error| format!("CoInitializeEx: {error}"))?;
        let event_loop = EventLoopBuilder::<Command>::with_user_event().build();
        let mut shared = Shared {
            table: Arc::new(Table::new(&args)),
            control: control.clone(),
            environment: None,
        };
        let primary = create_view(&args, &mut shared, false, &event_loop)?;
        control.send(&format!(
            "HELLO pid={} arm={} initial={} store={} handler={} engine={}",
            std::process::id(),
            args.arm.name(),
            primary.initial,
            args.store.name(),
            args.handler.name(),
            primary.engine
        ));
        super::forward_commands(&control, event_loop.create_proxy())?;
        // Second views stay alive (window, controller and webview) until exit.
        let mut secondary: Vec<View> = Vec::new();
        event_loop.run(move |event, target, control_flow| {
            *control_flow = ControlFlow::Wait;
            let quit = match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => true,
                Event::UserEvent(Command::Second) => {
                    let reply = match create_view(&args, &mut shared, true, target) {
                        Ok(view) => {
                            let initial = view.initial.clone();
                            secondary.push(view);
                            format!("ok initial={initial}")
                        }
                        Err(error) => format!("err:{error}"),
                    };
                    control.send(&format!("RE second {reply}"));
                    false
                }
                Event::UserEvent(command) => answer(command, &primary, &shared, &control),
                _ => false,
            };
            if quit {
                *control_flow = ControlFlow::Exit;
            }
        })
    }

    /// Answers one control command; `true` means quit.
    fn answer(command: Command, view: &View, shared: &Shared, control: &Control) -> bool {
        match command {
            Command::Url => control.send(&format!("RE url {}", source(&view.webview))),
            Command::Served => control.send(&format!("RE served {}", shared.table.summary())),
            Command::Reload => {
                // SAFETY: live webview on its STA.
                let reloaded = unsafe { view.webview.Reload() };
                let outcome =
                    reloaded.map_or_else(|error| format!("err:{error}"), |()| String::from("ok"));
                control.send(&format!("RE reload {outcome}"));
            }
            Command::Second => control.send("RE second err:handled-by-the-event-loop"),
            Command::Procs => {
                let listed = shared
                    .environment
                    .as_ref()
                    .map_or_else(|| String::from("unavailable=no-environment"), process_infos);
                control.send(&format!("RE procs {listed}"));
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
        wry_lab::run(args, control)
    }
    #[cfg(target_os = "windows")]
    {
        webview2_lab::run(args, control)
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
