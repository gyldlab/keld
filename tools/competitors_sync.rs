//! Shallow-clone / update competitor trees from `competitors.lock.toml`.
//!
//! Not a Cargo member. Compile with:
//! `rustc --edition=2024 -D warnings tools/competitors_sync.rs`
//!
//! Usage:
//!   competitors-sync [--dry-run] [--force] [REPO_ROOT]
//!
//! Paths are gitignored; this tool never stages into the Keld repo.
//! Error text states the fix. Codes are not `KELD-*` (same as `ci_hygiene.rs`)
//! so this standalone tool does not collide with the product error registry.

use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Competitor,
    MigrationOracle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Repo {
    name: String,
    kind: Kind,
    url: String,
    branch: String,
    sha: Option<String>,
    path_override: Option<String>,
}

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let force = args.iter().any(|a| a == "--force");
    args.retain(|a| a != "--dry-run" && a != "--force");
    let root = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    match run(&root, dry_run, force) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("competitors-sync: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(root: &Path, dry_run: bool, force: bool) -> Result<(), String> {
    let lock_path = root.join("competitors.lock.toml");
    let text = fs::read_to_string(&lock_path).map_err(|error| {
        format!(
            "missing `{}`: {error}. Restore competitors.lock.toml from git.",
            lock_path.display()
        )
    })?;
    let repos = parse_lockfile(&text)?;
    if repos.is_empty() {
        return Err(
            "competitors.lock.toml has no [[repo]] entries. Add at least one [[repo]] table."
                .into(),
        );
    }

    for repo in &repos {
        let dest = resolve_path(root, repo)?;
        if dry_run {
            println!(
                "would sync {} ({:?}) → {} [branch={}{}]",
                repo.name,
                repo.kind,
                dest.display(),
                repo.branch,
                repo.sha
                    .as_ref()
                    .map(|s| format!(" sha={s}"))
                    .unwrap_or_default()
            );
            continue;
        }
        sync_one(repo, &dest, force)?;
    }
    if dry_run {
        println!("competitors-sync: dry-run ok ({} repos)", repos.len());
    } else {
        println!("competitors-sync: ok ({} repos)", repos.len());
    }
    Ok(())
}

/// Resolve clone destination and refuse anything outside `competitors/`.
fn resolve_path(root: &Path, repo: &Repo) -> Result<PathBuf, String> {
    validate_repo_name(&repo.name)?;
    let competitors = root.join("competitors");
    let dest = if let Some(rel) = &repo.path_override {
        validate_path_override(rel)?;
        root.join(rel)
    } else {
        match repo.kind {
            Kind::Competitor => competitors.join(&repo.name),
            Kind::MigrationOracle => competitors.join("migration").join(&repo.name),
        }
    };
    ensure_under_competitors(root, &dest)?;
    Ok(dest)
}

fn validate_repo_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(
            "repo `name` is empty. Set name to a single path segment (no `/`, `\\`, `.`, or `..`)."
                .into(),
        );
    }
    if name == "." || name == ".." {
        return Err(format!(
            "repo `name` `{name}` is not allowed. Use a single path segment under competitors/."
        ));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(format!(
            "repo `name` `{name}` must be a single path segment (no separators). Put nested paths in `path` only if they stay under competitors/."
        ));
    }
    Ok(())
}

fn validate_path_override(rel: &str) -> Result<(), String> {
    let path = Path::new(rel);
    if path.is_absolute() {
        return Err(format!(
            "path `{rel}` is absolute. Use a repo-relative path under competitors/."
        ));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir | Component::ParentDir => {
                return Err(format!(
                    "path `{rel}` contains `.` or `..`. Use a normalized relative path under competitors/."
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "path `{rel}` is not a relative path under competitors/."
                ));
            }
        }
    }
    Ok(())
}

