//! Phase 1 hello-world window (delegates to `keld-wv`).
//!
//! Callers go through this module so `AppKit`/`NSApplication` and the UI thread
//! stay inside the host stack (`keld-core` → `keld-wv` `WKWebView` backend).
//! Invoke on the process main thread; do not start a second event loop.

use std::fs;
use std::path::Path;

use keld_wv::{HELLO_HTML, WvError, run_hello_window as wv_run_hello};

/// Prepares the current process for its platform webview backend.
///
/// On Linux NVIDIA+Wayland this may replace the process through exact
/// `/proc/self/exe` with the DMA-BUF safe-mode environment applied. Shipping
/// binaries call this from their process-entry dispatcher before creating
/// threads or non-repeatable state. Other platforms return immediately.
///
/// # Errors
///
/// Returns [`WvError`] if Linux exact-self re-exec fails.
pub fn prepare_webview_process() -> Result<(), WvError> {
    #[cfg(target_os = "linux")]
    {
        let _ = keld_wv::webkitgtk::prepare_gpu_safe_mode_process()?;
    }
    Ok(())
}

/// Window title when config/args do not supply one.
pub const DEFAULT_HELLO_TITLE: &str = "Keld";

/// Renderer path when `keld.config.ts` has no `renderer` field.
pub const DEFAULT_RENDERER: &str = "index.html";

/// Bun entry path used by the hello template and boot compiler.
pub const DEFAULT_ENTRY: &str = "src/main.ts";

/// Opens the default Keld hello window until the user closes it.
/// On Linux, process entry must call [`prepare_webview_process`] first.
///
/// # Errors
///
/// Forwards [`keld_wv::WvError`] from the webview layer.
pub fn run_hello_window() -> Result<(), WvError> {
    run_hello_window_titled(DEFAULT_HELLO_TITLE)
}

/// Opens a hello window with `title` until the user closes it.
/// On Linux, process entry must call [`prepare_webview_process`] first.
///
/// # Errors
///
/// Forwards [`keld_wv::WvError`] from the webview layer.
pub fn run_hello_window_titled(title: &str) -> Result<(), WvError> {
    run_hello_window_html(title, HELLO_HTML)
}

/// Opens a window with `title` rendering inline `html` until the user closes it.
///
/// Used by `keld dev` with the project's renderer file contents. `keld hello`
/// and `keld-host --hello` keep the compiled `HELLO_HTML` constant.
/// On Linux, process entry must call [`prepare_webview_process`] first.
///
/// # Errors
///
/// Forwards [`keld_wv::WvError`] from the webview layer.
pub fn run_hello_window_html(title: &str, html: &str) -> Result<(), WvError> {
    wv_run_hello(title, html)
}

/// `--title <name>` or `--title=<name>` from argv (binary name included is fine).
#[must_use]
pub fn hello_title_from_args<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let arg = arg.as_ref();
        if let Some(value) = arg.strip_prefix("--title=") {
            let value = value.trim();
            if value.is_empty() {
                return None;
            }
            return Some(value.to_owned());
        }
        if arg == "--title" {
            let value = iter.next()?.as_ref().trim().to_owned();
            if value.is_empty() || value.starts_with('-') {
                return None;
            }
            return Some(value);
        }
    }
    None
}

fn quoted_config_field(source: &str, prefix: &str) -> Option<String> {
    for line in source.lines() {
        let Some(rest) = line.trim().strip_prefix(prefix) else {
            continue;
        };
        let rest = rest.trim().trim_end_matches(',').trim();
        let Some(inner) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
            continue;
        };
        if !inner.is_empty() {
            return Some(inner.to_owned());
        }
    }
    None
}

/// Reads `name: "..."` from a hello-template `keld.config.ts`.
#[must_use]
pub fn title_from_config_ts(source: &str) -> Option<String> {
    quoted_config_field(source, "name:")
}

/// Reads `renderer: "..."` from a hello-template `keld.config.ts`.
#[must_use]
pub fn renderer_from_config_ts(source: &str) -> Option<String> {
    quoted_config_field(source, "renderer:")
}

/// Reads `entry: "..."` from a hello-template `keld.config.ts`.
#[must_use]
pub fn entry_from_config_ts(source: &str) -> Option<String> {
    quoted_config_field(source, "entry:")
}

/// `name` from `project_root/keld.config.ts`, if the file exists and parses.
#[must_use]
pub fn read_config_title(project_root: &Path) -> Option<String> {
    let source = fs::read_to_string(project_root.join("keld.config.ts")).ok()?;
    title_from_config_ts(&source)
}

/// `renderer` from `project_root/keld.config.ts`, if the file exists and parses.
#[must_use]
pub fn read_config_renderer(project_root: &Path) -> Option<String> {
    let source = fs::read_to_string(project_root.join("keld.config.ts")).ok()?;
    renderer_from_config_ts(&source)
}

/// `entry` from `project_root/keld.config.ts`, if the file exists and parses.
#[must_use]
pub fn read_config_entry(project_root: &Path) -> Option<String> {
    let source = fs::read_to_string(project_root.join("keld.config.ts")).ok()?;
    entry_from_config_ts(&source)
}

