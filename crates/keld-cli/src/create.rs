//! `keld create` — scaffold the hello-world template (KEL-29).

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::template::HELLO_TEMPLATE;

/// Errors from project scaffolding.
#[derive(Debug)]
pub enum CreateError {
    /// Project name failed validation.
    InvalidName {
        /// The rejected name (empty when the caller omitted it).
        name: String,
        /// Why the name was rejected.
        reason: &'static str,
    },
    /// Target directory already exists.
    Exists(PathBuf),
    /// Filesystem failure.
    Io(std::io::Error),
}

impl std::fmt::Display for CreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName { name, reason } if name.is_empty() => {
                write!(
                    f,
                    "KELD-CLI-020: invalid project name — {reason}. \
                     Pass a name: `keld create my-app`."
                )
            }
            Self::InvalidName { name, reason } => {
                write!(
                    f,
                    "KELD-CLI-020: invalid project name `{name}` — {reason}. \
                     Use lowercase letters, numbers, and hyphens."
                )
            }
            Self::Exists(path) => write!(
                f,
                "KELD-CLI-021: directory already exists — {}. \
                 Choose another name or remove the folder.",
                path.display()
            ),
            Self::Io(e) => write!(
                f,
                "KELD-CLI-022: failed to write template — {e}. \
                 Check the parent path exists and is a writable directory."
            ),
        }
    }
}

impl std::error::Error for CreateError {}

impl From<std::io::Error> for CreateError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Validates a project name for the hello template.
///
/// # Errors
///
/// Returns [`CreateError::InvalidName`] when the name is empty, contains
/// characters outside `[a-z0-9-]`, or starts/ends with a hyphen.
pub fn validate_name(name: &str) -> Result<(), CreateError> {
    if name.is_empty() {
        return Err(CreateError::InvalidName {
            name: name.to_owned(),
            reason: "name is empty",
        });
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(CreateError::InvalidName {
            name: name.to_owned(),
            reason: "use only lowercase letters, digits, and hyphens",
        });
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err(CreateError::InvalidName {
            name: name.to_owned(),
            reason: "name cannot start or end with a hyphen",
        });
    }
    Ok(())
}

