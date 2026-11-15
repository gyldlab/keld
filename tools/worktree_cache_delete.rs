//! Only disposable Cargo targets in Git-registered worktrees may be deleted.
//! Cooperative cleanup, not an adversarial filesystem sandbox.
use super::{directory_bytes, git, git_path, is_link, worktree_paths};
use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
struct Options {
    all: bool,
    yes: bool,
    dry_run: bool,
    worktree: Option<PathBuf>,
}

impl Options {
    fn parse(args: &[OsString]) -> Result<Self, String> {
        let mut value = Self {
            all: false,
            yes: false,
            dry_run: false,
            worktree: None,
        };
        let mut items = args.iter();
        while let Some(arg) = items.next() {
            if arg == "--all" && !value.all {
                value.all = true;
            } else if arg == "--yes" && !value.yes {
                value.yes = true;
            } else if arg == "--dry-run" && !value.dry_run {
                value.dry_run = true;
            } else if arg == "--worktree" && value.worktree.is_none() {
                value.worktree = Some(PathBuf::from(
                    items.next().ok_or("--worktree requires PATH")?,
                ));
            } else {
                return Err(format!(
                    "invalid/duplicate option: {}",
                    arg.to_string_lossy()
                ));
            }
        }
        if value.all == value.worktree.is_some() {
            return Err("select exactly one of --all or --worktree PATH".into());
        }
        if value.yes && value.dry_run {
            return Err("--yes conflicts with --dry-run".into());
        }
        Ok(value)
    }
}

/// Cooperative heuristic, not a complete active-build proof: it matches the
/// exact executable names of the Cargo, rustc, C/C++ compiler and linker
/// drivers a Keld build spawns. Hash-suffixed build scripts, wrapper names,
/// target-prefixed cross tools and Linux `comm` values truncated to 15 bytes
/// are not recognized, so a clean census lowers but never removes the risk
/// of deleting a target that is being written.
fn active_build_name(name: &str) -> bool {
    let name = name
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    matches!(
        name,
        // Cargo and Rust drivers.
        "cargo"
            | "cargo-clippy"
            | "cargo-nextest"
            | "rustc"
            | "rustdoc"
            | "nextest"
            | "clippy-driver"
            // C and C++ compiler drivers.
            | "cc"
            | "c++"
            | "clang"
            | "clang++"
            | "clang-cl"
            | "gcc"
            | "g++"
            | "cc1"
            | "cc1plus"
            | "cl"
            // Linkers, including rustc's bundled LLD flavors.
            | "ld"
            | "ld.bfd"
            | "ld.gold"
            | "ld.lld"
            | "ld64"
            | "ld64.lld"
            | "lld"
            | "lld-link"
            | "rust-lld"
            | "mold"
            | "collect2"
            | "link"
            | "ninja"
    )
}

#[cfg(unix)]
fn running_process_names() -> Result<Vec<u8>, String> {
    let out = Command::new("ps")
        .args(["-A", "-o", "comm="])
        .output()
        .map_err(|e| format!("cannot inspect Unix processes: {e}"))?;
    if !out.status.success() {
        return Err("Unix process inspection failed".into());
    }
    Ok(out.stdout)
}
#[cfg(windows)]
fn running_process_names() -> Result<Vec<u8>, String> {
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-Process | Select-Object -ExpandProperty ProcessName",
        ])
        .output()
        .map_err(|e| format!("cannot inspect Windows processes: {e}"))?;
    if !out.status.success() {
        return Err("Windows process inspection failed".into());
    }
    Ok(out.stdout)
}
fn validate_process_inventory(data: &[u8]) -> Result<(), String> {
    let names = std::str::from_utf8(data).map_err(|_| "process names not valid UTF-8")?;
    let mut observed = 0usize;
    for name in names.lines().map(str::trim).filter(|name| !name.is_empty()) {
        observed += 1;
        if active_build_name(name) {
            return Err(format!(
                "active compiler/build {name:?} detected; refusing cleanup"
            ));
        }
    }
    if observed == 0 {
        return Err("empty process inventory; refusing cleanup".into());
    }
    Ok(())
}
fn ensure_no_builds() -> Result<(), String> {
    validate_process_inventory(&running_process_names()?)
}

