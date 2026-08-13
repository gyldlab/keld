//! `keld doctor` — environment checks (KEL-29 / KEL-42 `--json`).

use std::path::{Component, Path};
use std::process::Command;

use keld_core::{DEFAULT_RENDERER, read_config_renderer};
use schemars::JsonSchema;
use serde::Serialize;

use crate::error_object::KeldErrorObject;

/// Exact fix text when Bun is missing (spec AC4 — exact-match tested).
pub const BUN_MISSING_FIX: &str = "install Bun from https://bun.sh and ensure `bun` is on PATH";

/// Code on the bun finding when `bun` is not on PATH.
pub const BUN_MISSING_CODE: &str = "KELD-CLI-033";

/// Code on the project finding when layout files are missing.
pub const PROJECT_LAYOUT_CODE: &str = "KELD-CLI-034";

/// Code when the project renderer HTML is missing or not a project-relative path.
pub const RENDERER_LOAD_CODE: &str = "KELD-CLI-035";

/// Registry `fix` for [`RENDERER_LOAD_CODE`] (also `keld dev` `DevError::Renderer`).
pub const RENDERER_LOAD_FIX: &str = "Set `renderer` in keld.config.ts to a project-relative HTML file \
     (no `..` or absolute paths) and create it.";

/// One doctor check result (human-oriented CLI form).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// Short label shown in output.
    pub label: &'static str,
    /// Whether the check passed.
    pub ok: bool,
    /// Detail line (fix hint on failure).
    pub detail: String,
    /// Arch/07 §2 error when `ok` is false.
    pub error: Option<KeldErrorObject>,
}

/// Serializable finding shared by `keld doctor --json` and MCP `keld_doctor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DoctorFinding {
    /// Short label (e.g. `bun`, `project`, `renderer`, `webview`).
    pub label: String,
    /// Whether the check passed.
    pub ok: bool,
    /// Detail line (human-readable).
    pub detail: String,
    /// Present iff `!ok` — agents read `fix` rather than parsing `detail`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<KeldErrorObject>,
}

impl From<&Check> for DoctorFinding {
    fn from(check: &Check) -> Self {
        Self {
            label: check.label.to_owned(),
            ok: check.ok,
            detail: check.detail.clone(),
            error: check.error.clone(),
        }
    }
}

/// Full `KELD-CLI-035` message used by doctor findings and `keld dev`.
#[must_use]
pub fn renderer_load_message(path: &Path, reason: &str) -> String {
    format!(
        "{RENDERER_LOAD_CODE}: cannot load renderer `{}` — {reason}. {RENDERER_LOAD_FIX}",
        path.display()
    )
}

/// Why `renderer` cannot be used as a project-relative HTML path, if it cannot.
///
/// Shared with `keld dev` so doctor and window load reject the same tokens.
#[must_use]
pub fn renderer_path_problem(renderer: &str) -> Option<&'static str> {
    let trimmed = renderer.trim();
    if trimmed.is_empty() {
        return Some("path is empty");
    }
    let path = Path::new(trimmed);
    let unsafe_path = path.is_absolute()
        || path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
    if unsafe_path {
        Some("path must be relative to the project root")
    } else {
        None
    }
}

/// Runs environment checks for local development.
#[must_use]
pub fn run_checks(project_root: Option<&Path>) -> Vec<Check> {
    let mut checks = vec![check_bun(), check_project_layout(project_root)];
    if let Some(root) = project_root {
        checks.push(check_renderer(root));
    }
    #[cfg(target_os = "macos")]
    {
        checks.push(check_macos_hello());
    }
    checks
}

/// Same checks as [`run_checks`], as JSON-serializable findings.
#[must_use]
pub fn run_findings(project_root: Option<&Path>) -> Vec<DoctorFinding> {
    run_checks(project_root)
        .iter()
        .map(DoctorFinding::from)
        .collect()
}

/// Serializes findings as a top-level JSON array (no wrapper object).
///
/// # Errors
///
/// Returns [`serde_json::Error`] when serialization fails (should not for these types).
pub fn findings_json(project_root: Option<&Path>) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&run_findings(project_root))
}

/// Returns true when every check passed.
#[must_use]
pub fn all_ok(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.ok)
}

/// Returns true when every finding passed.
#[must_use]
pub fn findings_all_ok(findings: &[DoctorFinding]) -> bool {
    findings.iter().all(|f| f.ok)
}