/// Writes the hello template into `parent/name`.
///
/// # Errors
///
/// Returns [`CreateError`] on validation failure, existing directory, or I/O errors.
pub fn create_project(parent: &Path, name: &str) -> Result<PathBuf, CreateError> {
    validate_name(name)?;
    let root = parent.join(name);
    match fs::create_dir(&root) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            return Err(CreateError::Exists(root));
        }
        Err(e) => return Err(e.into()),
    }
    for file in HELLO_TEMPLATE {
        let rendered = file.render(name);
        let dest = root.join(file.path);
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&dest, rendered)?;
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression for Ubuntu `KELD-CORE-037`: Bun `from "./kipc-transport.ts"`
    /// must resolve a real sidecar in the created hello tree.
    fn assert_created_hello_keeps_kipc_transport(root: &Path, main: &str) {
        let sidecar = root.join("src").join("kipc-transport.ts");
        assert!(
            main.contains("from \"./kipc-transport.ts\""),
            "hello main.ts must import the sidecar: {main}"
        );
        assert!(
            sidecar.is_file(),
            "created hello tree imported ./kipc-transport.ts but src/kipc-transport.ts is missing"
        );
    }

    fn file_count(path: &Path) -> usize {
        let mut n = 0;
        for entry in fs::read_dir(path).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                n += file_count(&path);
            } else {
                n += 1;
            }
        }
        n
    }

    fn css_block<'a>(document: &'a str, selector: &str) -> &'a str {
        let opening = format!("{selector} {{");
        let start = document
            .find(&opening)
            .unwrap_or_else(|| panic!("missing `{selector}` CSS block: {document}"));
        let body = &document[start + opening.len()..];
        let end = body
            .find('}')
            .unwrap_or_else(|| panic!("unterminated `{selector}` CSS block: {document}"));
        &body[..end]
    }

    #[test]
    fn rejects_empty_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = create_project(dir.path(), "").expect_err("empty name must fail");
        let msg = err.to_string();
        assert!(msg.contains("KELD-CLI-020"), "{msg}");
        assert!(msg.contains("empty"), "{msg}");
        assert!(msg.contains("keld create"), "{msg}");
        assert!(
            !dir.path().join("keld.config.ts").exists(),
            "empty name must not write into the parent directory"
        );
    }

    #[test]
    fn rejects_uppercase_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = create_project(dir.path(), "Hello").expect_err("uppercase must fail");
        let msg = err.to_string();
        assert!(msg.contains("KELD-CLI-020"), "{msg}");
        assert!(msg.contains("`Hello`"), "{msg}");
        assert!(msg.contains("lowercase"), "{msg}");
        assert!(
            !dir.path().join("Hello").exists(),
            "rejected name must not create a directory"
        );
        assert!(
            !dir.path().join("hello").exists(),
            "must not auto-lowercase; uppercase is a hard reject"
        );
    }

    #[test]
    fn rejects_path_separator_in_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = create_project(dir.path(), "foo/bar").expect_err("slash must fail");
        let msg = err.to_string();
        assert!(msg.contains("KELD-CLI-020"), "{msg}");
        assert!(msg.contains("foo/bar"), "{msg}");
        assert!(
            !dir.path().join("foo").exists(),
            "path-like name must not create nested directories"
        );
    }

    #[test]
    fn invalid_parent_file_is_cli_022() {
        let dir = tempfile::tempdir().expect("tempdir");
        let parent = dir.path().join("not-a-dir");
        fs::write(&parent, "x").expect("write file where a directory is required");
        let err = create_project(&parent, "app").expect_err("file parent must fail");
        let msg = err.to_string();
        assert!(msg.contains("KELD-CLI-022"), "{msg}");
        assert!(msg.contains("writable"), "{msg}");
        assert!(!parent.join("app").exists());
    }

    #[test]
    fn refuses_existing_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = create_project(dir.path(), "app").expect("first create");
        let original = fs::read_to_string(root.join("keld.config.ts")).expect("read");
        let err = create_project(dir.path(), "app").expect_err("second create must fail");
        let msg = err.to_string();
        assert!(msg.contains("KELD-CLI-021"), "{msg}");
        assert!(msg.contains("app"), "{msg}");
        assert_eq!(
            fs::read_to_string(root.join("keld.config.ts")).expect("reread"),
            original,
            "existing project must not be overwritten"
        );
    }

    #[test]
    fn existing_file_at_destination_is_cli_021() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("app");
        fs::write(&dest, "x").expect("file where the project dir would go");
        let err = create_project(dir.path(), "app").expect_err("file must not be overwritten");
        let msg = err.to_string();
        assert!(msg.contains("KELD-CLI-021"), "{msg}");
        assert_eq!(fs::read_to_string(&dest).expect("reread"), "x");
    }

    fn formatter_fixture_command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
        let mut command = std::process::Command::new(program);
        // A fixture owns its Git repository and config. Preserve ordinary OS/Bun
        // inputs while excluding all Git controls, including GIT_CONFIG.
        command
            .env_clear()
            .envs(std::env::vars_os().filter(|(key, _)| {
                !key.to_string_lossy()
                    .to_ascii_uppercase()
                    .starts_with("GIT_")
            }));
        command
    }

    fn assert_disposable_directory(path: &Path, parent: &Path) {
        let metadata = fs::symlink_metadata(path).expect("disposable directory metadata");
        assert!(metadata.is_dir() && !metadata.is_symlink());
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            assert_eq!(
                metadata.file_attributes() & 0x400,
                0,
                "fixture reparse point"
            );
        }
        assert_eq!(
            path.canonicalize()
                .expect("canonical disposable directory")
                .parent(),
            Some(parent),
            "disposable directory must be directly beneath its owner"
        );
    }

    fn assert_formatter_preserves_inherited_git_target(config_only: bool) {
        const FORMATTER_TEST: &str =
            "create::tests::created_app_native_formatter_checks_repairs_and_is_idempotent";
        let parent = std::env::temp_dir()
            .canonicalize()
            .expect("canonical task temp");
        let fixture = tempfile::tempdir().expect("disposable Git isolation fixture");
        assert_disposable_directory(fixture.path(), &parent);
        let owner = fixture
            .path()
            .canonicalize()
            .expect("canonical fixture owner");
        let caller = fixture.path().join("caller");
        let child_temp = fixture.path().join("child-temp");
        for directory in [&caller, &child_temp] {
            fs::create_dir(directory).expect("disposable child directory");
            assert_disposable_directory(directory, &owner);
        }
        let initialized = formatter_fixture_command("git")
            .args(["init", "-q"])
            .current_dir(&caller)
            .output()
            .expect("initialize disposable caller");
        assert!(initialized.status.success(), "{initialized:?}");
        let git_dir = caller.join(".git");
        assert_disposable_directory(&git_dir, &caller.canonicalize().expect("caller identity"));
        fs::write(caller.join("sentinel.txt"), b"caller source must survive\n")
            .expect("caller source sentinel");
        let staged = formatter_fixture_command("git")
            .args(["add", "sentinel.txt"])
            .current_dir(&caller)
            .output()
            .expect("stage disposable caller sentinel");
        assert!(staged.status.success(), "{staged:?}");
        let config_sentinel = fixture.path().join("sentinel.config");
        fs::write(&config_sentinel, b"[fixture]\n\tmarker = preserve\n").expect("config sentinel");
        let protected = [
            git_dir.join("config"),
            git_dir.join("HEAD"),
            git_dir.join("index"),
            config_sentinel.clone(),
        ];
        let before = protected
            .each_ref()
            .map(|path| fs::read(path).expect("sentinel bytes"));
        let mut child =
            formatter_fixture_command(std::env::current_exe().expect("current test executable"));
        child
            .args([FORMATTER_TEST, "--exact", "--nocapture", "--color", "never"])
            .current_dir(&child_temp)
            .env("TEMP", &child_temp)
            .env("TMP", &child_temp)
            .env("TMPDIR", &child_temp);
        if config_only {
            child.env("GIT_CONFIG", &config_sentinel);
        } else {
            child
                .env("GIT_DIR", &git_dir)
                .env("GIT_COMMON_DIR", &git_dir)
                .env("GIT_WORK_TREE", &caller)
                .env("GIT_INDEX_FILE", git_dir.join("index"));
        }
        let output = child
            .output()
            .expect("run formatter with disposable Git poison");
        for (path, expected) in protected.iter().zip(before) {
            assert_eq!(
                fs::read(path).expect("sentinel after child"),
                expected,
                "formatter changed disposable caller/config sentinel {} (config_only={config_only})",
                path.display()
            );
        }
        assert!(output.status.success(), "formatter child: {output:?}");
        let stdout = String::from_utf8(output.stdout).expect("test output UTF-8");
        assert!(stdout.contains(FORMATTER_TEST), "{stdout}");
        assert!(stdout.contains("1 passed; 0 failed"), "{stdout}");
    }

    #[test]
    fn created_app_native_formatter_preserves_inherited_git_location() {
        assert_formatter_preserves_inherited_git_target(false);
    }

    #[test]
    fn created_app_native_formatter_preserves_inherited_git_config() {
        assert_formatter_preserves_inherited_git_target(true);
    }

    #[test]
    fn created_app_native_formatter_checks_repairs_and_is_idempotent() {
        let git = |cwd: &Path, args: &[&str]| {
            let output = formatter_fixture_command("git")
                .args(args)
                .current_dir(cwd)
                .output()
                .expect("run fixture Git");
            assert!(output.status.success(), "fixture Git {args:?}: {output:?}");
        };
        for name in ["formatted-app".to_owned(), "a".repeat(100)] {
            let dir = tempfile::tempdir().expect("tempdir");
            // Model a generated app under another checkout's ignored scratch
            // tree: Bun must still inspect its own files, including new ones.
            git(dir.path(), &["init", "-q"]);
            git(dir.path(), &["config", "maintenance.auto", "false"]);
            fs::write(dir.path().join(".gitignore"), "ignored/\n").expect("outer ignore");
            let ignored = dir.path().join("ignored");
            fs::create_dir(&ignored).expect("ignored fixture parent");
            let root = create_project(&ignored, &name).expect("create app");
            let inherited_ignore = formatter_fixture_command("git")
                .args(["check-ignore", "--quiet"])
                .arg(root.join("src/main.ts"))
                .current_dir(dir.path())
                .status()
                .expect("check outer ignore");
            assert!(
                inherited_ignore.success(),
                "outer ignore must hide app files"
            );

            // Isolate the app as its own Git root. Without this boundary,
            // format:check incorrectly succeeds on unformatted ignored files.
            git(&root, &["init", "-q"]);
            git(&root, &["config", "maintenance.auto", "false"]);
            let run = |script: &str| {
                formatter_fixture_command("bun")
                    .args(["run", script])
                    .current_dir(&root)
                    .output()
                    .expect("run native formatter script")
            };
            let fresh = run("format:check");
            assert!(fresh.status.success(), "fresh app: {fresh:?}");

            let source = root.join("src/unformatted source.ts");
            let unformatted = b"export const value={answer:42};\n";
            fs::write(&source, unformatted).expect("plant unformatted source");
            let rejected = run("format:check");
            assert!(!rejected.status.success(), "unformatted app passed");
            assert_eq!(fs::read(&source).expect("read after check"), unformatted);

            let formatted = run("format");
            assert!(formatted.status.success(), "format app: {formatted:?}");
            let bytes = fs::read(&source).expect("formatted source");
            assert_ne!(bytes, unformatted);
            assert!(run("format:check").status.success());
            assert!(run("format").status.success());
            assert_eq!(fs::read(&source).expect("second format"), bytes);
        }
    }

    #[test]
    fn created_app_omits_unavailable_native_lint_script() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = create_project(dir.path(), "demo").expect("create");
        let package = fs::read_to_string(root.join("package.json")).expect("package");
        let manifest: serde_json::Value = serde_json::from_str(&package).expect("package JSON");
        assert!(
            manifest["scripts"].get("lint").is_none(),
            "native lint remains deferred until stable Bun supports it: {package}"
        );
    }

    #[test]
    fn writes_expected_file_contents() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = create_project(dir.path(), "demo").expect("create");
        assert_eq!(
            file_count(&root),
            8,
            "hello template is exactly eight files"
        );

        let config = fs::read_to_string(root.join("keld.config.ts")).expect("config");
        assert!(config.contains("name: \"demo\""), "{config}");
        assert!(config.contains("entry: \"src/main.ts\""), "{config}");
        assert!(config.contains("renderer: \"index.html\""), "{config}");
        assert!(!config.contains("{{name}}"), "{config}");

        let package = fs::read_to_string(root.join("package.json")).expect("package");
        assert!(package.contains("\"name\": \"demo\""), "{package}");
        assert!(package.contains("\"type\": \"module\""), "{package}");
        assert!(!package.contains("{{name}}"), "{package}");

        let html = fs::read_to_string(root.join("index.html")).expect("html");
        assert!(html.contains("<title>demo</title>"), "{html}");
        assert!(html.contains("<h1>demo</h1>"), "{html}");
        let root_style = css_block(&html, "html");
        assert!(root_style.contains("background: #000;"), "{html}");
        assert!(root_style.contains("color-scheme: dark;"), "{html}");
        let body_style = css_block(&html, "body");
        assert!(body_style.contains("background: #000;"), "{html}");
        assert!(body_style.contains("color: #fff;"), "{html}");
        assert!(!html.contains("{{name}}"), "{html}");

        let main = fs::read_to_string(root.join("src/main.ts")).expect("main");
        assert!(main.contains("KELD-CLI-010"), "{main}");
        assert!(main.contains("KELD_APP_LINK"), "{main}");
        assert!(main.contains("AppLinkSession"), "{main}");
        assert!(main.contains("export class AppLinkSession"), "{main}");
        assert!(
            main.contains("export async function echoRoundtrip"),
            "{main}"
        );
        assert!(
            main.contains("from \"./kipc-transport.ts\""),
            "echo adapter must import the embedded canonical transport: {main}"
        );
        assert!(
            !main.contains("from \"./kipc\""),
            "the configured entry must not import the compatibility facade: {main}"
        );
        assert_created_hello_keeps_kipc_transport(&root, &main);
        let echo_generated =
            fs::read_to_string(root.join("src").join("echo.generated.ts")).expect("echo types");
        assert_eq!(
            echo_generated,
            include_str!("../templates/hello/src/echo.generated.ts"),
            "keld create must emit the exact freshness-checked generated artifact"
        );
        assert!(
            main.contains("import type { EchoRequest, EchoResponse }")
                && main.contains("export type { EchoRequest, EchoResponse }"),
            "generated main must consume and re-export only generated payload types: {main}"
        );
        assert!(
            !main.contains("interface EchoRequest") && !main.contains("interface EchoResponse"),
            "generated main must not retain hand-written payload declarations: {main}"
        );
        let transport =
            fs::read_to_string(root.join("src").join("kipc-transport.ts")).expect("kipc-transport");
        assert!(
            transport.contains("Bun.connect"),
            "canonical transport owns the socket connect: {transport}"
        );
        assert!(
            transport.contains("export class FrameReader"),
            "generated transport must be the implementation, not the in-repo shim: {transport}"
        );
        assert!(
            !main.contains("ipc-client"),
            "KEL-30: template must speak kipc directly, not shell out: {main}"
        );
        assert!(
            main.contains("demo: main process ready (IPC echo ok)"),
            "{main}"
        );
        assert!(
            main.contains("await windowsClosed")
                && main.contains("await session.quit()")
                && main.contains("quitAndCloseLink(this.#link"),
            "KEL-185 / GH-528 T3: the stock app consumes LastWindowClosed, then its Quit \
             awaits the correlated Reply and closes the link: {main}"
        );
        assert!(
            !main.contains("await new Promise(() => {})"),
            "KEL-185: stock app must not park forever after echo: {main}"
        );
        assert!(!main.contains("{{name}}"), "{main}");

        let kipc = fs::read_to_string(root.join("src/kipc.ts")).expect("kipc facade");
        assert_eq!(kipc, "export * from \"./main\";\n");
        assert!(
            !kipc.contains("class AppLinkSession") && !kipc.contains("Bun.connect"),
            "the compatibility facade must not duplicate the wire client: {kipc}"
        );

        let gitignore = fs::read_to_string(root.join(".gitignore")).expect("gitignore");
        assert!(gitignore.contains("node_modules/"), "{gitignore}");
        assert!(gitignore.contains(".keld/"), "{gitignore}");
    }

    #[test]
    fn created_hello_tree_keeps_kipc_transport_when_main_imports_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = create_project(dir.path(), "sidecar").expect("create");
        let main = fs::read_to_string(root.join("src").join("main.ts")).expect("main");
        assert_created_hello_keeps_kipc_transport(&root, &main);
    }
}
