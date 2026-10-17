//! Native Git and filesystem negative controls for guarded cache removal.
use super::{Options, active_build_name, run_with_guard};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    secondary: PathBuf,
}
impl Fixture {
    fn git(repo: &Path, args: &[&str]) {
        let result = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fn new(ignored: bool) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let tick = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let serial = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "keld-cache-native-{}-{tick}-{serial}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap(); // atomic uniqueness check
        let repo = root.join("primary repo");
        let secondary = root.join("secondary worktree");
        fs::create_dir(&repo).unwrap();
        Self::git(&repo, &["init", "-q"]);
        Self::git(&repo, &["config", "user.name", "KELD test"]);
        Self::git(&repo, &["config", "user.email", "example@test.invalid"]);
        fs::write(repo.join("README.md"), "PRESERVE SOURCE").unwrap();
        if ignored {
            fs::write(repo.join(".gitignore"), "/target/\n").unwrap();
        }
        Self::git(&repo, &["add", "."]);
        Self::git(
            &repo,
            &["-c", "commit.gpgsign=false", "commit", "-qm", "fixture"],
        );
        let result = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["worktree", "add", "-q", "-b", "secondary"])
            .arg(&secondary)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        fs::write(secondary.join("keep-untracked"), "STILL HERE").unwrap();
        fs::create_dir_all(secondary.join("target").join("debug")).unwrap();
        fs::write(
            secondary.join("target").join("debug").join("cache.bin"),
            b"cache!",
        )
        .unwrap();
        Self {
            root,
            repo,
            secondary,
        }
    }
    fn args(&self) -> Vec<OsString> {
        vec!["--worktree".into(), self.secondary.as_os_str().to_owned()]
    }
    fn run(&self, args: &[OsString]) -> Result<(), String> {
        run_with_guard(&self.repo, args, || Ok(()))
    }
    fn target(&self) -> PathBuf {
        self.secondary.join("target")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root); // uniquely named test-only fixture
    }
}

#[test]
fn explicit_scope_and_yes_are_required() {
    assert!(Options::parse(&[]).is_err());
    assert!(Options::parse(&["--yes".into()]).is_err());
    assert!(Options::parse(&["--all".into(), "--worktree".into(), ".".into()]).is_err());
    assert!(Options::parse(&["--all".into(), "--yes".into(), "--dry-run".into()]).is_err());
}
#[test]
fn known_process_names_are_detected_without_resident_ide_false_positives() {
    for name in [
        "cargo",
        "/usr/bin/rustc",
        "clang",
        "clippy-driver",
        "rustdoc.exe",
    ] {
        assert!(active_build_name(name), "{name}");
    }
    for name in ["rust-analyzer", "sccache", "git", "bash", "notcargo"] {
        assert!(!active_build_name(name), "{name}");
    }
}
#[test]
fn empty_or_incomplete_process_inventory_refuses_cleanup() {
    use super::validate_process_inventory;
    assert!(validate_process_inventory(b"").is_err());
    assert!(validate_process_inventory(b" \n  \t\n").is_err());
    assert!(validate_process_inventory(b"cargo\n").is_err());
    assert!(validate_process_inventory(b"rustc.exe\n").is_err());
    assert!(validate_process_inventory(b"bash\nps\n").is_ok());
}
#[test]
fn without_yes_or_dry_run_never_deletes() {
    let f = Fixture::new(true);
    assert!(f.run(&f.args()).is_err());
    assert!(f.target().exists());
    let mut dry = f.args();
    dry.push("--dry-run".into());
    f.run(&dry).unwrap();
    assert!(f.target().exists());
}
#[test]
fn removes_only_explicitly_selected_ignored_target() {
    let f = Fixture::new(true);
    let mut args = f.args();
    args.push("--yes".into());
    f.run(&args).unwrap();
    assert!(!f.target().exists());
    assert_eq!(
        fs::read_to_string(f.repo.join("README.md")).unwrap(),
        "PRESERVE SOURCE"
    );
    assert_eq!(
        fs::read_to_string(f.secondary.join("keep-untracked")).unwrap(),
        "STILL HERE"
    );
    assert!(f.secondary.join(".git").exists());
}
#[test]
fn refuses_target_without_ignore_policy() {
    let f = Fixture::new(false);
    let mut args = f.args();
    args.push("--yes".into());
    let err = f.run(&args).unwrap_err();
    assert!(err.contains("not Git-ignored"), "{err}");
    assert!(f.target().exists());
}
#[test]
fn refuses_busy_native_cargo_lock() {
    let f = Fixture::new(true);
    let path = f.target().join("debug").join(".cargo-lock");
    fs::write(&path, "").unwrap();
    let owner: File = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    owner.try_lock().unwrap();
    let mut args = f.args();
    args.push("--dry-run".into());
    let err = f.run(&args).unwrap_err();
    assert!(err.contains("Cargo lock"), "{err}");
    owner.unlock().unwrap();
    assert!(f.target().exists());
}
#[test]
fn refuses_nested_git_repository_inside_build_cache() {
    let f = Fixture::new(true);
    fs::create_dir_all(f.target().join("debug").join(".git")).unwrap();
    let mut args = f.args();
    args.push("--yes".into());
    let err = f.run(&args).unwrap_err();
    assert!(err.contains("nested Git"), "{err}");
    assert!(f.target().exists());
}
#[test]
fn refuses_casefolded_nested_git_marker_and_preserves_unregistered_source() {
    let f = Fixture::new(true);
    let nested = f.target().join("debug").join("unregistered repo");
    fs::create_dir_all(&nested).unwrap();
    Fixture::git(&nested, &["init", "-q"]);
    fs::write(nested.join("source"), "PRESERVE").unwrap();
    fs::rename(nested.join(".git"), nested.join(".GiT")).unwrap();
    let mut args = f.args();
    args.push("--yes".into());
    let error = f.run(&args).unwrap_err();
    assert!(error.contains("nested Git"), "{error}");
    assert_eq!(
        fs::read_to_string(nested.join("source")).unwrap(),
        "PRESERVE"
    );
}