/// `--title` wins; else config `name`; else [`DEFAULT_HELLO_TITLE`].
#[must_use]
pub fn resolve_hello_title(args: &[String], project_root: Option<&Path>) -> String {
    hello_title_from_args(args)
        .or_else(|| project_root.and_then(read_config_title))
        .unwrap_or_else(|| DEFAULT_HELLO_TITLE.to_owned())
}

/// First argv token `keld-host --hello` does not accept.
///
/// Allowed after argv0: `--hello`, `--title <name>`, `--title=<name>`.
/// `--title` with no following value is still consumed (title resolution
/// treats it as absent). `--title` followed by a token that starts with `-`
/// does not consume that token as a title; the token is the unknown arg.
#[must_use]
pub fn host_hello_unknown_arg<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    let _argv0 = iter.next();
    while let Some(arg) = iter.next() {
        let arg = arg.as_ref();
        if arg == "--hello" || arg.starts_with("--title=") {
            continue;
        }
        if arg == "--title" {
            let Some(value) = iter.next() else {
                continue;
            };
            let value = value.as_ref();
            if value.starts_with('-') {
                return Some(value.to_owned());
            }
            continue;
        }
        return Some(arg.to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_ENTRY, DEFAULT_HELLO_TITLE, DEFAULT_RENDERER, entry_from_config_ts,
        hello_title_from_args, host_hello_unknown_arg, read_config_entry, renderer_from_config_ts,
        resolve_hello_title, title_from_config_ts,
    };

    #[test]
    fn default_title_is_keld() {
        assert_eq!(DEFAULT_HELLO_TITLE, "Keld");
    }

    #[test]
    fn config_file_entry_reads_value_and_missing_is_none() {
        let temp = tempfile::tempdir().expect("config entry root");
        assert_eq!(read_config_entry(temp.path()), None);
        std::fs::write(
            temp.path().join("keld.config.ts"),
            "export default {\n  entry: \"src/primary.ts\",\n};\n",
        )
        .expect("write config entry");
        assert_eq!(
            read_config_entry(temp.path()),
            Some(String::from("src/primary.ts"))
        );
    }

    #[test]
    fn title_flag_space_and_equals() {
        assert_eq!(
            hello_title_from_args(["keld-host", "--hello", "--title", "Acme"]),
            Some(String::from("Acme"))
        );
        assert_eq!(
            hello_title_from_args(["keld-host", "--hello", "--title=Acme"]),
            Some(String::from("Acme"))
        );
        assert_eq!(hello_title_from_args(["keld-host", "--hello"]), None);
    }

    #[test]
    fn title_flag_rejects_empty_and_dangling() {
        assert_eq!(hello_title_from_args(["--title="]), None);
        assert_eq!(hello_title_from_args(["--title"]), None);
        assert_eq!(hello_title_from_args(["--title", "--hello"]), None);
    }

    #[test]
    fn host_hello_accepts_title_flags() {
        assert_eq!(host_hello_unknown_arg(["keld-host", "--hello"]), None);
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "--title", "Acme"]),
            None
        );
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "--title=Acme"]),
            None
        );
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "--title"]),
            None
        );
    }

    #[test]
    fn host_hello_rejects_unknown_flags() {
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "--title", "--devtools"]),
            Some(String::from("--devtools"))
        );
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "--devtools"]),
            Some(String::from("--devtools"))
        );
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "--permissions"]),
            Some(String::from("--permissions"))
        );
        assert_eq!(
            host_hello_unknown_arg(["keld-host", "--hello", "extra"]),
            Some(String::from("extra"))
        );
    }

    #[test]
    fn config_name_from_hello_template_shape() {
        let source = r#"export default {
  name: "demo-app",
  entry: "src/main.ts",
  renderer: "index.html",
} as const;
"#;
        assert_eq!(title_from_config_ts(source).as_deref(), Some("demo-app"));
        assert_eq!(
            renderer_from_config_ts(source).as_deref(),
            Some("index.html")
        );
        assert_eq!(entry_from_config_ts(source).as_deref(), Some("src/main.ts"));
    }

    #[test]
    fn config_name_ignores_missing_or_empty() {
        assert_eq!(title_from_config_ts("export default {}"), None);
        assert_eq!(title_from_config_ts("name: \"\""), None);
        assert_eq!(renderer_from_config_ts("export default {}"), None);
        assert_eq!(renderer_from_config_ts("renderer: \"\""), None);
        assert_eq!(entry_from_config_ts("entry: \"\""), None);
    }

    #[test]
    fn default_renderer_is_index_html() {
        assert_eq!(DEFAULT_RENDERER, "index.html");
        assert_eq!(DEFAULT_ENTRY, "src/main.ts");
    }

    #[test]
    fn args_override_config_and_default() {
        let args = vec![String::from("--title"), String::from("from-args")];
        assert_eq!(resolve_hello_title(&args, None), "from-args");
        assert_eq!(resolve_hello_title(&[], None), "Keld");
    }

    #[test]
    fn config_file_title_when_no_args() {
        let dir = std::env::temp_dir().join(format!(
            "keld-core-hello-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("keld.config.ts"),
            "export default {\n  name: \"from-config\",\n} as const;\n",
        )
        .expect("write");
        assert_eq!(resolve_hello_title(&[], Some(&dir)), "from-config");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