fn ensure_under_competitors(root: &Path, dest: &Path) -> Result<(), String> {
    let competitors = normalize_lexically(&root.join("competitors"));
    let dest_norm = normalize_lexically(dest);
    if !dest_norm.starts_with(&competitors) {
        return Err(format!(
            "destination `{}` escapes competitors/. Keep `name` / `path` under competitors/.",
            dest.display()
        ));
    }
    if dest_norm == competitors {
        return Err(format!(
            "destination `{}` is the competitors/ root, not a repo checkout. Set `name` or a nested `path`.",
            dest.display()
        ));
    }
    Ok(())
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn sync_one(repo: &Repo, dest: &Path, force: bool) -> Result<(), String> {
    if dest.join(".git").is_dir() {
        update_existing(repo, dest, force)
    } else if dest.exists() {
        Err(format!(
            "`{}` exists but is not a git checkout; remove or convert it, then re-run.",
            dest.display()
        ))
    } else {
        clone_new(repo, dest)
    }
}

fn clone_new(repo: &Repo, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "cannot create `{}`: {error}. Fix permissions or free disk space, then re-run.",
                parent.display()
            )
        })?;
    }
    let mut cmd = Command::new("git");
    cmd.args(["clone", "--depth", "1", "--branch", &repo.branch, &repo.url]);
    cmd.arg(dest);
    run_git(&mut cmd, &format!("clone {}", repo.name))?;
    if let Some(sha) = &repo.sha {
        // Fresh clone is clean; force is irrelevant.
        fetch_reset_sha(dest, sha, true)?;
    }
    println!(
        "competitors-sync: cloned {} → {}",
        repo.name,
        dest.display()
    );
    Ok(())
}

fn update_existing(repo: &Repo, dest: &Path, force: bool) -> Result<(), String> {
    refuse_dirty_unless_force(dest, force)?;
    if let Some(sha) = &repo.sha {
        fetch_reset_sha(dest, sha, force)?;
    } else {
        let mut fetch = Command::new("git");
        fetch.current_dir(dest);
        fetch.args(["fetch", "--depth", "1", "origin", &repo.branch]);
        run_git(&mut fetch, &format!("fetch {}", repo.name))?;

        let mut reset = Command::new("git");
        reset.current_dir(dest);
        reset.args(["reset", "--hard", &format!("origin/{}", repo.branch)]);
        // After shallow fetch of a branch, FETCH_HEAD / origin/<branch> may need
        // the remote-tracking ref; fall back to FETCH_HEAD.
        if run_git(&mut reset, &format!("reset {}", repo.name)).is_err() {
            let mut reset_fh = Command::new("git");
            reset_fh.current_dir(dest);
            reset_fh.args(["reset", "--hard", "FETCH_HEAD"]);
            run_git(&mut reset_fh, &format!("reset FETCH_HEAD {}", repo.name))?;
        }
    }
    println!(
        "competitors-sync: updated {} @ {}",
        repo.name,
        dest.display()
    );
    Ok(())
}

fn refuse_dirty_unless_force(dest: &Path, force: bool) -> Result<(), String> {
    if force || !is_dirty(dest)? {
        return Ok(());
    }
    Err(format!(
        "`{}` has local modifications; re-run with --force to discard them, or commit/stash inside that checkout first.",
        dest.display()
    ))
}

fn is_dirty(dest: &Path) -> Result<bool, String> {
    let mut status = Command::new("git");
    status.current_dir(dest);
    status.args(["status", "--porcelain"]);
    let output = status.output().map_err(|error| {
        format!(
            "status {}: failed to spawn git: {error}. Install git and retry.",
            dest.display()
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "status {}: git failed ({}). {stderr}",
            dest.display(),
            output.status
        ));
    }
    Ok(!output.stdout.is_empty())
}

fn fetch_reset_sha(dest: &Path, sha: &str, force: bool) -> Result<(), String> {
    refuse_dirty_unless_force(dest, force)?;
    let mut fetch = Command::new("git");
    fetch.current_dir(dest);
    fetch.args(["fetch", "--depth", "1", "origin", sha]);
    run_git(&mut fetch, &format!("fetch sha {sha}"))?;
    let mut reset = Command::new("git");
    reset.current_dir(dest);
    reset.args(["reset", "--hard", "FETCH_HEAD"]);
    run_git(&mut reset, &format!("reset sha {sha}"))
}