fn real_ancestors(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        let meta =
            fs::symlink_metadata(ancestor).map_err(|e| format!("{}: {e}", ancestor.display()))?;
        if is_link(&meta) {
            return Err(format!(
                "linked/reparse worktree ancestor: {}",
                ancestor.display()
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn same_device(root: &Metadata, item: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    root.dev() == item.dev()
}
#[cfg(windows)]
fn same_device(_root: &Metadata, _item: &Metadata) -> bool {
    // Mounted Win32 folders have the reparse flag and are rejected separately.
    true
}

#[cfg(unix)]
fn decode_mount_field(raw: &str) -> Result<PathBuf, String> {
    use std::os::unix::ffi::OsStringExt;
    let b = raw.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            if i + 3 >= b.len() {
                return Err("truncated mountinfo escape".into());
            }
            let mut v = 0u8;
            for digit in &b[i + 1..i + 4] {
                if !(b'0'..=b'7').contains(digit) {
                    return Err("malformed mountinfo escape".into());
                }
                v = v
                    .checked_mul(8)
                    .and_then(|number| number.checked_add(digit - b'0'))
                    .ok_or("octal mount escape exceeds one byte")?;
            }
            out.push(v);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Ok(PathBuf::from(std::ffi::OsString::from_vec(out)))
}

#[cfg(target_os = "linux")]
fn mount_points() -> Result<Vec<PathBuf>, String> {
    // Parse every mount ID, not just device IDs: a bind mount can have the
    // same st_dev as target while exposing unrelated source contents.
    let text = fs::read_to_string("/proc/self/mountinfo")
        .map_err(|e| format!("Linux mountinfo unavailable: {e}"))?;
    let mut mounts = Vec::new();
    for row in text.lines() {
        let fields: Vec<_> = row.split_ascii_whitespace().collect();
        if fields.len() < 7 {
            return Err("truncated mountinfo row".into());
        }
        mounts.push(decode_mount_field(fields[4])?);
    }
    if mounts.is_empty() {
        return Err("empty mountinfo inventory".into());
    }
    Ok(mounts)
}

#[cfg(target_os = "macos")]
fn mount_points() -> Result<Vec<PathBuf>, String> {
    let output = Command::new("mount")
        .output()
        .map_err(|e| format!("macOS mount inventory unavailable: {e}"))?;
    if !output.status.success() {
        return Err("macOS mount inventory failed".into());
    }
    let output = String::from_utf8(output.stdout).map_err(|_| "mount output not UTF-8")?;
    let mut mounts = Vec::new();
    for line in output.lines() {
        let (_, rest) = line.split_once(" on ").ok_or("unrecognized mount entry")?;
        let (mount, _) = rest.rsplit_once(" (").ok_or("unrecognized mount options")?;
        mounts.push(decode_mount_field(mount)?);
    }
    if mounts.is_empty() {
        return Err("empty macOS mount inventory".into());
    }
    Ok(mounts)
}

#[cfg(windows)]
fn mount_points() -> Result<Vec<PathBuf>, String> {
    // Windows mounted directories are reparse points and are rejected by walk.
    Ok(vec![])
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn mount_points() -> Result<Vec<PathBuf>, String> {
    Err("unsupported filesystem mount inventory".into())
}

fn no_mount_overlap(target: &Path, mounts: &[PathBuf]) -> Result<(), String> {
    for mount in mounts {
        if mount.starts_with(target) {
            return Err(format!(
                "mounted filesystem inside target: {}",
                mount.display()
            ));
        }
    }
    Ok(())
}
fn no_nested_mounts(target: &Path) -> Result<(), String> {
    no_mount_identity_overlap(target, mount_points()?)
}
fn no_mount_identity_overlap(target: &Path, mounts: Vec<PathBuf>) -> Result<(), String> {
    let physical = mounts
        .iter()
        .map(|path| mount_identity(path))
        .collect::<Result<Vec<_>, _>>()?;
    no_mount_overlap(target, &physical)
}
/// Physical spelling of a mount point. A stale (ENOENT) or unreadable
/// (EACCES) entry resolves through its deepest canonicalizable ancestor, so an
/// unrelated entry no longer aborts preflight while any alias of a mount inside
/// the canonical target still lands inside it. Mount tables record
/// kernel-resolved paths, so the unresolved suffix holds no symlink; a suffix
/// that cannot be placed (`..`, no file name) fails closed.
fn mount_identity(path: &Path) -> Result<PathBuf, String> {
    let mut suffix = Vec::new();
    let mut cursor = path;
    loop {
        match fs::canonicalize(cursor) {
            Ok(mut resolved) => {
                resolved.extend(suffix.iter().rev());
                return Ok(resolved);
            }
            Err(e) => match (cursor.parent(), cursor.file_name()) {
                (Some(parent), Some(name)) => {
                    suffix.push(name);
                    cursor = parent;
                }
                _ => return Err(format!("mount identity {}: {e}", path.display())),
            },
        }
    }
}

fn ignored_untracked(tree: &Path) -> Result<(), String> {
    if !git(tree, &["ls-files", "-z", "--", "target"])?.is_empty() {
        return Err(format!(
            "tracked files present in target/: {}",
            tree.display()
        ));
    }
    let result = Command::new("git")
        .arg("-C")
        .arg(tree)
        .args(["check-ignore", "-q", "--", "target/"])
        .status()
        .map_err(|e| format!("cannot inspect Git ignored paths: {e}"))?;
    if !result.success() {
        return Err(format!("target/ is not Git-ignored: {}", tree.display()));
    }
    Ok(())
}

fn ensure_cargo_locks_free(target: &Path) -> Result<(), String> {
    let root_meta =
        fs::symlink_metadata(target).map_err(|e| format!("{}: {e}", target.display()))?;
    let mut pending = vec![target.to_path_buf()];
    let mut held = Vec::<File>::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            let path = entry.path();
            let meta =
                fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            if is_link(&meta) {
                return Err(format!(
                    "link/reparse entry inside target: {}",
                    path.display()
                ));
            }
            if !same_device(&root_meta, &meta) {
                return Err(format!(
                    "cross-filesystem entry inside target: {}",
                    path.display()
                ));
            }
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(".git")
            {
                return Err(format!(
                    "nested Git source inside target: {}",
                    path.display()
                ));
            }
            if meta.is_dir() {
                pending.push(path);
            } else if !meta.is_file() {
                return Err(format!("non-regular cache entry: {}", path.display()));
            } else if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(".cargo-lock")
            {
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .map_err(|e| format!("cannot open Cargo lock {}: {e}", path.display()))?;
                file.try_lock()
                    .map_err(|e| format!("Cargo lock busy {}: {e}", path.display()))?;
                held.push(file);
            }
        }
    }
    // Windows cannot remove the locked file while our own HANDLE is open.
    // The process inventory is rechecked after release, but this is still a
    // cooperative best-effort boundary, not an atomic Cargo revocation.
    for lock in &held {
        lock.unlock()
            .map_err(|e| format!("unlocking Cargo lock: {e}"))?;
    }
    Ok(())
}

#[derive(Debug)]
struct Target {
    dir: PathBuf,
    bytes: u64,
}

fn preflight(tree: &Path) -> Result<Option<Target>, String> {
    real_ancestors(tree)?;
    let canonical_tree = fs::canonicalize(tree)
        .map_err(|e| format!("registered worktree {}: {e}", tree.display()))?;
    let reported = git(tree, &["rev-parse", "--show-toplevel"])?;
    let reported = git_path(reported.strip_suffix(b"\n").unwrap_or(&reported))?;
    if fs::canonicalize(reported).map_err(|e| format!("Git identity: {e}"))? != canonical_tree {
        return Err(format!(
            "registered Git worktree changed: {}",
            tree.display()
        ));
    }
    let target = tree.join("target");
    let meta = match fs::symlink_metadata(&target) {
        Ok(meta) => meta,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", target.display())),
    };
    if !meta.is_dir() || is_link(&meta) {
        return Err(format!(
            "target is not a real directory: {}",
            target.display()
        ));
    }
    let real_target =
        fs::canonicalize(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    if real_target != canonical_tree.join("target") {
        return Err(format!(
            "target escapes registered worktree: {}",
            target.display()
        ));
    }
    let executable =
        fs::canonicalize(std::env::current_exe().map_err(|e| format!("current executable: {e}"))?)
            .map_err(|e| format!("canonical executable: {e}"))?;
    if executable.starts_with(&real_target) {
        return Err("cache contains running cleanup executable; preserving it".into());
    }
    let cwd =
        fs::canonicalize(std::env::current_dir().map_err(|e| format!("current directory: {e}"))?)
            .map_err(|e| format!("canonical working directory: {e}"))?;
    if cwd.starts_with(&real_target) {
        return Err("cache contains current working directory; preserving it".into());
    }
    ignored_untracked(tree)?;
    no_nested_mounts(&real_target)?;
    let (bytes, links) = directory_bytes(&target)?;
    if links != 0 {
        return Err(format!(
            "target contains {links} links/reparse points: {}",
            target.display()
        ));
    }
    ensure_cargo_locks_free(&target)?;
    Ok(Some(Target { dir: target, bytes }))
}

pub(super) fn run(repo: &Path, args: &[OsString]) -> Result<(), String> {
    run_with_guard(repo, args, ensure_no_builds)
}

// Test fixtures inject the process state, but production always obtains a
// fresh OS-visible process census before and during each deletion.
fn run_with_guard(
    repo: &Path,
    args: &[OsString],
    build_guard: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let options = Options::parse(args)?;
    let original_registry = git(repo, &["worktree", "list", "--porcelain", "-z"])?;
    let listed = worktree_paths(&original_registry)?;
    let canonical = listed
        .iter()
        .map(|path| {
            fs::canonicalize(path)
                .map_err(|e| format!("registered worktree {}: {e}", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let selected: Vec<PathBuf> = if let Some(path) = options.worktree {
        let physical = fs::canonicalize(&path)
            .map_err(|e| format!("requested worktree {}: {e}", path.display()))?;
        let matches: Vec<_> = listed
            .iter()
            .zip(canonical.iter())
            .filter(|(_, root)| **root == physical)
            .map(|(original, _)| original.clone())
            .collect();
        if matches.len() != 1 {
            return Err("requested worktree is not uniquely Git-registered".into());
        }
        matches
    } else {
        listed
    };
    let mut targets = Vec::new();
    for tree in &selected {
        let candidate = tree.join("target");
        // A separate Git worktree may have been registered *inside* an
        // ignored target. Inspect all registrations, including unselected.
        if fs::symlink_metadata(&candidate).is_ok() {
            let candidate_root = fs::canonicalize(&candidate)
                .map_err(|e| format!("{}: {e}", candidate.display()))?;
            if let Some(child) = canonical
                .iter()
                .find(|root| root.starts_with(&candidate_root))
            {
                return Err(format!(
                    "target contains another registered worktree {}; no deletion",
                    child.display()
                ));
            }
        }
        if let Some(target) = preflight(tree)? {
            targets.push(target);
        }
    }
    let bytes = targets
        .iter()
        .try_fold(0u64, |sum, item| sum.checked_add(item.bytes))
        .ok_or("cache size sum overflow")?;
    // The read-only process census runs before any eligibility report, so a
    // dry run refuses exactly when the matching --yes run would refuse.
    build_guard()?;
    println!(
        "Eligible targets: {} in {} worktrees; {bytes} logical bytes",
        targets.len(),
        selected.len()
    );
    for target in &targets {
        println!("  {} ({} bytes)", target.dir.display(), target.bytes);
    }
    if options.dry_run {
        println!("Dry run: no files were deleted.");
        return Ok(());
    }
    if !options.yes {
        return Err("no files deleted; supply --yes or use --dry-run".into());
    }
    let mut removed = 0usize;
    for target in &targets {
        build_guard()?;
        // A nested worktree may be registered after the first inventory.
        // All registered roots are source, even if their .git marker vanished.
        if git(repo, &["worktree", "list", "--porcelain", "-z"])? != original_registry {
            return Err(format!(
                "Git worktree registration changed during cleanup; stopped after {removed} removals"
            ));
        }
        let tree = target.dir.parent().ok_or("cache target missing parent")?;
        let refreshed =
            preflight(tree)?.ok_or_else(|| format!("target vanished: {}", target.dir.display()))?;
        if refreshed.bytes != target.bytes || refreshed.dir != target.dir {
            return Err(format!(
                "target changed during validation; stopped after {removed} removals"
            ));
        }
        fs::remove_dir_all(&target.dir)
            .map_err(|e| format!("deleting {}: {e}", target.dir.display()))?;
        removed += 1;
        println!("REMOVED {}", target.dir.display());
    }
    println!("Removed {removed} target directories ({bytes} logical bytes inventoried).");
    Ok(())
}

#[cfg(test)]
#[path = "worktree_cache_delete_tests.rs"]
mod tests;
