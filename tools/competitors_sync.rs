//! Shallow-clone / update competitor trees from `competitors.lock.toml`.
//!
//! Not a Cargo member. Compile with:
//! `rustc --edition=2024 -D warnings tools/competitors_sync.rs`
//!
//! Usage:
//!   competitors-sync [--dry-run] [--force] [`REPO_ROOT`]
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

fn git_command() -> Command {
    #[cfg(test)]
    {
        // Test fixtures must not inherit high-precedence process Git policy
        // that overrides their disposable repositories' local config.
        let mut cmd = Command::new("git");
        cmd.env_remove("GIT_CONFIG_COUNT");
        cmd.env_remove("GIT_CONFIG_PARAMETERS");
        cmd
    }
    #[cfg(not(test))]
    {
        // Preserve the caller's policy in the real standalone utility.
        Command::new("git")
    }
}

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let force = args.iter().any(|a| a == "--force");
    args.retain(|a| a != "--dry-run" && a != "--force");
    let root = args
        .first()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);

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

    // Validate all references and destinations before cloning or resetting
    // any checkout. Git's "--" ends option parsing, but refspecs can still
    // write local refs and an invalid later path must not cause partial sync.
    let mut destinations = Vec::with_capacity(repos.len());
    for repo in &repos {
        validate_reference_inputs(repo)?;
        let dest = resolve_path(root, repo)?;
        if destinations
            .iter()
            .any(|prior: &PathBuf| dest.starts_with(prior) || prior.starts_with(&dest))
        {
            return Err(format!(
                "repo `{}`: destination `{}` overlaps an earlier checkout. Give each repo a distinct path.",
                repo.name,
                dest.display()
            ));
        }
        destinations.push(dest);
    }

    for (repo, dest) in repos.iter().zip(&destinations) {
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
        sync_one(repo, dest, force)?;
    }
    if dry_run {
        println!("competitors-sync: dry-run ok ({} repos)", repos.len());
    } else {
        println!("competitors-sync: ok ({} repos)", repos.len());
    }
    Ok(())
}

/// Accept only branch names and full pinned object IDs, never fetch refspecs.
fn validate_reference_inputs(repo: &Repo) -> Result<(), String> {
    let branch = &repo.branch;
    if branch.is_empty() || branch.starts_with('-') || branch.contains("@{") {
        return Err(format!(
            "repo `{}`: invalid branch {branch:?}. Set a literal remote branch name.",
            repo.name
        ));
    }
    // Use Git's own branch-name contract, but reject option-like strings and
    // previous-checkout shorthand before Git can reinterpret them.
    let check = git_command()
        .args(["check-ref-format", "--branch", branch])
        .output()
        .map_err(|error| {
            format!(
                "repo `{}`: cannot validate branch: {error}. Install git and retry.",
                repo.name
            )
        })?;
    let reported_branch = String::from_utf8_lossy(&check.stdout);
    let reported_branch = reported_branch.trim_end_matches(['\r', '\n']);
    if !check.status.success() || reported_branch != branch {
        return Err(format!(
            "repo `{}`: invalid branch {branch:?}. Use a Git branch name, not a refspec or revision expression.",
            repo.name
        ));
    }

    if let Some(sha) = &repo.sha
        && (!matches!(sha.len(), 40 | 64) || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(format!(
            "repo `{}`: invalid sha {sha:?}. Pin a full 40- or 64-character hexadecimal Git object ID.",
            repo.name
        ));
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
    let mut cmd = clone_command(repo, dest);
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

fn clone_command(repo: &Repo, dest: &Path) -> Command {
    let mut cmd = git_command();
    cmd.args([
        "clone",
        "--depth",
        "1",
        "--branch",
        &repo.branch,
        "--",
        &repo.url,
    ]);
    cmd.arg(dest);
    cmd
}

fn update_existing(repo: &Repo, dest: &Path, force: bool) -> Result<(), String> {
    refuse_dirty_unless_force(dest, force)?;
    if let Some(sha) = &repo.sha {
        fetch_reset_sha(dest, sha, force)?;
    } else {
        // A branch can be named like a full object ID. Resolve the remote
        // branch explicitly instead of allowing Git to prefer the object ID.
        let branch_ref = format!("refs/heads/{}", repo.branch);
        fetch_origin_reference(dest, &branch_ref, &format!("fetch {}", repo.name))?;

        // Capture the immutable object ID now. A later fetch may replace the
        // shared FETCH_HEAD before a reset that resolves it again.
        let fetched = fetched_commit_id(dest, &format!("fetch {}", repo.name))?;
        // An earlier remote-tracking ref can still exist but be stale when
        // the clone's fetch map covers only its original single branch.
        reset_to_commit(dest, &fetched, &format!("reset fetched {}", repo.name))?;
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
    let mut status = git_command();
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
    fetch_origin_reference(dest, sha, &format!("fetch sha {sha}"))?;
    // A 40/64-hex string can also be a remote ref name. Verify the pin, then
    // reset the captured immutable ID rather than re-reading FETCH_HEAD.
    let fetched = fetched_commit_id(dest, &format!("fetch sha {sha}"))?;
    verify_pinned_identity(sha, &fetched)?;
    reset_to_commit(dest, &fetched, &format!("reset sha {sha}"))
}

fn verify_pinned_identity(expected: &str, fetched: &str) -> Result<(), String> {
    if !fetched.eq_ignore_ascii_case(expected) {
        return Err(format!(
            "fetched commit does not match pinned sha {expected}; refusing to reset checkout"
        ));
    }
    Ok(())
}

fn fetched_commit_id(dest: &Path, label: &str) -> Result<String, String> {
    let mut verify = git_command();
    verify.current_dir(dest);
    verify.args(["rev-parse", "--verify", "FETCH_HEAD^{commit}"]);
    let output = verify
        .output()
        .map_err(|error| format!("{label}: failed to verify fetched commit: {error}"))?;
    let actual = String::from_utf8_lossy(&output.stdout);
    let actual = actual.trim();
    if !output.status.success()
        || !matches!(actual.len(), 40 | 64)
        || !actual.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "{label}: Git did not return one valid fetched commit ID"
        ));
    }
    Ok(actual.to_string())
}

