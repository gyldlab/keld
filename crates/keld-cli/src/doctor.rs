//! `keld doctor` — environment checks (proto version for KEL-29).

use std::path::Path;
use std::process::Command;

/// One doctor check result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// Short label shown in output.
    pub label: &'static str,
    /// Whether the check passed.
    pub ok: bool,
    /// Detail line (fix hint on failure).
    pub detail: String,
}

/// Runs environment checks for local development.
#[must_use]
pub fn run_checks(project_root: Option<&Path>) -> Vec<Check> {
    let mut checks = vec![check_bun(), check_project_layout(project_root)];
    #[cfg(target_os = "macos")]
    checks.push(check_macos_hello());
    checks
}

/// Returns true when every check passed.
#[must_use]
pub fn all_ok(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.ok)
}

fn check_bun() -> Check {
    match Command::new("bun").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            Check {
                label: "bun",
                ok: true,
                detail: format!("found bun {version}"),
            }
        }
        _ => Check {
            label: "bun",
            ok: false,
            detail: "install Bun from https://bun.sh and ensure `bun` is on PATH".to_owned(),
        },
    }
}

fn check_project_layout(project_root: Option<&Path>) -> Check {
    let Some(root) = project_root else {
        return Check {
            label: "project",
            ok: true,
            detail: "no project directory (run inside a scaffolded app for layout checks)"
                .to_owned(),
        };
    };
    let has_config = root.join("keld.config.ts").is_file();
    let has_main = root.join("src/main.ts").is_file();
    if has_config && has_main {
        Check {
            label: "project",
            ok: true,
            detail: format!("keld project at {}", root.display()),
        }
    } else {
        Check {
            label: "project",
            ok: false,
            detail: "missing keld.config.ts or src/main.ts — run `keld create <name>` first"
                .to_owned(),
        }
    }
}

#[cfg(target_os = "macos")]
fn check_macos_hello() -> Check {
    Check {
        label: "webview",
        ok: true,
        detail: "macOS WKWebView hello window available via `keld dev`".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project_check(root: Option<&Path>) -> Check {
        run_checks(root)
            .into_iter()
            .find(|c| c.label == "project")
            .expect("project check is always present")
    }

    #[test]
    fn outside_project_layout_is_ok() {
        let check = project_check(None);
        assert!(check.ok, "{check:?}");
        assert!(
            check.detail.contains("no project directory"),
            "{}",
            check.detail
        );
    }

    #[test]
    fn layout_ok_when_config_and_main_exist() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("keld.config.ts"), "export default {}\n").expect("config");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(dir.path().join("src/main.ts"), "export {}\n").expect("main");
        let check = project_check(Some(dir.path()));
        assert!(check.ok, "{check:?}");
        assert!(
            check.detail.contains(&dir.path().display().to_string()),
            "{}",
            check.detail
        );
        let checks = run_checks(Some(dir.path()));
        let bun = checks.iter().find(|c| c.label == "bun").expect("bun check");
        if bun.ok {
            assert!(all_ok(&checks), "project+bun ok must make all_ok true");
        } else {
            assert!(!all_ok(&checks), "missing bun must make all_ok false");
        }
    }

    #[test]
    fn layout_fails_when_config_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(dir.path().join("src/main.ts"), "export {}\n").expect("main");
        let checks = run_checks(Some(dir.path()));
        let project = checks
            .iter()
            .find(|c| c.label == "project")
            .expect("project check");
        assert!(!project.ok, "missing keld.config.ts must fail: {project:?}");
        assert!(
            project.detail.contains("keld.config.ts"),
            "{}",
            project.detail
        );
        assert!(!all_ok(&checks), "all_ok must be false when project fails");
    }

    #[test]
    fn layout_fails_when_main_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("keld.config.ts"), "export default {}\n").expect("config");
        let checks = run_checks(Some(dir.path()));
        let project = checks
            .iter()
            .find(|c| c.label == "project")
            .expect("project check");
        assert!(!project.ok, "missing src/main.ts must fail: {project:?}");
        assert!(project.detail.contains("src/main.ts"), "{}", project.detail);
        assert!(!all_ok(&checks), "all_ok must be false when project fails");
    }
}