fn run_git(cmd: &mut Command, label: &str) -> Result<(), String> {
    let output = cmd.output().map_err(|error| {
        format!("{label}: failed to spawn git: {error}. Install git and retry.")
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(format!(
        "{label}: git failed ({}).\n{stderr}{stdout}",
        output.status
    ))
}

/// Minimal TOML subset parser for `[[repo]]` tables with string keys we care about.
fn parse_lockfile(text: &str) -> Result<Vec<Repo>, String> {
    let mut repos = Vec::new();
    let mut current: Option<PartialRepo> = None;

    for (line_no, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line == "[[repo]]" {
            if let Some(partial) = current.take() {
                repos.push(partial.finish(line_no)?);
            }
            current = Some(PartialRepo::default());
            continue;
        }
        let Some(partial) = current.as_mut() else {
            return Err(format!(
                "line {}: unexpected content outside [[repo]] — only [[repo]] tables are supported.",
                line_no + 1
            ));
        };
        let (key, value) = split_kv(line, line_no + 1)?;
        match key {
            "name" => partial.name = Some(unquote(value, line_no + 1)?),
            "kind" => {
                partial.kind = Some(match unquote(value, line_no + 1)?.as_str() {
                    "competitor" => Kind::Competitor,
                    "migration-oracle" => Kind::MigrationOracle,
                    other => {
                        return Err(format!(
                            "line {}: unknown kind `{other}` (use competitor or migration-oracle).",
                            line_no + 1
                        ));
                    }
                });
            }
            "url" => partial.url = Some(unquote(value, line_no + 1)?),
            "branch" => partial.branch = Some(unquote(value, line_no + 1)?),
            "sha" => partial.sha = Some(unquote(value, line_no + 1)?),
            "path" => partial.path_override = Some(unquote(value, line_no + 1)?),
            other => {
                return Err(format!(
                    "line {}: unknown key `{other}` in [[repo]].",
                    line_no + 1
                ));
            }
        }
    }
    if let Some(partial) = current.take() {
        repos.push(partial.finish(text.lines().count())?);
    }
    Ok(repos)
}

#[derive(Default)]
struct PartialRepo {
    name: Option<String>,
    kind: Option<Kind>,
    url: Option<String>,
    branch: Option<String>,
    sha: Option<String>,
    path_override: Option<String>,
}

impl PartialRepo {
    fn finish(self, context_line: usize) -> Result<Repo, String> {
        Ok(Repo {
            name: self.name.ok_or_else(|| {
                format!("[[repo]] near line {context_line}: missing required `name`.")
            })?,
            kind: self.kind.ok_or_else(|| {
                format!("[[repo]] near line {context_line}: missing required `kind`.")
            })?,
            url: self.url.ok_or_else(|| {
                format!("[[repo]] near line {context_line}: missing required `url`.")
            })?,
            branch: self.branch.ok_or_else(|| {
                format!("[[repo]] near line {context_line}: missing required `branch`.")
            })?,
            sha: self.sha,
            path_override: self.path_override,
        })
    }
}

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..idx],
            _ => {}
        }
    }
    line
}

fn split_kv(line: &str, line_no: usize) -> Result<(&str, &str), String> {
    let Some((key, value)) = line.split_once('=') else {
        return Err(format!("line {line_no}: expected `key = \"value\"`."));
    };
    Ok((key.trim(), value.trim()))
}

