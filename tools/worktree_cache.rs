//! KELD worktree build-cache inventory and opt-in guarded deletion.
//! No external Rust crates. The delete path is a cooperative developer tool, not an OS sandbox.
use std::ffi::OsString;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[path = "worktree_cache_delete.rs"]
mod cleanup;

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let result = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|err| format!("cannot run git: {err}"))?;
    if !result.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    Ok(result.stdout)
}

#[cfg(unix)]
fn git_path(bytes: &[u8]) -> Result<PathBuf, String> {
    Ok(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(windows)]
fn git_path(bytes: &[u8]) -> Result<PathBuf, String> {
    String::from_utf8(bytes.to_vec())
        .map(PathBuf::from)
        .map_err(|_| "git supplied an invalid UTF-8 worktree path".to_string())
}

fn worktree_paths(data: &[u8]) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for field in data.split(|&c| c == 0) {
        if let Some(raw_path) = field.strip_prefix(b"worktree ") {
            paths.push(git_path(raw_path)?);
        }
    }
    if paths.is_empty() {
        return Err("git reported no registered worktrees".into());
    }
    Ok(paths)
}

#[cfg(windows)]
fn is_reparse(meta: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
}

#[cfg(not(windows))]
fn is_reparse(_meta: &Metadata) -> bool {
    false
}

fn is_link(meta: &Metadata) -> bool {
    meta.file_type().is_symlink() || is_reparse(meta)
}

fn directory_bytes(root: &Path) -> Result<(u64, usize), String> {
    let mut total = 0u64;
    let mut skipped_links = 0usize;
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for item in entries {
            let item = item.map_err(|e| format!("{}: {e}", dir.display()))?;
            let path = item.path();
            let meta =
                fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            if is_link(&meta) {
                skipped_links += 1;
            } else if meta.is_dir() {
                pending.push(path);
            } else if meta.is_file() {
                total = total
                    .checked_add(meta.len())
                    .ok_or_else(|| "cache byte count overflow".to_string())?;
            }
        }
    }
    Ok((total, skipped_links))
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.2} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(windows)]
fn disk_space(_repo: &Path) -> Result<Vec<String>, String> {
    let script = "Get-PSDrive -PSProvider FileSystem | Where-Object { $null -ne $_.Free } | ForEach-Object { Write-Output ($_.Name + '=' + $_.Free) }";
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|e| format!("PowerShell disk query failed: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let mut rows = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some((drive, number)) = line.trim().split_once('=')
            && let Ok(bytes) = number.parse::<u64>()
        {
            rows.push(format!("{drive}: free {}", format_size(bytes)));
        }
    }
    if rows.is_empty() {
        return Err("PowerShell returned no readable drive capacities".into());
    }
    Ok(rows)
}

