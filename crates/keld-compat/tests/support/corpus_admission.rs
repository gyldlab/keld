//! Execution admission for registered corpora (gh566 D8), split from the owner under
//! the gh566 D1 review condition (the owner passed 1,500 lines). It runs each distinct
//! registered target once with the live KEL-237 command lines and admits a cell only
//! when its exact case passed once. It holds no test function, and it needs the owner
//! module `corpus_manifest` beside it at the crate root.
// Each including target uses a different subset of this module (keld-ipc precedent).
#![allow(dead_code)]
// Typed rejections come from the owner's `CorpusError` (see its module note).
#![allow(clippy::result_large_err)]

use std::process::Command;

use keld_compat::evidence::{CellKey, Platform};

use crate::corpus_manifest::{
    Cell, Corpus, CorpusError, Registration, Runner, TestTarget, host_platform, workspace_root,
};

/// Require exactly one successful libtest pretty-format case, not a substring.
/// Ignored, missing, similarly named and duplicate records fail closed.
pub fn rust_case_passed(stdout: &str, name: &str) -> bool {
    let expected = format!("test {name} ... ok");
    stdout.lines().filter(|line| *line == expected).count() == 1
}

/// Require one successful Bun no-color console record for an exact leaf name.
/// Strip only the optional timing suffix and the runner's describe hierarchy.
/// A reporter-format change fails closed; source text is never a fallback.
pub fn bun_case_passed(stderr: &str, name: &str) -> bool {
    stderr
        .lines()
        .filter_map(|line| line.strip_prefix("(pass) "))
        .map(|line| {
            let full_name = match line.rsplit_once(" [") {
                Some((label, timing)) if timing.ends_with(']') => label,
                _ => line,
            };
            full_name.rsplit(" > ").next().unwrap_or(full_name)
        })
        .filter(|found| *found == name)
        .count()
        == 1
}

/// A receipt-named case is live when libtest `--list` prints exactly one `<name>: test`
/// line and `--list --ignored` prints none (C1).
pub fn rust_case_listed(list: &str, ignored: &str, name: &str) -> bool {
    let expected = format!("{name}: test");
    list.lines().filter(|line| *line == expected).count() == 1
        && !ignored.lines().any(|line| line == expected)
}

