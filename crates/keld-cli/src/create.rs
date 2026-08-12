//! `keld create` — scaffold the hello-world template (KEL-29).

use std::fs;
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
    if root.exists() {
        return Err(CreateError::Exists(root));
    }
    fs::create_dir_all(&root)?;
    for file in HELLO_TEMPLATE {
        let rendered = file.contents.replace("{{name}}", name);
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
    fn writes_expected_file_contents() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = create_project(dir.path(), "demo").expect("create");
        assert_eq!(file_count(&root), 5, "hello template is exactly five files");

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
        assert!(!html.contains("{{name}}"), "{html}");

        let main = fs::read_to_string(root.join("src/main.ts")).expect("main");
        assert!(main.contains("KELD-CLI-010"), "{main}");
        assert!(main.contains("KELD_APP_LINK"), "{main}");
        assert!(main.contains("ipc-client"), "{main}");
        assert!(
            main.contains("demo: main process ready (IPC echo ok)"),
            "{main}"
        );
        assert!(!main.contains("{{name}}"), "{main}");

        let gitignore = fs::read_to_string(root.join(".gitignore")).expect("gitignore");
        assert!(gitignore.contains("node_modules/"), "{gitignore}");
        assert!(gitignore.contains(".keld/"), "{gitignore}");
    }
}