fn check_bun() -> Check {
    match Command::new("bun").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            Check {
                label: "bun",
                ok: true,
                detail: format!("found bun {version}"),
                error: None,
            }
        }
        _ => Check {
            label: "bun",
            ok: false,
            detail: BUN_MISSING_FIX.to_owned(),
            error: Some(KeldErrorObject::new(
                BUN_MISSING_CODE,
                "Bun runtime not found on PATH",
                BUN_MISSING_FIX,
            )),
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
            error: None,
        };
    };
    let has_config = root.join("keld.config.ts").is_file();
    let has_main = root.join("src/main.ts").is_file();
    if has_config && has_main {
        Check {
            label: "project",
            ok: true,
            detail: format!("keld project at {}", root.display()),
            error: None,
        }
    } else {
        let fix = "missing keld.config.ts or src/main.ts — run `keld create <name>` first";
        Check {
            label: "project",
            ok: false,
            detail: fix.to_owned(),
            error: Some(
                KeldErrorObject::new(
                    PROJECT_LAYOUT_CODE,
                    format!("project layout incomplete at {}", root.display()),
                    fix,
                )
                .with_cause(root.display().to_string()),
            ),
        }
    }
}

fn check_renderer(root: &Path) -> Check {
    let renderer = read_config_renderer(root).unwrap_or_else(|| DEFAULT_RENDERER.to_owned());
    let rel = renderer.trim();
    let reason = renderer_path_problem(&renderer).or_else(|| {
        if root.join(rel).is_file() {
            None
        } else {
            Some("file is missing")
        }
    });
    if let Some(reason) = reason {
        let path = Path::new(rel);
        Check {
            label: "renderer",
            ok: false,
            detail: renderer_load_message(path, reason),
            error: Some(
                KeldErrorObject::new(
                    RENDERER_LOAD_CODE,
                    format!("cannot load renderer `{rel}` — {reason}"),
                    RENDERER_LOAD_FIX,
                )
                .with_cause(rel.to_owned()),
            ),
        }
    } else {
        Check {
            label: "renderer",
            ok: true,
            detail: format!("renderer `{rel}` readable"),
            error: None,
        }
    }
}