fn unquote(value: &str, line_no: usize) -> Result<String, String> {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return Ok(value[1..value.len() - 1].to_string());
    }
    Err(format!(
        "line {line_no}: expected a double-quoted string, got `{value}`."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_repos() -> Vec<Repo> {
        let text = r#"
[[repo]]
name = "electron"
kind = "competitor"
url = "https://github.com/electron/electron.git"
branch = "main"

[[repo]]
name = "vscode"
kind = "migration-oracle"
url = "https://github.com/microsoft/vscode.git"
branch = "main"
sha = "deadbeef"
"#;
        parse_lockfile(text).expect("parse")
    }

    #[test]
    fn parses_competitor_and_migration_oracle() {
        let repos = sample_repos();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].name, "electron");
        assert_eq!(repos[0].kind, Kind::Competitor);
        assert_eq!(repos[1].kind, Kind::MigrationOracle);
        assert_eq!(repos[1].sha.as_deref(), Some("deadbeef"));

        let root = Path::new("/tmp/keld");
        assert_eq!(
            resolve_path(root, &repos[0]).expect("electron path"),
            PathBuf::from("/tmp/keld/competitors/electron")
        );
        assert_eq!(
            resolve_path(root, &repos[1]).expect("vscode path"),
            PathBuf::from("/tmp/keld/competitors/migration/vscode")
        );
    }

    #[test]
    fn rejects_unknown_kind() {
        let text = r#"
[[repo]]
name = "x"
kind = "other"
url = "https://example.com/x.git"
branch = "main"
"#;
        let err = parse_lockfile(text).expect_err("kind");
        assert!(err.contains("unknown kind"), "{err}");
    }

    #[test]
    fn rejects_upstream_reference_kind() {
        let text = r#"
[[repo]]
name = "bun"
kind = "upstream-reference"
url = "https://github.com/oven-sh/bun.git"
branch = "main"
"#;
        let err = parse_lockfile(text).expect_err("kind");
        assert!(err.contains("unknown kind"), "{err}");
        assert!(err.contains("upstream-reference"), "{err}");
    }

    #[test]
    fn bun_wry_tao_use_competitor_kind_under_competitors() {
        let text = r#"
[[repo]]
name = "bun"
kind = "competitor"
url = "https://github.com/oven-sh/bun.git"
branch = "main"

[[repo]]
name = "wry"
kind = "competitor"
url = "https://github.com/tauri-apps/wry.git"
branch = "dev"

[[repo]]
name = "tao"
kind = "competitor"
url = "https://github.com/tauri-apps/tao.git"
branch = "dev"
"#;
        let repos = parse_lockfile(text).expect("parse upstream checkouts");
        assert_eq!(repos.len(), 3);
        let root = Path::new("/tmp/keld");
        for repo in &repos {
            assert_eq!(repo.kind, Kind::Competitor);
            let dest = resolve_path(root, repo).expect("path");
            assert_eq!(
                dest,
                PathBuf::from(format!("/tmp/keld/competitors/{}", repo.name))
            );
        }
        assert_eq!(repos[0].name, "bun");
        assert_eq!(repos[0].branch, "main");
        assert_eq!(repos[1].name, "wry");
        assert_eq!(repos[1].branch, "dev");
        assert_eq!(repos[2].name, "tao");
        assert_eq!(repos[2].branch, "dev");
    }

    #[test]
    fn committed_lockfile_lists_bun_wry_tao() {
        let text = fs::read_to_string("competitors.lock.toml")
            .expect("run parser tests from the repo root so competitors.lock.toml is readable");
        let repos = parse_lockfile(&text).expect("committed lock parses");
        let names: Vec<&str> = repos.iter().map(|repo| repo.name.as_str()).collect();
        for required in ["bun", "wry", "tao"] {
            assert!(
                names.contains(&required),
                "competitors.lock.toml must list `{required}` so just competitors-sync fetches it"
            );
        }
        for repo in &repos {
            if matches!(repo.name.as_str(), "bun" | "wry" | "tao") {
                assert_eq!(repo.kind, Kind::Competitor);
                let dest = resolve_path(Path::new("/tmp/keld"), repo).expect("path");
                assert_eq!(
                    dest,
                    PathBuf::from(format!("/tmp/keld/competitors/{}", repo.name))
                );
            }
        }
        let vscode: Vec<&Repo> = repos.iter().filter(|repo| repo.name == "vscode").collect();
        assert_eq!(vscode.len(), 1);
        assert_eq!(vscode[0].kind, Kind::MigrationOracle);
    }

    #[test]
    fn rejects_parent_dir_name() {
        let mut repo = sample_repos().remove(0);
        repo.name = "..".into();
        let err = resolve_path(Path::new("/tmp/keld"), &repo).expect_err("name");
        assert!(
            err.contains("not allowed") || err.contains("escapes"),
            "{err}"
        );
    }

    #[test]
    fn rejects_path_override_traversal() {
        let mut repo = sample_repos().remove(0);
        repo.path_override = Some("competitors/../.git".into());
        let err = resolve_path(Path::new("/tmp/keld"), &repo).expect_err("path");
        assert!(
            err.contains(".") || err.contains("escapes") || err.contains("normalized"),
            "{err}"
        );
    }

    #[test]
    fn rejects_absolute_path_override() {
        let mut repo = sample_repos().remove(0);
        repo.path_override = Some("/tmp/evil".into());
        let err = resolve_path(Path::new("/tmp/keld"), &repo).expect_err("abs");
        assert!(err.contains("absolute"), "{err}");
    }

    #[test]
    fn accepts_nested_path_under_competitors() {
        let mut repo = sample_repos().remove(0);
        repo.path_override = Some("competitors/custom/electron".into());
        let dest = resolve_path(Path::new("/tmp/keld"), &repo).expect("nested");
        assert_eq!(dest, PathBuf::from("/tmp/keld/competitors/custom/electron"));
    }
}