/// Runs `cargo test --offline -p keld-compat --test <target>` with `extra` libtest args.
fn cargo_libtest(target: &str, extra: &[&str]) -> Result<String, CorpusError> {
    let output = Command::new(env!("CARGO"))
        .args([
            "test",
            "--offline",
            "--color",
            "never",
            "-p",
            "keld-compat",
            "--test",
            target,
            "--",
        ])
        .args(extra)
        .env_remove("RUST_TEST_NOCAPTURE")
        .current_dir(workspace_root())
        .output()
        .map_err(|error| CorpusError::RunnerFailed {
            target: target.to_owned(),
            detail: format!("cannot run Cargo: {error}"),
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        return Err(CorpusError::RunnerFailed {
            target: target.to_owned(),
            detail: format!(
                "exit {}. stdout:\n{stdout}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }
    Ok(stdout)
}

/// Lists a keld-compat integration target: (`--list`, `--list --ignored`) output (C1).
pub fn list_libtest(target: &str) -> Result<(String, String), CorpusError> {
    Ok((
        cargo_libtest(target, &["--list"])?,
        cargo_libtest(target, &["--list", "--ignored"])?,
    ))
}

/// Output of one runner over one registered target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerOutput {
    /// Registered target path.
    pub path: String,
    /// Libtest stdout or Bun stderr, where each runner prints case results.
    pub text: String,
}

/// Which runner a check covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerKind {
    /// Cargo and libtest.
    Libtest,
    /// Bun.
    Bun,
}

fn runner_kind(runner: Runner) -> RunnerKind {
    match runner {
        Runner::Libtest { .. } => RunnerKind::Libtest,
        Runner::Bun => RunnerKind::Bun,
    }
}

fn distinct_targets(corpora: &[&Corpus], kind: RunnerKind) -> Vec<TestTarget> {
    let mut targets: Vec<TestTarget> = Vec::new();
    for corpus in corpora {
        for target in corpus.registration().targets {
            if runner_kind(target.runner) == kind && !targets.contains(target) {
                targets.push(*target);
            }
        }
    }
    targets
}

/// Runs each distinct registered target of `kind` once (live command lines, unchanged).
pub fn run_targets(
    corpora: &[&Corpus],
    kind: RunnerKind,
) -> Result<Vec<RunnerOutput>, CorpusError> {
    distinct_targets(corpora, kind)
        .into_iter()
        .map(|target| {
            let text = match target.runner {
                Runner::Libtest { target: name } => {
                    cargo_libtest(name, &["--format", "pretty", "--color", "never"])?
                }
                Runner::Bun => run_bun(target.path)?,
            };
            Ok(RunnerOutput {
                path: target.path.to_owned(),
                text,
            })
        })
        .collect()
}

fn run_bun(path: &str) -> Result<String, CorpusError> {
    let output = Command::new("bun")
        .args(["test", &format!("./{path}")])
        .env("NO_COLOR", "1")
        .env_remove("FORCE_COLOR")
        .current_dir(workspace_root())
        .output()
        .map_err(|error| CorpusError::RunnerFailed {
            target: path.to_owned(),
            detail: format!("cannot spawn bun ({error}); bun must be on PATH"),
        })?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(CorpusError::RunnerFailed {
            target: path.to_owned(),
            detail: format!(
                "exit {}. stdout:\n{}\nstderr:\n{stderr}",
                output.status,
                String::from_utf8_lossy(&output.stdout)
            ),
        });
    }
    Ok(stderr)
}

/// Cells whose mapped test is not run on this host: their lane is `unknown`, never
/// passed and never silently skipped (gh566 D13).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdmissionReport {
    /// (corpus id, cell) pairs that do not declare the host platform.
    pub unknown: Vec<(String, CellKey)>,
}

/// Pure admission check on `host`: every cell mapped to a `kind` target that declares
/// `host` has exactly one passing case in that target's output. A green runner with a
/// missing, skipped or `cfg`-gated case fails closed.
pub fn check_admission(
    corpora: &[&Corpus],
    host: Platform,
    kind: RunnerKind,
    outputs: &[RunnerOutput],
) -> Result<AdmissionReport, CorpusError> {
    let mut report = AdmissionReport::default();
    for corpus in corpora {
        for key in check_cells(
            corpus.id(),
            corpus.cells(),
            corpus.registration(),
            host,
            kind,
            outputs,
        )? {
            report.unknown.push((corpus.id().to_owned(), key));
        }
    }
    Ok(report)
}

/// Admission over one corpus's cells; returns the cells that do not declare `host`. A
/// cell whose path maps no registered target fails closed (`UnregisteredTarget`), even
/// though `Corpus::parse` already rejects one: admission never skips a cell silently.
pub fn check_cells(
    corpus_id: &str,
    cells: &[Cell],
    registration: &Registration,
    host: Platform,
    kind: RunnerKind,
    outputs: &[RunnerOutput],
) -> Result<Vec<CellKey>, CorpusError> {
    let mut unknown = Vec::new();
    for cell in cells {
        let target = registration
            .targets
            .iter()
            .find(|target| target.path == cell.test_path)
            .ok_or_else(|| CorpusError::UnregisteredTarget {
                corpus_id: corpus_id.to_owned(),
                cell: cell.key.operation_id.clone(),
                test_path: cell.test_path.clone(),
            })?;
        if runner_kind(target.runner) != kind {
            continue;
        }
        if !cell.platforms.contains(&host) {
            unknown.push(cell.key.clone());
            continue;
        }
        let passed = outputs
            .iter()
            .find(|output| output.path == target.path)
            .is_some_and(|output| match kind {
                RunnerKind::Libtest => rust_case_passed(&output.text, &cell.test_name),
                RunnerKind::Bun => bun_case_passed(&output.text, &cell.test_name),
            });
        if !passed {
            return Err(CorpusError::CaseNotAdmitted {
                corpus_id: corpus_id.to_owned(),
                cell: cell.key.operation_id.clone(),
                target: target.path.to_owned(),
                test_name: cell.test_name.clone(),
            });
        }
    }
    Ok(unknown)
}

/// Runs and admits every libtest-mapped cell of `corpora` on this host.
pub fn admit_libtest(corpora: &[&Corpus]) -> Result<AdmissionReport, CorpusError> {
    let outputs = run_targets(corpora, RunnerKind::Libtest)?;
    check_admission(corpora, host_platform(), RunnerKind::Libtest, &outputs)
}

/// Runs and admits every Bun-mapped cell of `corpora` on this host.
pub fn admit_bun(corpora: &[&Corpus]) -> Result<AdmissionReport, CorpusError> {
    let outputs = run_targets(corpora, RunnerKind::Bun)?;
    check_admission(corpora, host_platform(), RunnerKind::Bun, &outputs)
}