fn reset_to_commit(dest: &Path, oid: &str, label: &str) -> Result<(), String> {
    let mut reset = git_command();
    reset.current_dir(dest);
    reset.args(["reset", "--hard", oid]);
    run_git(&mut reset, label)
}

fn fetch_origin_reference(dest: &Path, reference: &str, label: &str) -> Result<(), String> {
    let mut fetch = git_command();
    fetch.current_dir(dest);
    fetch.args(["fetch", "--depth", "1", "--", "origin", reference]);
    run_git(&mut fetch, label)
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

    static FETCH_TEMP_NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    struct TempDir(PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn isolated_temp_dir() -> TempDir {
        let temp_path = env::temp_dir().join(format!(
            "keld-fetch-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos(),
            FETCH_TEMP_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        // Reserve the name before assigning cleanup ownership.
        fs::create_dir(&temp_path).expect("reserve unique temp directory");
        TempDir(temp_path)
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = git_command()
            .current_dir(dir)
            .args(args)
            .output()
            .expect("git installed");
        assert!(output.status.success(), "{args:?}: {output:?}");
        String::from_utf8(output.stdout).expect("git output")
    }

    // Quote a path for the shell Git uses to launch its upload-pack command.
    // Git for Windows uses forward-slash paths with the same POSIX quoting.
    fn shell_quote_path(path: &Path) -> String {
        let path = path.to_str().expect("test marker path must be UTF-8");
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        #[cfg(not(windows))]
        let path = path.to_owned();
        format!("'{}'", path.replace('\'', "'\\''"))
    }

    // Exercise real Git's option parser and local transport, not a command mock.
    fn check_fetch_reference_is_data(pinned: bool) {
        let temp = isolated_temp_dir();
        let origin = temp.0.join("origin");
        let dest = temp.0.join("checkout");
        fs::create_dir(&origin).expect("origin directory");
        fs::create_dir(&dest).expect("checkout directory");
        git(&origin, &["init", "--initial-branch=main"]);
        git(&origin, &["config", "maintenance.auto", "false"]);
        // Do not let the developer's global signing policy prevent the
        // disposable origin commit from reaching the fetch oracle.
        git(&origin, &["config", "--local", "commit.gpgSign", "false"]);
        git(
            &origin,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "fixture",
            ],
        );
        let sha = git(&origin, &["rev-parse", "HEAD"]).trim().to_string();
        git(&dest, &["init"]);
        git(&dest, &["config", "maintenance.auto", "false"]);
        // This fixture fetches from a local repository on purpose. Keep the
        // transport override inside this checkout, not in global Git policy.
        git(
            &dest,
            &["config", "--local", "protocol.file.allow", "always"],
        );
        git(
            &dest,
            &["remote", "add", "origin", origin.to_str().expect("path")],
        );
        let mut repo = sample_repos().remove(0);
        let marker = temp.0.join("upload-pack-executed");
        let malicious = format!(
            "--upload-pack=git --version > {}",
            shell_quote_path(&marker)
        );
        // Prove this exact Git transport and shell quoting can execute the
        // harmless marker command. Otherwise the rejection test could pass
        // because its negative oracle is incapable of seeing execution.
        let positive = git_command()
            .current_dir(&dest)
            .args(["fetch", "--depth", "1", &malicious, "origin", "main"])
            .output()
            .expect("Git must run the positive-control probe");
        assert!(
            marker.is_file(),
            "positive upload-pack probe did not create marker: {positive:?}"
        );
        fs::remove_file(&marker).expect("clear positive-control marker");

        let result = if pinned {
            fetch_reset_sha(&dest, &malicious, true)
        } else {
            // Exercise the shared fetch argument builder with a raw hostile
            // reference. Normal branch updates use a refs/heads/ prefix.
            // Removing the "--" in this builder must still fail this test.
            assert!(fetch_origin_reference(&dest, &malicious, "raw branch probe").is_err());
            repo.branch = malicious;
            update_existing(&repo, &dest, true)
        };
        assert!(result.is_err(), "invalid reference must fail");
        assert!(
            !marker.exists(),
            "reference executed the upload-pack command"
        );

        // Both legitimate paths must still fetch and reset to the upstream commit.
        if pinned {
            fetch_reset_sha(&dest, &sha, true).expect("pinned fetch");
        } else {
            repo.branch = "main".into();
            update_existing(&repo, &dest, true).expect("branch fetch");
        }
        assert_eq!(git(&dest, &["rev-parse", "HEAD"]).trim(), sha);
    }

    #[test]
    fn pinned_fetch_does_not_execute_reference() {
        check_fetch_reference_is_data(true);
    }

    #[test]
    fn branch_fetch_does_not_execute_reference() {
        check_fetch_reference_is_data(false);
    }

    #[test]
    fn accepts_literal_branches_and_full_object_ids() {
        let mut repo = sample_repos().remove(0);
        for branch in ["main", "dev", "feature/nested", "release/2026-11"] {
            repo.branch = branch.into();
            repo.sha = None;
            validate_reference_inputs(&repo).expect("literal Git branch");
        }
        for sha in ["a".repeat(40), "F".repeat(64)] {
            repo.branch = "main".into();
            repo.sha = Some(sha);
            validate_reference_inputs(&repo).expect("full Git object ID");
        }
    }

    #[test]
    fn pinned_identity_rejects_different_commit_before_reset() {
        let expected = "a".repeat(40);
        let other = "b".repeat(40);
        verify_pinned_identity(&expected, &expected.to_uppercase()).expect("same pinned hash");
        let error = verify_pinned_identity(&expected, &other).expect_err("different hash");
        assert!(error.contains("refusing to reset checkout"), "{error}");
    }

    #[test]
    fn pinned_tag_object_must_not_reset_to_peeled_commit() {
        let temp = isolated_temp_dir();
        let origin = temp.0.join("origin");
        let checkout = temp.0.join("checkout");
        fs::create_dir(&origin).expect("origin directory");
        fs::create_dir(&checkout).expect("checkout directory");
        git(&origin, &["init", "--initial-branch=main"]);
        git(&origin, &["config", "maintenance.auto", "false"]);
        git(&origin, &["config", "--local", "commit.gpgSign", "false"]);
        git(&origin, &["config", "--local", "tag.gpgSign", "false"]);
        git(
            &origin,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "initial",
            ],
        );
        let initial = git(&origin, &["rev-parse", "HEAD"]).trim().to_string();
        git(
            &origin,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "tag",
                "-a",
                "annotated",
                "-m",
                "tag object is not a commit",
            ],
        );
        let tag_oid = git(&origin, &["rev-parse", "refs/tags/annotated^{tag}"])
            .trim()
            .to_string();
        git(
            &origin,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "new main",
            ],
        );
        let later = git(&origin, &["rev-parse", "HEAD"]).trim().to_string();
        git(&checkout, &["init"]);
        git(&checkout, &["config", "maintenance.auto", "false"]);
        git(
            &checkout,
            &["config", "--local", "protocol.file.allow", "always"],
        );
        git(
            &checkout,
            &[
                "remote",
                "add",
                "origin",
                origin.to_str().expect("origin path"),
            ],
        );
        git(&checkout, &["fetch", "--", "origin", "refs/heads/main"]);
        git(&checkout, &["reset", "--hard", "FETCH_HEAD"]);
        assert_eq!(git(&checkout, &["rev-parse", "HEAD"]).trim(), later);
        let err = fetch_reset_sha(&checkout, &tag_oid, true).expect_err("non-commit pin");
        assert!(err.contains("fetched commit does not match"), "{err}");
        assert_eq!(
            git(&checkout, &["rev-parse", "FETCH_HEAD^{commit}"]).trim(),
            initial,
            "the raw tag object peels to a different commit"
        );
        assert_eq!(
            git(&checkout, &["rev-parse", "HEAD"]).trim(),
            later,
            "a mismatched pin must never reset the checkout"
        );
    }

    #[test]
    fn refuses_refspecs_and_revision_expressions() {
        let mut repo = sample_repos().remove(0);
        repo.sha = None;
        for branch in [
            "",
            "--upload-pack=command",
            "-side-effect",
            "main:refs/tags/side-effect",
            "+main:refs/heads/side-effect",
            "@{-1}",
            "HEAD",
            "main..other",
            "main^{}",
        ] {
            repo.branch = branch.into();
            assert!(
                validate_reference_inputs(&repo).is_err(),
                "should reject branch {branch:?}"
            );
        }
        repo.branch = "main".into();
        for sha in [
            "HEAD".to_string(),
            "deadbeef".to_string(),
            "main:refs/heads/side-effect".to_string(),
            "--upload-pack=command".to_string(),
            "f".repeat(39),
            "f".repeat(65),
            "g".repeat(40),
        ] {
            repo.sha = Some(sha.clone());
            assert!(
                validate_reference_inputs(&repo).is_err(),
                "should reject sha {sha:?}"
            );
        }
    }

    #[test]
    fn rejects_all_bad_pins_before_any_clone() {
        let temp = isolated_temp_dir();
        fs::write(
            temp.0.join("competitors.lock.toml"),
            r#"
[[repo]]
name = "first"
kind = "competitor"
url = "missing-local-origin"
branch = "main"
sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

[[repo]]
name = "second"
kind = "competitor"
url = "missing-local-origin"
branch = "main"
sha = "main:refs/heads/unexpected"
"#,
        )
        .expect("write hostile lockfile");
        let err = run(&temp.0, false, true).expect_err("invalid pin");
        assert!(err.contains("invalid sha"), "{err}");
        assert!(
            !temp.0.join("competitors").exists(),
            "reject malformed lockfile before fetching even the first repo"
        );
    }

    #[test]
    fn rejects_late_bad_or_overlapping_paths_before_any_clone() {
        let temp = isolated_temp_dir();
        for path in [
            "competitors/../outside",
            "competitors/first",
            "competitors/first/nested",
        ] {
            let lock = format!(
                r#"
[[repo]]
name = "first"
kind = "competitor"
url = "missing-local-origin"
branch = "main"

[[repo]]
name = "second"
kind = "competitor"
url = "missing-local-origin"
branch = "main"
path = "{path}"
"#
            );
            fs::write(temp.0.join("competitors.lock.toml"), lock).expect("write lockfile");
            let err = run(&temp.0, false, true).expect_err("invalid checkout destination");
            assert!(
                err.contains("normalized") || err.contains("overlaps"),
                "path={path:?}: {err}"
            );
            assert!(
                !temp.0.join("competitors").exists(),
                "path={path:?}: must reject before cloning any checkout"
            );
        }
    }

    #[test]
    fn hex_named_branch_uses_fetched_head_not_an_object_or_stale_ref() {
        let temp = isolated_temp_dir();
        let origin = temp.0.join("-origin");
        let checkout = temp.0.join("checkout");
        fs::create_dir(&origin).expect("create origin");
        git(&origin, &["init", "--initial-branch=main"]);
        git(&origin, &["config", "--local", "commit.gpgSign", "false"]);
        git(&origin, &["config", "maintenance.auto", "false"]);
        for label in ["initial", "new branch target"] {
            git(
                &origin,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-m",
                    label,
                ],
            );
            if label == "initial" {
                let first = git(&origin, &["rev-parse", "HEAD"]).trim().to_owned();
                git(&origin, &["branch", &first]);
            }
        }
        let first = git(&origin, &["rev-parse", "HEAD^"]).trim().to_owned();
        let second = git(&origin, &["rev-parse", "HEAD"]).trim().to_owned();
        git(&origin, &["branch", "--force", &first, "HEAD"]);
        let mut repo = sample_repos().remove(0);
        repo.url = "-origin".to_owned();
        repo.branch = "main".to_owned();
        repo.sha = None;
        // Exercise the production clone argument builder with an actual local
        // path beginning with '-'. Without its '--', Git misparses this URL.
        let mut clone = clone_command(&repo, &checkout);
        clone.current_dir(&temp.0);
        // The test owns this one subprocess; it must not change the machine's
        // global file-transport policy.
        clone.env("GIT_CONFIG_COUNT", "1");
        clone.env("GIT_CONFIG_KEY_0", "protocol.file.allow");
        clone.env("GIT_CONFIG_VALUE_0", "always");
        run_git(&mut clone, "clone literal dash-prefixed origin")
            .expect("clone a literal dash-prefixed origin");
        git(
            &checkout,
            &[
                "remote",
                "set-url",
                "origin",
                origin.to_str().expect("UTF-8 origin"),
            ],
        );
        git(
            &checkout,
            &["config", "--local", "protocol.file.allow", "always"],
        );
        // Model a shallow checkout that tracks only main but has an old
        // remote-tracking branch. That ref can be stale and still resolve.
        git(
            &checkout,
            &[
                "config",
                "--local",
                "remote.origin.fetch",
                "+refs/heads/main:refs/remotes/origin/main",
            ],
        );
        git(
            &checkout,
            &["fetch", "--depth", "2", "--", "origin", &first],
        );
        let stale_ref = format!("refs/remotes/origin/{first}");
        git(&checkout, &["update-ref", &stale_ref, &first]);
        assert_eq!(git(&checkout, &["rev-parse", &stale_ref]).trim(), first);
        repo.branch = first.clone();
        validate_reference_inputs(&repo).expect("40-hex branch is a valid name");
        update_existing(&repo, &checkout, true).expect("update literal hex-named branch");
        assert_eq!(
            git(&checkout, &["rev-parse", "HEAD"]).trim(),
            second,
            "must use remote branch target, not the same-named object or stale tracking ref"
        );
        // Prove the reset helper does not resolve the mutable FETCH_HEAD
        // again after a different fetch changes it.
        git(&checkout, &["fetch", "--", "origin", &first]);
        reset_to_commit(&checkout, &second, "reset captured object").expect("immutable reset");
        assert_eq!(git(&checkout, &["rev-parse", "HEAD"]).trim(), second);
    }

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
        let current = env::current_dir().expect("current directory");
        let root = current
            .ancestors()
            .find(|candidate| candidate.join("competitors.lock.toml").is_file())
            .map(Path::to_path_buf)
            .expect("run parser tests from within a Keld checkout");
        let text = fs::read_to_string(root.join("competitors.lock.toml"))
            .expect("read the checkout's competitors.lock.toml");
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
            err.contains('.') || err.contains("escapes") || err.contains("normalized"),
            "{err}"
        );
    }

    #[test]
    fn rejects_absolute_path_override() {
        let mut repo = sample_repos().remove(0);
        repo.path_override = Some(env::temp_dir().join("evil").to_string_lossy().into_owned());
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