#[test]
fn refuses_casefolded_held_cargo_lock() {
    let f = Fixture::new(true);
    let path = f.target().join("debug").join(".CARGO-LOCK");
    fs::write(&path, "").unwrap();
    let handle = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    handle.try_lock().unwrap();
    let mut args = f.args();
    args.push("--dry-run".into());
    let error = f.run(&args).unwrap_err();
    assert!(error.contains("Cargo lock"), "{error}");
    handle.unlock().unwrap();
    assert!(f.target().exists());
}

#[test]
fn refuses_other_registered_worktree_inside_selected_cache() {
    let f = Fixture::new(true);
    let path = f.repo.join("target").join("nested-source");
    fs::create_dir_all(f.repo.join("target")).unwrap();
    let result = Command::new("git")
        .arg("-C")
        .arg(&f.repo)
        .args(["worktree", "add", "-q", "-b", "nested"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let secondary_source = path.join("README.md");
    assert!(secondary_source.is_file());
    // Even with the physical Git marker missing, registered source is not cache.
    // This is independent of the separate nested-.git refusal.
    fs::remove_file(path.join(".git")).unwrap();
    let args = [
        "--worktree".into(),
        f.repo.as_os_str().to_owned(),
        "--yes".into(),
    ];
    let err = f.run(&args).unwrap_err();
    assert!(err.contains("another registered worktree"), "{err}");
    assert_eq!(
        fs::read_to_string(&secondary_source).unwrap(),
        "PRESERVE SOURCE"
    );
}
#[test]
fn concurrent_new_worktree_registration_refuses_before_deletion() {
    use super::run_with_guard;
    use std::cell::Cell;
    let f = Fixture::new(true);
    let nested = f.target().join("late registered");
    let calls = Cell::new(0u8);
    let mut args = f.args();
    args.push("--yes".into());
    let err = run_with_guard(&f.repo, &args, || {
        if calls.get() == 0 {
            let result = Command::new("git")
                .arg("-C")
                .arg(&f.repo)
                .args(["worktree", "add", "-q", "--no-checkout", "-b", "late"])
                .arg(&nested)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            // Independent of the marker guard and stable logical byte count.
            fs::remove_file(nested.join(".git")).unwrap();
        }
        calls.set(calls.get() + 1);
        Ok(())
    })
    .unwrap_err();
    assert!(err.contains("registration changed"), "{err}");
    assert!(nested.is_dir());
    assert!(f.target().join("debug").join("cache.bin").exists());
}

#[cfg(unix)]
#[test]
fn symlink_cache_entry_never_touches_outside_files() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new(true);
    let external = f.root.join("outside");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("secret"), "NEVER DELETE").unwrap();
    symlink(&external, f.target().join("debug").join("link")).unwrap();
    let mut args = f.args();
    args.push("--yes".into());
    let err = f.run(&args).unwrap_err();
    assert!(err.contains("link"), "{err}");
    assert!(f.target().exists());
    assert_eq!(
        fs::read_to_string(external.join("secret")).unwrap(),
        "NEVER DELETE"
    );
}
#[cfg(target_os = "linux")]
#[test]
fn linux_mount_field_parser_retains_spaces_and_unicode_bytes() {
    use super::decode_mount_field;
    assert_eq!(
        decode_mount_field("/tmp/x\\040y").unwrap(),
        PathBuf::from("/tmp/x y")
    );
    assert!(decode_mount_field("/tmp/x\\04").is_err());
    assert!(decode_mount_field("/tmp/x\\08q").is_err());
}
#[cfg(windows)]
#[test]
fn windows_junction_cache_entry_refused_and_outside_preserved() {
    let f = Fixture::new(true);
    let external = f.root.join("external");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("secret"), "PRESERVE").unwrap();
    let junction = f.target().join("debug").join("junction");
    let out = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&external)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut args = f.args();
    args.push("--yes".into());
    let err = f.run(&args).unwrap_err();
    assert!(err.contains("link") || err.contains("reparse"), "{err}");
    assert_eq!(
        fs::read_to_string(external.join("secret")).unwrap(),
        "PRESERVE"
    );
}

#[test]
fn mount_inventory_blocks_even_same_device_bind_mounts() {
    use super::no_mount_overlap;
    let target = PathBuf::from("/cache/target");
    assert!(no_mount_overlap(&target, &[PathBuf::from("/cache/target/bind")]).is_err());
    assert!(no_mount_overlap(&target, &[PathBuf::from("/cache/targetish")]).is_ok());
}

#[test]
fn an_active_build_signal_never_deletes_an_eligible_cache() {
    use super::run_with_guard;
    let f = Fixture::new(true);
    let mut args = f.args();
    args.push("--yes".into());
    let error = run_with_guard(&f.repo, &args, || Err("active build marker".into())).unwrap_err();
    assert!(error.contains("active build marker"), "{error}");
    assert!(f.target().exists());
}