#[cfg(unix)]
fn disk_space(repo: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("df")
        .arg("-Pk")
        .arg(repo)
        .output()
        .map_err(|e| format!("df disk query failed: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let data = text
        .lines()
        .nth(1)
        .ok_or("df returned no filesystem data")?;
    let parts: Vec<_> = data.split_whitespace().collect();
    let kilobytes: u64 = parts
        .get(3)
        .ok_or("df returned an unexpected format")?
        .parse()
        .map_err(|_| "df reported invalid available space")?;
    let bytes = kilobytes
        .checked_mul(1024)
        .ok_or("disk capacity overflow")?;
    Ok(vec![format!(
        "Current worktree filesystem: free {}",
        format_size(bytes)
    )])
}

fn run(args: &[OsString]) -> Result<(), String> {
    if args.get(1).is_some_and(|value| value == "delete") {
        let repo = args.get(2).ok_or(
            "Usage: worktree-cache delete REPO (--all | --worktree PATH) [--dry-run | --yes]",
        )?;
        return cleanup::run(Path::new(repo), &args[3..]);
    }
    if args.get(1).is_none_or(|value| value != "check") {
        return Err("Usage: worktree-cache check [repository-path] [--all] | delete REPO (--all | --worktree PATH) [--dry-run | --yes]".into());
    }
    let mut all = false;
    let mut repo = PathBuf::from(".");
    let mut seen_repo = false;
    for arg in &args[2..] {
        if arg == "--all" {
            all = true;
        } else if !seen_repo {
            repo = PathBuf::from(arg);
            seen_repo = true;
        } else {
            return Err(format!("unexpected argument: {}", arg.to_string_lossy()));
        }
    }
    let root = git(&repo, &["rev-parse", "--show-toplevel"])?;
    let root = git_path(root.strip_suffix(b"\n").unwrap_or(&root))?;
    let worktrees = worktree_paths(&git(&root, &["worktree", "list", "--porcelain", "-z"])?)?;
    let mut rows = Vec::<(u64, PathBuf, usize)>::new();
    let mut linked_targets = 0usize;
    for tree in &worktrees {
        let target = tree.join("target");
        let meta = match fs::symlink_metadata(&target) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("{}: {error}", target.display())),
        };
        if is_link(&meta) {
            linked_targets += 1;
            continue;
        }
        if !meta.is_dir() {
            continue;
        }
        let (bytes, links) = directory_bytes(&target)?;
        rows.push((bytes, target, links));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let total = rows
        .iter()
        .try_fold(0u64, |total, row| total.checked_add(row.0))
        .ok_or("aggregate cache byte count overflow")?;
    println!("Registered Git worktrees: {}", worktrees.len());
    println!("Local target/ caches: {}", rows.len());
    println!(
        "Total logical cache bytes: {} ({total} bytes)",
        format_size(total)
    );
    println!("Linked/reparse target paths excluded: {linked_targets}");
    let skipped: usize = rows.iter().map(|row| row.2).sum();
    if skipped != 0 {
        println!("Linked/reparse descendants excluded: {skipped}");
    }
    match disk_space(&root) {
        Ok(disks) => {
            for disk in disks {
                println!("{disk}");
            }
        }
        Err(error) => eprintln!("Disk capacity unavailable: {error}"),
    }
    println!("Largest cache folders:");
    for (bytes, target, _) in rows.iter().take(if all { usize::MAX } else { 10 }) {
        println!("  {:>10}  {}", format_size(*bytes), target.display());
    }
    if !all && rows.len() > 10 {
        println!("  ... {} more (run with --all)", rows.len() - 10);
    }
    println!("Read-only inventory. Logical size is not guaranteed recoverable disk space.");
    Ok(())
}

fn main() -> ExitCode {
    match run(&std::env::args_os().collect::<Vec<_>>()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("worktree-cache: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_nul_delimited_paths_with_spaces() {
        let paths = worktree_paths(
            b"worktree C:/Temp/working tree\0HEAD abc\0branch refs/heads/main\0\0worktree D:/more\0HEAD def\0detached\0\0",
        ).unwrap();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("C:/Temp/working tree"),
                PathBuf::from("D:/more")
            ]
        );
    }
    #[test]
    fn empty_worktrees_fail_closed() {
        assert!(worktree_paths(b"").is_err());
    }
    #[test]
    fn counts_real_files_recursively() {
        let root = std::env::temp_dir().join(format!(
            "keld-cache-check-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("a"), b"123").unwrap();
        fs::write(root.join("nested").join("b"), b"12345").unwrap();
        let (size, links) = directory_bytes(&root).unwrap();
        assert_eq!(size, 8);
        assert_eq!(links, 0);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn size_format_keeps_iec_units() {
        assert_eq!(format_size(1_073_741_824), "1.00 GiB");
        assert_eq!(format_size(1_048_576), "1.00 MiB");
    }
    #[test]
    fn unscoped_deletion_is_refused() {
        let args = vec![
            OsString::from("worktree-cache"),
            OsString::from("delete"),
            OsString::from("."),
        ];
        assert!(run(&args).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlink_descendants() {
        use std::os::unix::fs::symlink;
        let parent =
            std::env::temp_dir().join(format!("keld-cache-symlink-{}", std::process::id()));
        let root = parent.join("target");
        let outside = parent.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret"), [0u8; 200]).unwrap();
        symlink(&outside, root.join("linked")).unwrap();
        let (size, links) = directory_bytes(&root).unwrap();
        assert_eq!(size, 0);
        assert_eq!(links, 1);
        fs::remove_dir_all(&parent).unwrap();
    }
}