#[cfg(target_os = "macos")]
fn check_macos_hello() -> Check {
    Check {
        label: "webview",
        ok: true,
        detail: "macOS WKWebView hello window available via `keld dev`".to_owned(),
        error: None,
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

    fn renderer_check(root: &Path) -> Check {
        run_checks(Some(root))
            .into_iter()
            .find(|c| c.label == "renderer")
            .expect("renderer check runs when a project root is passed")
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
        assert!(check.error.is_none());
        assert!(
            run_checks(None).iter().all(|c| c.label != "renderer"),
            "no project root must not emit a renderer finding"
        );
    }

    #[test]
    fn layout_ok_when_config_and_main_exist() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("keld.config.ts"), "export default {}\n").expect("config");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(dir.path().join("src/main.ts"), "export {}\n").expect("main");
        fs::write(dir.path().join("index.html"), "<!DOCTYPE html><p>ok</p>\n").expect("html");
        let check = project_check(Some(dir.path()));
        assert!(check.ok, "{check:?}");
        assert!(
            check.detail.contains(&dir.path().display().to_string()),
            "{}",
            check.detail
        );
        let renderer = renderer_check(dir.path());
        assert!(renderer.ok, "{renderer:?}");
        assert!(
            renderer.detail.contains("index.html"),
            "{}",
            renderer.detail
        );
        let checks = run_checks(Some(dir.path()));
        let bun = checks.iter().find(|c| c.label == "bun").expect("bun check");
        if bun.ok {
            assert!(
                all_ok(&checks),
                "project+renderer+bun ok must make all_ok true"
            );
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
        let err = project.error.as_ref().expect("§2 error");
        assert_eq!(err.code, PROJECT_LAYOUT_CODE);
        assert!(err.fix.contains("keld create"), "{}", err.fix);
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
        let err = project.error.as_ref().expect("§2 error");
        assert_eq!(err.code, PROJECT_LAYOUT_CODE);
    }

    #[test]
    fn renderer_fails_when_index_html_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("keld.config.ts"), "export default {}\n").expect("config");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(dir.path().join("src/main.ts"), "export {}\n").expect("main");
        let project = project_check(Some(dir.path()));
        assert!(project.ok, "config+main is still a project: {project:?}");
        let renderer = renderer_check(dir.path());
        assert!(!renderer.ok, "missing index.html must fail: {renderer:?}");
        let msg = renderer.detail.clone();
        assert!(msg.contains(RENDERER_LOAD_CODE), "{msg}");
        assert!(msg.contains("index.html"), "{msg}");
        assert!(msg.contains("missing"), "{msg}");
        assert!(msg.contains("keld.config.ts"), "{msg}");
        let err = renderer.error.as_ref().expect("§2 error");
        assert_eq!(err.code, RENDERER_LOAD_CODE);
        assert_eq!(err.fix, RENDERER_LOAD_FIX);
        let checks = run_checks(Some(dir.path()));
        assert!(
            !all_ok(&checks),
            "missing renderer must make all_ok false even when project is ok"
        );
    }

    #[test]
    fn renderer_follows_config_path_and_rejects_traversal() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("keld.config.ts"),
            "export default {\n  renderer: \"ui/app.html\",\n} as const;\n",
        )
        .expect("config");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(dir.path().join("src/main.ts"), "export {}\n").expect("main");
        let missing = renderer_check(dir.path());
        assert!(!missing.ok, "{missing:?}");
        assert!(missing.detail.contains("ui/app.html"), "{}", missing.detail);
        assert!(
            missing.detail.contains(RENDERER_LOAD_CODE),
            "{}",
            missing.detail
        );

        fs::create_dir_all(dir.path().join("ui")).expect("ui");
        fs::write(dir.path().join("ui/app.html"), "<!DOCTYPE html><p>ok</p>\n").expect("html");
        let ok = renderer_check(dir.path());
        assert!(ok.ok, "{ok:?}");
        assert!(ok.detail.contains("ui/app.html"), "{}", ok.detail);

        fs::write(
            dir.path().join("keld.config.ts"),
            "export default {\n  renderer: \"../outside.html\",\n} as const;\n",
        )
        .expect("rewrite config");
        let traversal = renderer_check(dir.path());
        assert!(!traversal.ok, "{traversal:?}");
        assert!(
            traversal.detail.contains("relative"),
            "{}",
            traversal.detail
        );
        assert!(
            traversal.detail.contains("../outside.html"),
            "{}",
            traversal.detail
        );
        assert_eq!(
            traversal.error.as_ref().map(|e| e.code.as_str()),
            Some(RENDERER_LOAD_CODE)
        );
    }

    #[test]
    fn bun_missing_fix_is_exact() {
        assert_eq!(
            BUN_MISSING_FIX,
            "install Bun from https://bun.sh and ensure `bun` is on PATH"
        );
        let err = KeldErrorObject::new(
            BUN_MISSING_CODE,
            "Bun runtime not found on PATH",
            BUN_MISSING_FIX,
        );
        assert_eq!(err.fix, BUN_MISSING_FIX);
        assert_eq!(err.code, "KELD-CLI-033");
    }

    #[test]
    fn findings_json_is_array_with_required_fields() {
        let json = findings_json(None).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert!(value.is_array(), "doctor JSON must be a top-level array");
        let arr = value.as_array().expect("array");
        assert!(!arr.is_empty());
        for item in arr {
            assert!(
                item.get("label")
                    .and_then(serde_json::Value::as_str)
                    .is_some()
            );
            assert!(
                item.get("ok")
                    .and_then(serde_json::Value::as_bool)
                    .is_some()
            );
            assert!(
                item.get("detail")
                    .and_then(serde_json::Value::as_str)
                    .is_some()
            );
            if item["ok"] == serde_json::Value::Bool(false) {
                assert!(
                    item.get("error").and_then(|e| e.get("code")).is_some(),
                    "failed finding must embed §2 error.code: {item}"
                );
                assert!(
                    item.pointer("/error/fix")
                        .and_then(serde_json::Value::as_str)
                        .is_some(),
                    "failed finding must embed §2 error.fix: {item}"
                );
            }
        }
    }

    #[test]
    fn failed_finding_always_has_error_ok_finding_does_not() {
        let dir = tempfile::tempdir().expect("tempdir");
        let findings = run_findings(Some(dir.path()));
        for f in &findings {
            if f.ok {
                assert!(f.error.is_none(), "ok finding must not carry error: {f:?}");
            } else {
                let err = f.error.as_ref().expect("failed finding needs §2 error");
                assert!(err.code.starts_with("KELD-"), "{}", err.code);
                assert!(!err.fix.is_empty());
            }
        }
        let project = findings
            .iter()
            .find(|f| f.label == "project")
            .expect("project finding");
        assert!(!project.ok);
        assert_eq!(
            project.error.as_ref().map(|e| e.code.as_str()),
            Some(PROJECT_LAYOUT_CODE)
        );
    }
}
