//! Explicit local-source test preparation, never authenticated release inputs.
//!
//! The source snapshot and output receipt are created in this one execution.
//! Caller-supplied executable paths/digests cannot construct the observation.
//! This is a nonhermetic developer build, not source/dependency trust authority.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write as _};
use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use keld_runtime::windows_job::{WindowsLaunchedProcess, WindowsProcessJob};
use keld_runtime::windows_lpac::WindowsSuspendedChild;
use sha2::{Digest, Sha256};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
};

use super::BuildError;
use crate::boot;

const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const MAX_TREE_LIST: u64 = 16 * 1024 * 1024;
const MAX_SOURCE_FILE: u64 = 64 * 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_IMAGE: u64 = 256 * 1024 * 1024;

#[derive(Debug)]
struct SourceMember {
    path: String,
    object_id: String,
    executable: bool,
}

fn failure(phase: &'static str, error: impl std::fmt::Display) -> BuildError {
    BuildError::input(phase, error)
}

fn commit_id(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Parse Git's raw NUL-delimited full-tree diff; this is not an archive parser.
fn parse_members(bytes: &[u8]) -> Result<Vec<SourceMember>, BuildError> {
    if bytes.len() as u64 > MAX_TREE_LIST {
        return Err(failure(
            "local source inventory",
            "tree listing exceeds 16 MiB",
        ));
    }
    let mut fields = bytes.split(|byte| *byte == 0);
    let mut members = Vec::new();
    while let Some(header) = fields.next() {
        if header.is_empty() {
            if fields.next().is_some() {
                return Err(failure("local source inventory", "unexpected empty record"));
            }
            break;
        }
        let header = std::str::from_utf8(header)
            .map_err(|error| failure("local source inventory", error))?;
        let parts: Vec<_> = header.split(' ').collect();
        if parts.len() != 5
            || parts[0] != ":000000"
            || !matches!(parts[1], "100644" | "100755")
            || parts[2] != "0000000000000000000000000000000000000000"
            || !commit_id(parts[3])
            || parts[4] != "A"
        {
            return Err(failure(
                "local source inventory",
                "only complete added regular blobs are admitted; links and submodules refuse",
            ));
        }
        let path = fields
            .next()
            .ok_or_else(|| failure("local source inventory", "missing pathname"))?;
        let path =
            std::str::from_utf8(path).map_err(|error| failure("local source inventory", error))?;
        if path.is_empty()
            || path.len() > 4096
            || path.split('/').count() > 64
            || path.starts_with('/')
            || path
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | "..") || part.contains('\\'))
        {
            return Err(failure("local source inventory", "nonrelative source name"));
        }
        if members.len() == 50_000 {
            return Err(failure(
                "local source inventory",
                "source exceeds 50000 files",
            ));
        }
        members.push(SourceMember {
            path: path.to_owned(),
            object_id: parts[3].to_owned(),
            executable: parts[1] == "100755",
        });
    }
    if members.is_empty() || !bytes.ends_with(&[0]) {
        return Err(failure(
            "local source inventory",
            "empty or unterminated tree",
        ));
    }
    let names: Vec<_> = members.iter().map(|entry| entry.path.as_str()).collect();
    keld_guard::validate_windows_source_paths(&names)
        .map_err(|error| failure("local source inventory", error))?;
    members.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    if members.windows(2).any(|pair| pair[0].path == pair[1].path) {
        return Err(failure("local source inventory", "duplicate source path"));
    }
    Ok(members)
}

// Consumed before any blob checkout once the shared bounded stdout adapter is
// wired. It accepts only the exact requested blob sequence, not a caller budget.
fn parse_blob_sizes(bytes: &[u8], members: &[SourceMember]) -> Result<Vec<u64>, BuildError> {
    if bytes.len() > members.len().saturating_mul(80) || !bytes.ends_with(b"\n") {
        return Err(failure(
            "local source sizes",
            "size response exceeds exact query bound or is truncated",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|error| failure("local source sizes", error))?;
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != members.len() {
        return Err(failure(
            "local source sizes",
            "missing or extra size response",
        ));
    }
    let mut total = 0_u64;
    let mut sizes = Vec::with_capacity(members.len());
    for (line, member) in lines.iter().zip(members) {
        let fields: Vec<_> = line.split(' ').collect();
        if fields.len() != 3
            || fields[0] != member.object_id
            || fields[1] != "blob"
            || fields[2].is_empty()
            || !fields[2].bytes().all(|value| value.is_ascii_digit())
            || (fields[2].len() > 1 && fields[2].starts_with('0'))
        {
            return Err(failure(
                "local source sizes",
                "object identity/type/size syntax mismatch",
            ));
        }
        let size: u64 = fields[2]
            .parse()
            .map_err(|error| failure("local source sizes", error))?;
        if size > MAX_SOURCE_FILE {
            return Err(failure(
                "local source sizes",
                "blob exceeds 64 MiB before checkout",
            ));
        }
        total = total
            .checked_add(size)
            .ok_or_else(|| failure("local source sizes", "aggregate overflow"))?;
        if total > MAX_SOURCE_BYTES {
            return Err(failure(
                "local source sizes",
                "tree exceeds 2 GiB before checkout",
            ));
        }
        sizes.push(size);
    }
    Ok(sizes)
}

/// Bound every source directory and its immediate entries before extraction.
fn validate_source_layout(members: &[SourceMember]) -> Result<(), BuildError> {
    let mut directories = std::collections::BTreeSet::new();
    let mut children: std::collections::BTreeMap<&str, std::collections::BTreeSet<&str>> =
        std::collections::BTreeMap::new();
    for member in members {
        let mut parent = "";
        for end in member
            .path
            .match_indices('/')
            .map(|(end, _)| end)
            .chain(std::iter::once(member.path.len()))
        {
            let child = &member.path[..end];
            let entries = children.entry(parent).or_default();
            entries.insert(child);
            if entries.len() > 8192 {
                return Err(failure(
                    "source directories",
                    "one source directory exceeds 8192 entries",
                ));
            }
            if end != member.path.len() {
                directories.insert(child);
                if directories.len() > 50_000 {
                    return Err(failure(
                        "source directories",
                        "source exceeds 50000 directories",
                    ));
                }
            }
            parent = child;
        }
    }
    Ok(())
}

/// One transaction-wide budget, never reset by another blob/tool/cache entry.
/// Synchronous OS reads/flushes may block; checks bound work BETWEEN calls, not
/// cancellation of a blocked kernel I/O. Existing process retirement grace remains.
struct PreparationBudget {
    deadline: Instant,
}
impl PreparationBudget {
    fn new(now: Instant, limit: Duration) -> Self {
        Self {
            deadline: now + limit,
        }
    }
    fn remaining_at(&self, now: Instant, cap: Duration) -> Result<Duration, BuildError> {
        self.deadline
            .checked_duration_since(now)
            .filter(|remaining| !remaining.is_zero())
            .map(|remaining| remaining.min(cap))
            .ok_or_else(|| {
                failure(
                    "local source deadline",
                    "aggregate preparation deadline expired",
                )
            })
    }
    fn remaining(&self, cap: Duration) -> Result<Duration, BuildError> {
        self.remaining_at(Instant::now(), cap)
    }
    fn check(&self) -> Result<(), BuildError> {
        self.remaining(Duration::MAX).map(|_| ())
    }
}

/// git(1) `GIT_ALTERNATE_OBJECT_DIRECTORIES`: a path-list entry beginning with
/// double quote is C-style quoted, so `Windows` ';' remains inside ONE entry.
/// Non-Unicode/control paths refuse rather than lossy encoding or list splitting.
fn git_alternate_value(path: &Path) -> Result<OsString, BuildError> {
    let path = path
        .to_str()
        .ok_or_else(|| failure("Git alternate path", "non-Unicode path unsupported"))?;
    if path.is_empty() || path.chars().any(char::is_control) {
        return Err(failure(
            "Git alternate path",
            "empty/control path unsupported",
        ));
    }
    let mut quoted = String::from("\"");
    for ch in path.chars() {
        if matches!(ch, '\\' | '"') {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted.push('"');
    Ok(quoted.into())
}

pub(super) struct PinnedFile {
    pub(super) path: PathBuf,
    pub(super) file: File,
    pub(super) sha256: [u8; 32],
}

pub(super) fn pin_file(path: &Path, limit: u64) -> Result<PinnedFile, BuildError> {
    pin_file_with_budget(path, limit, None)
}
fn pin_file_with_budget(
    path: &Path,
    limit: u64,
    budget: Option<&PreparationBudget>,
) -> Result<PinnedFile, BuildError> {
    if let Some(budget) = budget {
        budget.check()?;
    }
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|error| failure("local source capture", error))?;
    let metadata = file
        .metadata()
        .map_err(|error| failure("local source capture", error))?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || metadata.len() > limit
    {
        return Err(failure(
            "local source capture",
            "nonregular, reparse or oversized input",
        ));
    }
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut count = 0_u64;
    loop {
        if let Some(budget) = budget {
            budget.check()?;
        }
        let read = file
            .read(&mut buffer)
            .map_err(|error| failure("local source capture", error))?;
        if let Some(budget) = budget {
            budget.check()?;
        }
        if read == 0 {
            break;
        }
        count = count
            .checked_add(read as u64)
            .ok_or_else(|| failure("local source capture", "length overflow"))?;
        if count > limit {
            return Err(failure("local source capture", "input grew beyond bound"));
        }
        digest.update(&buffer[..read]);
    }
    if count != metadata.len() {
        return Err(failure("local source capture", "input length changed"));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| failure("local source capture", error))?;
    Ok(PinnedFile {
        path: path.to_owned(),
        file,
        sha256: digest.finalize().into(),
    })
}

// Retain one independently owned exact Job handle during the native test.
// KILL_ON_JOB_CLOSE must not mask a removed explicit terminate-and-wait.
#[cfg(test)]
thread_local! {
    static OWNED_JOB_TEST_TAP:
        std::cell::RefCell<Option<std::sync::mpsc::Sender<std::os::windows::io::OwnedHandle>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn set_owned_job_test_tap(sender: std::sync::mpsc::Sender<std::os::windows::io::OwnedHandle>) {
    OWNED_JOB_TEST_TAP.with(|tap| {
        assert!(
            tap.borrow().is_none(),
            "one observer per owned fixture thread"
        );
        *tap.borrow_mut() = Some(sender);
    });
}

fn run_owned(
    tool: &PinnedFile,
    args: &[OsString],
    environment: &[(OsString, OsString)],
    cwd: &Path,
    deadline: Duration,
) -> Result<(), BuildError> {
    // The caller retains the borrowed PinnedFile, including its File, throughout this call.
    let milliseconds = u32::try_from(deadline.as_millis())
        .map_err(|error| failure("local source deadline", error))?;
    let mut job =
        WindowsProcessJob::create().map_err(|error| failure("local source Job", error))?;
    let child = {
        use keld_runtime::windows_lpac::WindowsLpacStdio;
        use std::os::windows::io::AsHandle as _;
        // Ordinary command-line tools need valid standard handles even when
        // their output is intentionally discarded. Do not inherit our console
        // or alter the default candidate's NULL/no-inheritance contract.
        let input = File::open("NUL").map_err(|error| failure("local source tool stdin", error))?;
        let output = OpenOptions::new()
            .write(true)
            .open("NUL")
            .map_err(|error| failure("local source tool output", error))?;
        WindowsSuspendedChild::spawn_same_token_with_stdio(
            &tool.path,
            args,
            environment,
            cwd,
            WindowsLpacStdio {
                stdin: input.as_handle(),
                stdout: output.as_handle(),
                stderr: output.as_handle(),
            },
        )
    }
    .map_err(|error| failure("local source tool spawn", error))?;
    // record failure occurs before resume and the existing suspended-child owner
    // retires it. Do not claim observed reap for that owner error branch.
    let mut process = WindowsLaunchedProcess::record(child)
        .map_err(|error| failure("local source process identity", error))?;
    let membership = match job.assign_child(process.child()) {
        Ok(membership) => membership,
        Err(error) => {
            // The child has never been resumed, so it cannot have descendants.
            process.terminate(1).map_err(|cleanup| {
                failure(
                    "local source suspended-child termination",
                    format!("{error}; {cleanup}"),
                )
            })?;
            process.wait(10_000).map_err(|cleanup| {
                failure(
                    "local source suspended-child reap",
                    format!("{error}; {cleanup}"),
                )
            })?;
            return Err(failure("local source Job assignment", error));
        }
    };
    #[cfg(test)]
    OWNED_JOB_TEST_TAP.with(|tap| -> Result<(), BuildError> {
        if let Some(sender) = tap.borrow().as_ref() {
            let witness = job
                .duplicate_cleanup_observer_handle()
                .map_err(|error| failure("source test Job witness", error))?;
            sender
                .send(witness)
                .map_err(|_| failure("source test Job witness", "observer disconnected"))?;
        }
        Ok(())
    })?;
    if let Err(error) = process.resume(&membership) {
        job.terminate_and_wait(process.child(), Duration::from_secs(10))
            .map_err(|cleanup| {
                failure(
                    "local source failed-resume retirement",
                    format!("{error}; {cleanup}"),
                )
            })?;
        return Err(failure("local source resume", error));
    }
    let status = process.wait(milliseconds);
    // Reap enrolled descendants even when the direct tool exits zero.
    job.terminate_and_wait(process.child(), Duration::from_secs(10))
        .map_err(|error| failure("local source family retirement", error))?;
    let status = status.map_err(|error| failure("local source tool wait", error))?;
    if status != 0 {
        return Err(failure(
            "local source tool",
            format!("exit status {status}; no preparation receipt"),
        ));
    }
    Ok(())
}

fn join_source_reader(
    spawned: std::io::Result<std::thread::JoinHandle<std::io::Result<Vec<u8>>>>,
    thread_phase: &'static str,
    reader_phase: &'static str,
) -> Result<Vec<u8>, BuildError> {
    spawned
        .map_err(|error| failure(thread_phase, error))
        .and_then(|thread| {
            thread
                .join()
                .map_err(|_| failure(reader_phase, "reader panicked"))
        })
        .and_then(|bytes| bytes.map_err(|error| failure(reader_phase, error)))
}

fn capture_source_pipe(
    reader: std::io::PipeReader,
    ceiling: usize,
    deadline: std::time::Instant,
    failed: &std::sync::atomic::AtomicBool,
) -> std::io::Result<Vec<u8>> {
    let result = std::panic::catch_unwind(|| {
        keld_runtime::read_windows_tool_pipe(reader, ceiling, deadline)
    })
    .unwrap_or_else(|_| Err(std::io::Error::other("capture reader panicked")));
    if result.is_err() {
        failed.store(true, std::sync::atomic::Ordering::Release);
    }
    result
}

struct CaptureOptions<'a> {
    stdout_limit: usize,
    stderr_limit: usize,
    timeout: Duration,
    stderr_file: Option<&'a mut File>,
}

fn retain_source_stderr(file: Option<&mut File>, bytes: &[u8]) -> Result<(), BuildError> {
    if let Some(file) = file {
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| failure("local source private diagnostic", error))?;
    }
    Ok(())
}

/// One observed tool execution with bounded binary stdout. No release authority.
pub(super) fn run_captured(
    tool: &PinnedFile,
    args: &[OsString],
    environment: &[(OsString, OsString)],
    cwd: &Path,
    input: &File,
    limit: usize,
) -> Result<Vec<u8>, BuildError> {
    run_captured_with_options(
        tool,
        args,
        environment,
        cwd,
        input,
        CaptureOptions {
            stdout_limit: limit,
            stderr_limit: 64 * 1024,
            timeout: Duration::from_secs(120),
            stderr_file: None,
        },
    )
}

fn run_captured_budget(
    tool: &PinnedFile,
    args: &[OsString],
    environment: &[(OsString, OsString)],
    cwd: &Path,
    input: &File,
    limit: usize,
    budget: &PreparationBudget,
) -> Result<Vec<u8>, BuildError> {
    let result = run_captured_with_options(
        tool,
        args,
        environment,
        cwd,
        input,
        CaptureOptions {
            stdout_limit: limit,
            stderr_limit: 64 * 1024,
            timeout: budget.remaining(Duration::from_secs(120))?,
            stderr_file: None,
        },
    )?;
    budget.check()?;
    Ok(result)
}

fn run_captured_with_options(
    tool: &PinnedFile,
    args: &[OsString],
    environment: &[(OsString, OsString)],
    cwd: &Path,
    input: &File,
    options: CaptureOptions<'_>,
) -> Result<Vec<u8>, BuildError> {
    use keld_runtime::windows_lpac::WindowsLpacStdio;
    use std::os::windows::io::AsHandle as _;
    let deadline = std::time::Instant::now() + options.timeout;
    let limit = options.stdout_limit;
    let stderr_limit = options.stderr_limit;
    let mut job =
        WindowsProcessJob::create().map_err(|error| failure("source capture Job", error))?;
    let (stdout, out_writer) = std::io::pipe().map_err(|error| failure("source stdout", error))?;
    let (stderr, err_writer) = std::io::pipe().map_err(|error| failure("source stderr", error))?;
    let child = WindowsSuspendedChild::spawn_same_token_with_stdio(
        &tool.path,
        args,
        environment,
        cwd,
        WindowsLpacStdio {
            stdin: input.as_handle(),
            stdout: out_writer.as_handle(),
            stderr: err_writer.as_handle(),
        },
    )
    .map_err(|error| failure("source capture spawn", error))?;
    drop(out_writer);
    drop(err_writer);
    let mut process = WindowsLaunchedProcess::record(child)
        .map_err(|error| failure("source capture record", error))?;
    let membership = match job.assign_child(process.child()) {
        Ok(membership) => membership,
        Err(error) => {
            process.terminate(1).map_err(|cleanup| {
                failure("source capture termination", format!("{error}; {cleanup}"))
            })?;
            process
                .wait(10_000)
                .map_err(|cleanup| failure("source capture reap", format!("{error}; {cleanup}")))?;
            return Err(failure("source capture assignment", error));
        }
    };
    let capture_failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let out_failed = std::sync::Arc::clone(&capture_failed);
    let err_failed = std::sync::Arc::clone(&capture_failed);

    let out = std::thread::Builder::new()
        .name("keld-source-stdout".into())
        .spawn(move || capture_source_pipe(stdout, limit, deadline, &out_failed));
    let err = std::thread::Builder::new()
        .name("keld-source-stderr".into())
        .spawn(move || capture_source_pipe(stderr, stderr_limit, deadline, &err_failed));
    let status = if out.is_ok() && err.is_ok() {
        process
            .resume(&membership)
            .map_err(|error| failure("source capture resume", error))
            .and_then(|()| {
                loop {
                    if capture_failed.load(std::sync::atomic::Ordering::Acquire) {
                        break Err(failure("source capture", "bounded reader refused output"));
                    }
                    let now = std::time::Instant::now();
                    if now >= deadline {
                        break Err(failure("source capture", "tool deadline"));
                    }
                    if process
                        .has_exited()
                        .map_err(|error| failure("source capture state", error))?
                    {
                        break process
                            .wait(0)
                            .map_err(|error| failure("source capture wait", error));
                    }
                    std::thread::sleep(
                        Duration::from_millis(10).min(deadline.saturating_duration_since(now)),
                    );
                }
            })
    } else {
        Err(failure(
            "source capture threads",
            "capture reader creation failed",
        ))
    };
    // Always retire the assigned family before accepting bytes or joining readers.
    let retired = job
        .terminate_and_wait(process.child(), Duration::from_secs(10))
        .map_err(|error| failure("source capture family reap", error));
    let stdout = join_source_reader(out, "source stdout thread", "source stdout");
    let stderr = join_source_reader(err, "source stderr thread", "source stderr");
    retired?;
    // Preserve the actual reader refusal before the generic cancellation/status
    // result. Otherwise an overflow-triggered child exit can hide its cause.
    let stdout = stdout?;
    // Do not echo captured diagnostics: source/tool output may contain private data.
    retain_source_stderr(options.stderr_file, &stderr?)?;
    let status = status?;
    if status != 0 {
        return Err(failure(
            "source capture status",
            format!("exit status {status}"),
        ));
    }
    Ok(stdout)
}

/// Observed source/tool/output identities from one nonhermetic developer build.
/// This private value cannot be deserialized or converted to release/Strict authority.
pub(super) struct LocalSourceBuildObservation {
    source_root: PathBuf,
    pub(super) source_commit: String,
    pub(super) source_sha256: [u8; 32],
    cargo_lock_sha256: [u8; 32],
    pub(super) cache_sha256: Option<[u8; 32]>,
    pub(super) recipe_sha256: [u8; 32],
    tool_sha256: [[u8; 32]; 3],
    source_pins: Vec<PinnedFile>,
    _directory_pins: Vec<File>,
    pub(super) host: PinnedFile,
    pub(super) helper: PinnedFile,
}

/// Borrowing this value keeps the existing exact source-tree file/directory pins
/// alive. Local source observation is not production-release authentication.
pub(super) struct ObservedCompilerSources<'a> {
    owner: &'a LocalSourceBuildObservation,
    entry: &'a PinnedFile,
}
impl LocalSourceBuildObservation {
    pub(super) fn compiler_sources(&self) -> Result<ObservedCompilerSources<'_>, BuildError> {
        for relative in [
            "crates/keld-cli/tools/closed-app-runner.ts",
            "crates/keld-cli/tools/closed-app.ts",
            "crates/keld-cli/tools/protocol.ts",
            "crates/keld-cli/tools/static-renderer.ts",
            "packages/@keld/kipc/scripts/package-artifacts.ts",
            "packages/@keld/kipc/src/transport.ts",
        ] {
            let path = self.source_root.join(relative);
            if !self.source_pins.iter().any(|pin| pin.path == path) {
                return Err(failure(
                    "observed compiler closure",
                    "required source is absent from observed pinned tree",
                ));
            }
        }
        let entry_path = self
            .source_root
            .join("crates/keld-cli/tools/closed-app-runner.ts");
        let entry = self
            .source_pins
            .iter()
            .find(|pin| pin.path == entry_path)
            .ok_or_else(|| {
                failure(
                    "observed compiler closure",
                    "required entry is absent from observed pinned tree",
                )
            })?;
        Ok(ObservedCompilerSources { owner: self, entry })
    }
}
impl ObservedCompilerSources<'_> {
    /// Same pinned compiler-source closure, not an ambient canonical path.
    pub(super) fn canonical_transport(&self) -> Result<&PinnedFile, BuildError> {
        let path = self
            .owner
            .source_root
            .join("packages/@keld/kipc/src/transport.ts");
        self.owner
            .source_pins
            .iter()
            .find(|pin| pin.path == path)
            .ok_or_else(|| {
                failure(
                    "observed canonical transport",
                    "required retained source absent",
                )
            })
    }

    pub(super) fn kipc_root(&self) -> PathBuf {
        self.owner.source_root.join("packages/@keld/kipc")
    }
    pub(super) fn entry(&self) -> &PinnedFile {
        self.entry
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CacheMode {
    Snapshot,
    MutableDeveloper,
}

fn cache_arguments(args: &[String]) -> Result<(&[String], CacheMode), BuildError> {
    if args.len() == 7 {
        return Ok((args, CacheMode::Snapshot));
    }
    if args.len() == 8 && args[7] == "--use-developer-cache" {
        return Ok((&args[..7], CacheMode::MutableDeveloper));
    }
    Err(BuildError::invocation(
        "expected seven source-preparation inputs, optionally followed by --use-developer-cache",
    ))
}

/// Metadata-only admission, not an immutable cache or secret-free guarantee.
/// The supplied existing Cargo home stays mutable during this explicit mode.
fn validate_developer_cache(root: &Path) -> Result<(), BuildError> {
    let _guards = pin_directories(root)?;
    boot::ensure_current_principal_owns(root)
        .map_err(|error| failure("developer cache owner", error))?;
    for name in ["config", "config.toml", "credentials", "credentials.toml"] {
        match fs::symlink_metadata(root.join(name)) {
            Ok(_) => {
                return Err(failure(
                    "developer cache configuration",
                    "selected Cargo home contains root configuration or credentials; no contents were read",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(failure("developer cache configuration", error)),
        }
    }
    Ok(())
}

/// Cargo walks from its manifest directory towards the volume root and can
/// load .cargo/config(.toml) outside the materialized, measured source tree.
/// Refuse any existing ancestor configuration before invoking Cargo, without
/// opening its potentially sensitive contents. This removes one ambient input;
/// compiler/linker/build-script inputs still prevent hermetic release claims.
fn validate_cargo_ancestor_configuration(source: &Path) -> Result<(), BuildError> {
    let parent = source
        .parent()
        .ok_or_else(|| failure("ambient Cargo configuration", "source parent missing"))?;
    for ancestor in parent.ancestors() {
        for name in ["config", "config.toml"] {
            let path = ancestor.join(".cargo").join(name);
            match fs::symlink_metadata(&path) {
                Ok(_) => {
                    return Err(failure(
                        "ambient Cargo configuration",
                        "unmeasured ancestor .cargo/config detected; isolated compiler refused",
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(failure("ambient Cargo configuration", error)),
            }
        }
    }
    Ok(())
}

pub(super) struct Request {
    pub(super) source: PathBuf,
    pub(super) commit: String,
    pub(super) session: PathBuf,
    pub(super) git: PathBuf,
    pub(super) cargo: PathBuf,
    pub(super) rustc: PathBuf,
    pub(super) cargo_home: PathBuf,
    pub(super) cache_mode: CacheMode,
}

fn pin_directories(path: &Path) -> Result<Vec<File>, BuildError> {
    super::windows_inputs::pin_input_root(path)
}

fn create_private(path: &Path) -> Result<(), BuildError> {
    boot::create_windows_stage_root(path).map_err(|error| failure("local source session", error))
}

fn os_args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

/// Prepare only the fixed recipe's target root and triple root.
/// Cargo's atomic root initializer renames a temporary sibling; that rename
/// conflicts with the immutable parent guard. Preexistence takes its documented
/// early-return path instead. Output descendants remain mutable/untrusted until
/// the existing post-retirement ancestor and output-file admission.
fn create_cargo_target_root(target: &Path) -> Result<Vec<File>, BuildError> {
    create_private(target)?;
    create_private(&target.join("x86_64-pc-windows-msvc"))?;
    pin_directories(target)
}

fn collect_pins(
    root: &Path,
    files: &mut Vec<PinnedFile>,
    directories: &mut Vec<File>,
    total: &mut u64,
    budget: &PreparationBudget,
) -> Result<(), BuildError> {
    collect_pins_at(root, files, directories, total, 0, budget)
}

fn collect_pins_at(
    root: &Path,
    files: &mut Vec<PinnedFile>,
    directories: &mut Vec<File>,
    total: &mut u64,
    depth: usize,
    budget: &PreparationBudget,
) -> Result<(), BuildError> {
    budget.check()?;
    if depth > 64 || directories.len() >= 50_000 {
        return Err(failure(
            "local source snapshot",
            "directory depth/count exceeds support limit",
        ));
    }
    directories.push(
        boot::open_windows_input_directory_guard(root)
            .map_err(|error| failure("local source snapshot", error))?,
    );
    let mut entries = Vec::new();
    for entry in fs::read_dir(root).map_err(|error| failure("local source snapshot", error))? {
        budget.check()?;
        if entries.len() >= 8192 {
            return Err(failure(
                "local source snapshot",
                "one directory exceeds 8192 entries",
            ));
        }
        entries.push(entry.map_err(|error| failure("local source snapshot", error))?);
    }
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        budget.check()?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| failure("local source snapshot", error))?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(failure("local source snapshot", "reparse object refused"));
        }
        if metadata.is_dir() {
            collect_pins_at(&entry.path(), files, directories, total, depth + 1, budget)?;
        } else {
            if files.len() >= 100_000 {
                return Err(failure(
                    "local source snapshot",
                    "snapshot file count exceeds bound",
                ));
            }
            let pinned = pin_file_with_budget(&entry.path(), MAX_SOURCE_FILE, Some(budget))?;
            *total = total
                .checked_add(metadata.len())
                .ok_or_else(|| failure("local source snapshot", "size overflow"))?;
            if *total > MAX_SOURCE_BYTES {
                return Err(failure("local source snapshot", "snapshot exceeds 2 GiB"));
            }
            files.push(pinned);
        }
    }
    budget.check()?;
    Ok(())
}

fn copy_cache(
    source: &Path,
    destination: &Path,
    total: &mut u64,
    objects: &mut usize,
    budget: &PreparationBudget,
) -> Result<(), BuildError> {
    copy_cache_at(source, destination, total, objects, 0, budget)
}

fn copy_cache_at(
    source: &Path,
    destination: &Path,
    total: &mut u64,
    objects: &mut usize,
    depth: usize,
    budget: &PreparationBudget,
) -> Result<(), BuildError> {
    budget.check()?;
    if depth > 64 {
        return Err(failure(
            "offline cache snapshot",
            "cache depth exceeds support limit",
        ));
    }
    *objects = objects
        .checked_add(1)
        .ok_or_else(|| failure("offline cache snapshot", "object count overflow"))?;
    if *objects > 100_000 {
        return Err(failure(
            "offline cache snapshot",
            "cache exceeds object limit",
        ));
    }
    let _guard = pin_directories(source)?;
    create_private(destination)?;
    let _destination_guard = pin_directories(destination)?;
    for entry in fs::read_dir(source).map_err(|error| failure("offline cache snapshot", error))? {
        budget.check()?;
        let entry = entry.map_err(|error| failure("offline cache snapshot", error))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| failure("offline cache snapshot", error))?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(failure("offline cache snapshot", "reparse cache object"));
        }
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_cache_at(&entry.path(), &target, total, objects, depth + 1, budget)?;
        } else {
            *objects = objects
                .checked_add(1)
                .ok_or_else(|| failure("offline cache snapshot", "object count overflow"))?;
            if *objects > 100_000 {
                return Err(failure(
                    "offline cache snapshot",
                    "cache exceeds object limit",
                ));
            }
            let mut input = pin_file_with_budget(&entry.path(), MAX_SOURCE_FILE, Some(budget))?;
            let input_len = input
                .file
                .metadata()
                .map_err(|error| failure("offline cache snapshot", error))?
                .len();
            *total = total
                .checked_add(input_len)
                .ok_or_else(|| failure("offline cache snapshot", "size overflow"))?;
            if *total > MAX_SOURCE_BYTES {
                return Err(failure(
                    "offline cache snapshot",
                    "cache exceeds 2 GiB support limit",
                ));
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|error| failure("offline cache snapshot", error))?;
            let mut count = 0_u64;
            let mut buffer = vec![0_u8; 64 * 1024];
            loop {
                budget.check()?;
                let read = input
                    .file
                    .read(&mut buffer)
                    .map_err(|error| failure("offline cache snapshot", error))?;
                budget.check()?;
                if read == 0 {
                    break;
                }
                count = count
                    .checked_add(read as u64)
                    .ok_or_else(|| failure("offline cache snapshot", "copy size overflow"))?;
                if count > input_len {
                    return Err(failure(
                        "offline cache snapshot",
                        "copy grew beyond admitted length",
                    ));
                }
                output
                    .write_all(&buffer[..read])
                    .map_err(|error| failure("offline cache snapshot", error))?;
            }
            if count != input_len {
                return Err(failure("offline cache snapshot", "copy length changed"));
            }
            budget.check()?;
            output
                .sync_all()
                .map_err(|error| failure("offline cache snapshot", error))?;
        }
    }
    budget.check()?;
    Ok(())
}

#[allow(clippy::too_many_lines)] // one source/build transaction retains every input and output pin through completion
pub(super) fn prepare(
    request: Request,
    initial_install: bool,
) -> Result<LocalSourceBuildObservation, BuildError> {
    let budget = PreparationBudget::new(Instant::now(), Duration::from_mins(30));
    if !commit_id(&request.commit) {
        return Err(failure(
            "local source commit",
            "exact lowercase SHA1 commit required",
        ));
    }
    if request.cache_mode == CacheMode::MutableDeveloper {
        validate_developer_cache(&request.cargo_home)?;
    }
    let mut directory_pins = pin_directories(&request.source)?;
    // The first source cell intentionally refuses linked worktrees/bare repos.
    // It never parses or follows a caller-controlled gitdir indirection file.
    directory_pins.extend(pin_directories(&request.source.join(".git"))?);
    directory_pins.extend(pin_directories(&request.source.join(".git/objects"))?);
    let parent = request
        .session
        .parent()
        .ok_or_else(|| failure("local source session", "session parent missing"))?;
    directory_pins.extend(pin_directories(parent)?);
    boot::ensure_current_principal_owns(parent)
        .map_err(|error| failure("local source session owner", error))?;
    for tool in [&request.git, &request.cargo, &request.rustc] {
        budget.check()?;
        let parent = tool
            .parent()
            .ok_or_else(|| failure("local source tool", "tool parent missing"))?;
        directory_pins.extend(pin_directories(parent)?);
    }
    let git = pin_file_with_budget(&request.git, MAX_IMAGE, Some(&budget))?;
    let cargo = pin_file_with_budget(&request.cargo, MAX_IMAGE, Some(&budget))?;
    let rustc = pin_file_with_budget(&request.rustc, MAX_IMAGE, Some(&budget))?;
    create_private(&request.session)?;
    let gitdir = request.session.join("git");
    let source = request.session.join("source");
    let target = request.session.join("target");
    let home = request.session.join("cargo-home");
    for directory in [&source, &home] {
        budget.check()?;
        create_private(directory)?;
        directory_pins.extend(pin_directories(directory)?);
    }
    directory_pins.extend(create_cargo_target_root(&target)?);
    let mut environment = Vec::new();
    // This is an explicit local developer toolchain, not a hermetic release.
    // Preserve only required MSVC/Windows lookup variables, never credentials,
    // arbitrary Cargo/rustc wrappers, Git config injection or proxy variables.
    for name in [
        "SystemRoot",
        "WINDIR",
        "PATH",
        "INCLUDE",
        "LIB",
        "LIBPATH",
        "VCINSTALLDIR",
        "VCToolsInstallDir",
        "WindowsSdkDir",
        "WindowsSDKVersion",
        "UCRTVersion",
    ] {
        if let Some(value) = std::env::var_os(name) {
            environment.push((name.into(), value));
        }
    }
    environment.extend([
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ("GIT_CONFIG_GLOBAL".into(), "NUL".into()),
        ("GIT_CONFIG_SYSTEM".into(), "NUL".into()),
        ("GIT_TERMINAL_PROMPT".into(), "0".into()),
        ("TEMP".into(), request.session.as_os_str().to_owned()),
        ("TMP".into(), request.session.as_os_str().to_owned()),
    ]);
    let template = request.session.join("empty-git-template");
    create_private(&template)?;
    let mut init = os_args(&["-c", "core.hooksPath=NUL", "init", "--bare", "--quiet"]);
    let maintenance = os_args(&["config", "maintenance.auto", "false"]);
    init.push(format!("--template={}", template.display()).into());
    init.push(gitdir.as_os_str().to_owned());
    run_owned(
        &git,
        &init,
        &environment,
        &request.session,
        budget.remaining(Duration::from_secs(120))?,
    )?;
    budget.check()?;
    environment.extend([
        ("GIT_DIR".into(), gitdir.as_os_str().to_owned()),
        ("GIT_WORK_TREE".into(), source.as_os_str().to_owned()),
        (
            "GIT_ALTERNATE_OBJECT_DIRECTORIES".into(),
            git_alternate_value(&request.source.join(".git/objects"))?,
        ),
    ]);
    run_owned(
        &git,
        &maintenance,
        &environment,
        &request.session,
        budget.remaining(Duration::from_secs(120))?,
    )?;
    let empty = request.session.join("empty-tree");
    File::create_new(&empty).map_err(|error| failure("local source empty tree", error))?;
    let null_input = File::open("NUL").map_err(|error| failure("source null input", error))?;
    let mut hash = os_args(&["hash-object", "-t", "tree", "-w"]);
    hash.push(empty.into_os_string());
    let empty_id = run_captured_budget(
        &git,
        &hash,
        &environment,
        &request.session,
        &null_input,
        42,
        &budget,
    )?;
    if empty_id != format!("{EMPTY_TREE}\n").as_bytes()
        && empty_id != format!("{EMPTY_TREE}\r\n").as_bytes()
    {
        return Err(failure(
            "source empty tree",
            "Git returned an unexpected empty tree identity",
        ));
    }
    run_owned(
        &git,
        &os_args(&["cat-file", "-e", &format!("{}^{{commit}}", request.commit)]),
        &environment,
        &request.session,
        budget.remaining(Duration::from_secs(120))?,
    )?;
    budget.check()?;
    let mut diff = os_args(&[
        "diff-tree",
        "--no-commit-id",
        "--raw",
        "--no-abbrev",
        "-z",
        "-r",
        "--no-renames",
        "--no-ext-diff",
    ]);
    diff.extend(os_args(&[EMPTY_TREE, &request.commit]));
    let listing = run_captured_budget(
        &git,
        &diff,
        &environment,
        &request.session,
        &null_input,
        usize::try_from(MAX_TREE_LIST)
            .map_err(|error| failure("local source inventory bound", error))?,
        &budget,
    )?;
    let members = parse_members(&listing)?;
    validate_source_layout(&members)?;
    let query = request.session.join("blob-size-input");
    {
        let mut file = File::create_new(&query).map_err(|error| failure("blob query", error))?;
        for member in &members {
            budget.check()?;
            writeln!(file, "{}", member.object_id).map_err(|error| failure("blob query", error))?;
        }
    }
    let query = pin_file_with_budget(&query, MAX_TREE_LIST, Some(&budget))?;
    let sizes = run_captured_budget(
        &git,
        &os_args(&[
            "cat-file",
            "--batch-check=%(objectname) %(objecttype) %(objectsize)",
        ]),
        &environment,
        &request.session,
        &query.file,
        members.len().saturating_mul(80),
        &budget,
    )?;
    let sizes = parse_blob_sizes(&sizes, &members)?;
    // All individual/aggregate blob sizes are refused before ANY source file
    // materialization. Raw cat-file output avoids checkout filters/attributes.
    let mut raw_source_pins = Vec::new();
    let mut created_directories = std::collections::HashSet::new();
    created_directories.insert(source.clone());
    for (member, size) in members.iter().zip(sizes) {
        budget.check()?;
        let bytes = run_captured_budget(
            &git,
            &os_args(&["cat-file", "blob", &member.object_id]),
            &environment,
            &request.session,
            &null_input,
            usize::try_from(size).map_err(|error| failure("source blob size", error))?,
            &budget,
        )?;
        if bytes.len() as u64 != size {
            return Err(failure(
                "source raw blob",
                "blob length differs from admitted size",
            ));
        }
        let path = source.join(&member.path);
        let parent = path
            .parent()
            .ok_or_else(|| failure("source blob", "parent absent"))?;
        let relative_parent = parent
            .strip_prefix(&source)
            .map_err(|error| failure("source directories", error))?;
        let mut directory = source.clone();
        for component in relative_parent.components() {
            budget.check()?;
            directory.push(component);
            if created_directories.insert(directory.clone()) {
                // Never accept an unexplained preexisting child, follow a
                // junction, or write below an unpinned new directory.
                fs::create_dir(&directory).map_err(|error| failure("source directories", error))?;
                directory_pins.push(
                    boot::open_windows_input_directory_guard(&directory)
                        .map_err(|error| failure("source directory pin", error))?,
                );
            }
        }
        let mut file = File::create_new(&path).map_err(|error| failure("source blob", error))?;
        file.write_all(&bytes)
            .map_err(|error| failure("source blob", error))?;
        file.sync_all()
            .map_err(|error| failure("source blob", error))?;
        drop(file);
        let pinned = pin_file_with_budget(&path, size, Some(&budget))?;
        let expected: [u8; 32] = Sha256::digest(&bytes).into();
        if pinned.sha256 != expected {
            return Err(failure(
                "source raw blob",
                "retained snapshot differs from captured bytes",
            ));
        }
        raw_source_pins.push(pinned);
    }
    run_owned(
        &git,
        &os_args(&["update-ref", "HEAD", &request.commit]),
        &environment,
        &request.session,
        budget.remaining(Duration::from_secs(120))?,
    )?;
    budget.check()?;
    let mut source_pins = Vec::new();
    let mut total = 0;
    collect_pins(
        &source,
        &mut source_pins,
        &mut directory_pins,
        &mut total,
        &budget,
    )?;
    if source_pins.len() != members.len() {
        return Err(failure(
            "local source snapshot",
            "checkout differs from exact tree inventory",
        ));
    }
    let mut source_digest = Sha256::new();
    source_digest.update(b"keld.local-source-test/v1\0");
    for member in &members {
        budget.check()?;
        let path = source.join(&member.path);
        let pinned = source_pins
            .iter()
            .find(|file| file.path == path)
            .ok_or_else(|| failure("local source snapshot", "missing tracked source"))?;
        source_digest.update((member.path.len() as u64).to_le_bytes());
        source_digest.update(member.path.as_bytes());
        source_digest.update(member.object_id.as_bytes());
        source_digest.update([u8::from(member.executable)]);
        source_digest.update(pinned.sha256);
    }
    let cargo_lock = source_pins
        .iter()
        .find(|file| file.path == source.join("Cargo.lock"))
        .ok_or_else(|| failure("local source lock", "tracked Cargo.lock required"))?
        .sha256;
    let cache_sha256 = if request.cache_mode == CacheMode::Snapshot {
        // Select only registry directories named index.crates.io-*. Names do not
        // prove public origin or secret-free contents. Root credentials/config,
        // other registry names, git caches and tool binaries are not copied.
        // Source/ancestor Cargo config and build scripts may still select ambient
        // inputs or credential helpers: this is a nonhermetic developer execution.
        let registry = home.join("registry");
        create_private(&registry)?;
        let mut copied_cache_bytes = 0;
        let mut copied_cache_objects = 0;
        for part in ["index", "cache", "src"] {
            budget.check()?;
            let destination = registry.join(part);
            create_private(&destination)?;
            let input = request.cargo_home.join("registry").join(part);
            let _cache_guard = pin_directories(&input)?;
            for entry in
                fs::read_dir(&input).map_err(|error| failure("offline cache snapshot", error))?
            {
                budget.check()?;
                let entry = entry.map_err(|error| failure("offline cache snapshot", error))?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("index.crates.io-")
                {
                    copy_cache(
                        &entry.path(),
                        &destination.join(entry.file_name()),
                        &mut copied_cache_bytes,
                        &mut copied_cache_objects,
                        &budget,
                    )?;
                }
            }
        }
        let mut cache_pins = Vec::new();
        collect_pins(
            &registry,
            &mut cache_pins,
            &mut directory_pins,
            &mut total,
            &budget,
        )?;
        let mut cache_digest = Sha256::new();
        cache_digest.update(b"keld.local-source-cache/v1\0");
        for file in &cache_pins {
            budget.check()?;
            let relative = file
                .path
                .strip_prefix(&registry)
                .map_err(|error| failure("offline cache identity", error))?;
            let name = relative
                .to_str()
                .ok_or_else(|| failure("offline cache identity", "non-Unicode cache name"))?;
            cache_digest.update((name.len() as u64).to_le_bytes());
            cache_digest.update(name.as_bytes());
            cache_digest.update(file.sha256);
        }
        source_pins.extend(cache_pins);
        Some(cache_digest.finalize().into())
    } else {
        // No copying, hashing, or dependency-cache pinning in this explicit mode.
        // Cargo can use its existing offline registry and Git caches normally.
        None
    };
    let selected_cargo_home = if request.cache_mode == CacheMode::MutableDeveloper {
        request.cargo_home.as_os_str().to_owned()
    } else {
        home.into_os_string()
    };
    environment.extend([
        ("CARGO_HOME".into(), selected_cargo_home),
        ("CARGO_INCREMENTAL".into(), "0".into()),
        ("CARGO_NET_OFFLINE".into(), "true".into()),
        ("RUSTC".into(), rustc.path.as_os_str().to_owned()),
    ]);
    let mut args = os_args(&[
        "build",
        "--frozen",
        "--offline",
        "--release",
        "--target",
        "x86_64-pc-windows-msvc",
        "-p",
        "keld-host",
        "-p",
        "keld-updater-helper",
        "--manifest-path",
    ]);
    args.push(source.join("Cargo.toml").into_os_string());
    args.push("--target-dir".into());
    args.push(target.as_os_str().to_owned());
    append_initial_install_feature(&mut args, initial_install);
    let mut recipe_digest = Sha256::new();
    recipe_digest.update(
        b"keld.local-source-recipe/v5\0aggregate-prepare-30min-cooperative-io\0quoted-git-alternate\0cargo-capture-1MiB-per-stream-30min-private-stderr\0precreated-fixed-msvc-target-root\0",
    );
    recipe_digest.update(match request.cache_mode {
        CacheMode::Snapshot => b"immutable-registry-snapshot".as_slice(),
        CacheMode::MutableDeveloper => b"mutable-developer-cache-unmeasured".as_slice(),
    });
    for word in args
        .iter()
        .chain(environment.iter().flat_map(|(name, value)| [name, value]))
    {
        use std::os::windows::ffi::OsStrExt as _;
        let wide: Vec<_> = word.encode_wide().collect();
        recipe_digest.update((wide.len() as u64).to_le_bytes());
        for unit in wide {
            budget.check()?;
            recipe_digest.update(unit.to_le_bytes());
        }
    }
    // Cargo would otherwise search unmeasured .cargo/config(.toml) in every
    // ancestor of the private source checkout. Refuse before starting Cargo.
    validate_cargo_ancestor_configuration(&source)?;
    // Retained source/tool pins remain alive. Never print arbitrary compiler diagnostics.
    // create_new refuses replacement; the enclosing session has retained private ancestry.
    let mut diagnostic = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .open(request.session.join("cargo-stderr.log"))
        .map_err(|error| failure("local source private diagnostic", error))?;
    run_captured_with_options(
        &cargo,
        &args,
        &environment,
        &source,
        &null_input,
        CaptureOptions {
            stdout_limit: 1024 * 1024,
            stderr_limit: 1024 * 1024,
            timeout: budget.remaining(Duration::from_mins(30))?,
            stderr_file: Some(&mut diagnostic),
        },
    )?;
    let release = target.join("x86_64-pc-windows-msvc/release");
    directory_pins.extend(pin_directories(&release)?);
    let host = pin_file_with_budget(&release.join("keld-host.exe"), MAX_IMAGE, Some(&budget))?;
    let helper = pin_file_with_budget(
        &release.join("keld-updater-helper.exe"),
        MAX_IMAGE,
        Some(&budget),
    )?;
    budget.check()?;
    Ok(LocalSourceBuildObservation {
        source_root: source,
        source_commit: request.commit,
        source_sha256: source_digest.finalize().into(),
        cargo_lock_sha256: cargo_lock,
        cache_sha256,
        recipe_sha256: recipe_digest.finalize().into(),
        tool_sha256: [git.sha256, cargo.sha256, rustc.sha256],
        source_pins,
        _directory_pins: directory_pins,
        host,
        helper,
    })
}

pub(super) fn observed_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::new();
    for &byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    output
}

pub(super) fn prepare_from_args(
    args: &[String],
) -> Result<LocalSourceBuildObservation, BuildError> {
    prepare_from_args_with_role(args, false)
}
fn append_initial_install_feature(args: &mut Vec<OsString>, initial_install: bool) {
    if initial_install {
        args.extend([
            OsString::from("--features"),
            OsString::from("keld-host/windows-initial-install,keld-host/windows-qualified-launch"),
        ]);
    }
}
pub(super) fn request_from_args(args: &[String]) -> Result<Request, BuildError> {
    let (args, cache_mode) = cache_arguments(args)?;
    let [source, commit, session, git, cargo, rustc, cargo_home] = args else {
        return Err(BuildError::invocation(
            "expected source-root, exact-commit, NEW-session-directory, absolute-git, absolute-cargo, absolute-rustc, cargo-cache-root",
        ));
    };
    Ok(Request {
        source: source.into(),
        commit: commit.clone(),
        session: session.into(),
        git: git.into(),
        cargo: cargo.into(),
        rustc: rustc.into(),
        cargo_home: cargo_home.into(),
        cache_mode,
    })
}

fn prepare_from_args_with_role(
    args: &[String],
    initial_install: bool,
) -> Result<LocalSourceBuildObservation, BuildError> {
    prepare(request_from_args(args)?, initial_install)
}

pub(super) fn run(args: &[String]) -> Result<(), BuildError> {
    let receipt = prepare_from_args(args)?;
    let hex = observed_hex;
    println!(
        "Prepared unsigned nonhermetic LocalSourceTest inputs; requested commit {}, observed checkout {}, Cargo.lock {}.",
        receipt.source_commit,
        hex(&receipt.source_sha256),
        hex(&receipt.cargo_lock_sha256)
    );
    match receipt.cache_sha256 {
        Some(digest) => println!(
            "Offline registry snapshot {}; recipe/environment {}.",
            hex(&digest),
            hex(&receipt.recipe_sha256)
        ),
        None => println!(
            "Mutable developer Cargo cache reused in place; dependency contents were not copied, measured or pinned. Recipe/environment {}.",
            hex(&receipt.recipe_sha256)
        ),
    }
    println!(
        "Host SHA256 {}; helper SHA256 {}; Git/Cargo/rustc {:?}.",
        hex(&receipt.host.sha256),
        hex(&receipt.helper.sha256),
        receipt.tool_sha256.map(|digest| hex(&digest))
    );
    println!(
        "Nonhermetic source preparation only. Cargo configuration, build scripts and system linker/toolchain lookup can consume ambient developer inputs. No app build, signing, installation, exact dependency trust, official release provenance or Strict qualification. This diagnostic is not a reusable admission receipt."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(mode: &str, path: &str) -> Vec<u8> {
        format!(":000000 {mode} 0000000000000000000000000000000000000000 1111111111111111111111111111111111111111 A\0{path}\0").into_bytes()
    }
    #[test]
    fn full_tree_listing_preserves_blob_identity_and_mode() {
        let mut bytes = entry("100644", "Cargo.toml");
        bytes.extend(entry("100755", "tools/build.sh"));
        let parsed = parse_members(&bytes).expect("regular source files");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].path, "Cargo.toml");
        assert_eq!(
            parsed[0].object_id,
            "1111111111111111111111111111111111111111"
        );
        assert!(!parsed[0].executable);
        assert!(parsed[1].executable);
    }
    #[test]
    fn links_submodules_truncation_duplicate_and_escapes_refuse() {
        for mode in ["120000", "160000", "040000"] {
            assert!(parse_members(&entry(mode, "outside")).is_err());
        }
        for path in [
            "../outside",
            "a/../outside",
            "/absolute",
            "a\\outside",
            "a//b",
        ] {
            assert!(parse_members(&entry("100644", path)).is_err());
        }
        let mut duplicate = entry("100644", "Cargo.toml");
        duplicate.extend(entry("100644", "Cargo.toml"));
        assert!(parse_members(&duplicate).is_err());
        let valid = entry("100644", "Cargo.toml");
        assert!(parse_members(&valid[..valid.len() - 1]).is_err());
        assert!(parse_members(&[]).is_err());
    }
    #[test]
    fn owned_runner_checks_real_exit_status_and_reaps_before_return() {
        let root = PathBuf::from(std::env::var_os("SystemRoot").expect("Windows system root"));
        let tool = pin_file(&root.join("System32").join("cmd.exe"), MAX_IMAGE)
            .expect("system command fixture");
        let environment = vec![("SystemRoot".into(), root.as_os_str().to_owned())];
        run_owned(
            &tool,
            &os_args(&["/d", "/c", "exit", "0"]),
            &environment,
            &root,
            Duration::from_secs(10),
        )
        .expect("zero exit");
        let error = run_owned(
            &tool,
            &os_args(&["/d", "/c", "exit", "7"]),
            &environment,
            &root,
            Duration::from_secs(10),
        )
        .expect_err("nonzero exit");
        assert!(error.to_string().contains("exit status 7"));
    }
    #[test]
    fn owned_runner_deadline_is_a_failure_not_a_receipt() {
        let root = PathBuf::from(std::env::var_os("SystemRoot").expect("Windows system root"));
        let tool = pin_file(&root.join("System32").join("cmd.exe"), MAX_IMAGE)
            .expect("system command fixture");
        let environment = vec![("SystemRoot".into(), root.as_os_str().to_owned())];
        let error = run_owned(
            &tool,
            &os_args(&["/d", "/c", "for /l %i in (0,0,1) do @rem"]),
            &environment,
            &root,
            Duration::from_millis(100),
        )
        .expect_err("deadline kills exact Job");
        assert!(error.to_string().contains("local source tool wait"));
    }

    #[test]
    fn blob_size_admission_precedes_materialization_and_is_identity_bound() {
        let members = parse_members(&entry("100644", "Cargo.toml")).expect("source");
        let valid = format!("{} blob {}\n", members[0].object_id, MAX_SOURCE_FILE);
        assert_eq!(
            parse_blob_sizes(valid.as_bytes(), &members).expect("exact limit"),
            vec![MAX_SOURCE_FILE]
        );
        for bad in [
            format!("{} blob {}\n", members[0].object_id, MAX_SOURCE_FILE + 1),
            format!("{} tree 1\n", members[0].object_id),
            format!("{} blob 01\n", members[0].object_id),
            format!("{} missing\n", members[0].object_id),
            "2222222222222222222222222222222222222222 blob 1\n".to_owned(),
            valid.trim_end().to_owned(),
            format!("{valid}{valid}"),
        ] {
            assert!(parse_blob_sizes(bad.as_bytes(), &members).is_err());
        }
    }

    #[test]
    fn layout_limits_precede_all_source_writes() {
        let mut members = Vec::new();
        for index in 0..8193 {
            members.push(SourceMember {
                path: format!("root/file{index}"),
                object_id: "1".repeat(40),
                executable: false,
            });
        }
        assert!(validate_source_layout(&members[..8192]).is_ok());
        assert!(validate_source_layout(&members).is_err());
    }

    #[test]
    fn fixed_target_precreation_preserves_guards_and_mutable_output_descendants() {
        let temporary = tempfile::tempdir().expect("owned target parent");
        let target = temporary.path().join("target");
        let guards = create_cargo_target_root(&target).expect("fixed Cargo roots");
        let architecture = target.join("x86_64-pc-windows-msvc");
        assert!(target.is_dir() && architecture.is_dir());
        // Match Cargo's path.is_dir early-return condition without emulating
        // success of a Cargo build. Ordinary build descendants remain writable.
        let release = architecture.join("release");
        fs::create_dir_all(&release).expect("mutable output descendant");
        let staged = tempfile::tempdir_in(&release).expect("mutable output temporary child");
        fs::rename(staged.path(), release.join("renamed-output"))
            .expect("unretained output descendants permit atomic writes");
        let sibling = tempfile::tempdir_in(&target).expect("target temporary sibling");
        let error = fs::rename(sibling.path(), target.join("new-root-child"))
            .expect_err("original immutable root exclusion remains");
        assert_eq!(error.raw_os_error(), Some(32));
        // Never accept/reuse a preexisting target, including a partial session.
        assert!(create_cargo_target_root(&target).is_err());
        drop(guards);
        fs::rename(sibling.path(), target.join("new-root-child"))
            .expect("released positive control");
    }

    #[test]
    fn isolated_source_refuses_ambient_cargo_configuration_in_parent_directories() {
        let temporary = tempfile::tempdir().expect("isolated source fixture");
        let source = temporary.path().join("session").join("source");
        fs::create_dir_all(&source).expect("source fixture");
        validate_cargo_ancestor_configuration(&source).expect("clean ancestry");
        let ambient = temporary.path().join(".cargo");
        fs::create_dir(&ambient).expect("ambient Cargo directory");
        for name in ["config", "config.toml"] {
            let path = ambient.join(name);
            fs::write(&path, b"[build]\nrustflags = ['--cfg=ambient_injection']\n")
                .expect("write only owned nonsecret regression input");
            assert!(
                validate_cargo_ancestor_configuration(&source).is_err(),
                "parent {name} must not influence the isolated build"
            );
            fs::remove_file(path).expect("remove owned regression input");
        }
        // Cargo config inside the materialized source is part of the measured source.
        let tracked = source.join(".cargo");
        fs::create_dir(&tracked).expect("tracked source config directory");
        fs::write(tracked.join("config.toml"), b"[build]\n").expect("write tracked fixture");
        validate_cargo_ancestor_configuration(&source).expect("source config is not ambient");
    }

    #[test]
    fn developer_cache_requires_explicit_opt_in_and_refuses_root_configuration() {
        let args = vec![String::new(); 7];
        assert_eq!(
            cache_arguments(&args).expect("default").1,
            CacheMode::Snapshot
        );
        let mut explicit = args;
        explicit.push("--use-developer-cache".into());
        assert_eq!(
            cache_arguments(&explicit).expect("explicit").1,
            CacheMode::MutableDeveloper
        );
        explicit[7] = "--unknown".into();
        assert!(cache_arguments(&explicit).is_err());
        explicit[7] = String::new();
        explicit[0] = "--use-developer-cache".into();
        assert!(cache_arguments(&explicit).is_err());
        explicit.push("--use-developer-cache".into());
        assert!(cache_arguments(&explicit).is_err());
        let temporary = tempfile::tempdir().expect("owned cache parent");
        let root = temporary.path().join("cache");
        create_private(&root).expect("private cache root");
        validate_developer_cache(&root).expect("clean selected cache");
        for name in ["config", "config.toml", "credentials", "credentials.toml"] {
            let path = root.join(name);
            fs::write(&path, b"synthetic sentinel, not a credential")
                .expect("test metadata sentinel");
            let error = validate_developer_cache(&root).expect_err("root config refused");
            assert!(error.to_string().contains("developer cache configuration"));
            fs::remove_file(path).expect("remove owned sentinel");
        }
    }

    #[test]
    fn private_capture_uses_its_shared_deadline_and_stderr_ceiling() {
        let root = PathBuf::from(std::env::var_os("SystemRoot").expect("Windows system root"));
        let tool = pin_file(&root.join("System32").join("cmd.exe"), MAX_IMAGE)
            .expect("system command fixture");
        let input = File::open("NUL").expect("null input");
        let environment = vec![("SystemRoot".into(), root.as_os_str().to_owned())];
        let start = std::time::Instant::now();
        let error = run_captured_with_options(
            &tool,
            &os_args(&["/d", "/c", "for /l %i in (0,0,1) do @rem"]),
            &environment,
            &root,
            &input,
            CaptureOptions {
                stdout_limit: 1024,
                stderr_limit: 1024,
                timeout: Duration::from_millis(100),
                stderr_file: None,
            },
        )
        .expect_err("shared short deadline");
        assert!(error.to_string().contains("deadline"));
        assert!(start.elapsed() < Duration::from_secs(15));
        let error = run_captured_with_options(
            &tool,
            &os_args(&["/d", "/c", "for /l %i in (0,0,1) do @echo overflow 1>&2"]),
            &environment,
            &root,
            &input,
            CaptureOptions {
                stdout_limit: 1024,
                stderr_limit: 16,
                timeout: Duration::from_secs(10),
                stderr_file: None,
            },
        )
        .expect_err("stderr bound");
        let detail = error.to_string();
        assert!(detail.contains("source stderr") && detail.contains("tool output exceeds limit"));
    }

    #[test]
    fn private_stderr_is_retained_without_echoing_into_build_errors() {
        let root = PathBuf::from(std::env::var_os("SystemRoot").expect("Windows system root"));
        let tool = pin_file(&root.join("System32").join("cmd.exe"), MAX_IMAGE)
            .expect("system command fixture");
        let input = File::open("NUL").expect("null input");
        let mut output = tempfile::tempfile().expect("private test file");
        let environment = vec![("SystemRoot".into(), root.as_os_str().to_owned())];
        let error = run_captured_with_options(
            &tool,
            &os_args(&[
                "/d",
                "/c",
                "echo PRIVATE-COMPILER-DIAGNOSTIC 1>&2 & exit /b 17",
            ]),
            &environment,
            &root,
            &input,
            CaptureOptions {
                stdout_limit: 1024,
                stderr_limit: 1024,
                timeout: Duration::from_secs(10),
                stderr_file: Some(&mut output),
            },
        )
        .expect_err("nonzero tool");
        assert!(error.to_string().contains("exit status 17"));
        assert!(!error.to_string().contains("PRIVATE-COMPILER-DIAGNOSTIC"));
        output.seek(SeekFrom::Start(0)).expect("rewind");
        let mut bytes = String::new();
        output
            .read_to_string(&mut bytes)
            .expect("read retained diagnostic");
        assert!(bytes.contains("PRIVATE-COMPILER-DIAGNOSTIC"));
    }

    #[test]
    fn capture_overflow_cancels_the_job_without_waiting_for_tool_deadline() {
        let root = PathBuf::from(std::env::var_os("SystemRoot").expect("Windows system root"));
        let tool = pin_file(&root.join("System32").join("cmd.exe"), MAX_IMAGE)
            .expect("system command fixture");
        let input = File::open("NUL").expect("null input");
        let environment = vec![("SystemRoot".into(), root.as_os_str().to_owned())];
        let start = std::time::Instant::now();
        let error = run_captured(
            &tool,
            &os_args(&["/d", "/c", "for /l %i in (0,0,1) do @echo overflow"]),
            &environment,
            &root,
            &input,
            16,
        )
        .expect_err("actual stdout overflow");
        let detail = error.to_string();
        assert!(
            detail.contains("source stdout") && detail.contains("tool output exceeds limit"),
            "spawn, initialization, panic, nonzero exit or unrelated I/O cannot satisfy overflow: {detail}"
        );
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "overflow must promptly cancel, not wait 120 seconds"
        );
    }
}

#[cfg(test)]
mod initial_install_feature_tests {
    #[test]
    fn only_explicit_role_adds_exact_feature_before_recipe_capture() {
        let original = vec![std::ffi::OsString::from("build")];
        let mut args = original.clone();
        super::append_initial_install_feature(&mut args, false);
        assert_eq!(args, original);
        super::append_initial_install_feature(&mut args, true);
        assert_eq!(
            args,
            vec![
                "build",
                "--features",
                "keld-host/windows-initial-install,keld-host/windows-qualified-launch"
            ]
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod preparation_budget_tests {
    use super::*;
    #[test]
    fn aggregate_budget_does_not_reset_per_blob() {
        let start = Instant::now();
        let budget = PreparationBudget::new(start, Duration::from_secs(10));
        assert_eq!(
            budget
                .remaining_at(start, Duration::from_secs(120))
                .unwrap(),
            Duration::from_secs(10)
        );
        assert_eq!(
            budget
                .remaining_at(start + Duration::from_secs(9), Duration::from_secs(120))
                .unwrap(),
            Duration::from_secs(1)
        );
        assert!(
            budget
                .remaining_at(start + Duration::from_secs(10), Duration::from_secs(120))
                .is_err()
        );
        assert!(
            budget
                .remaining_at(start + Duration::from_secs(11), Duration::from_secs(120))
                .is_err()
        );
        assert_eq!(
            budget.remaining_at(start, Duration::from_secs(2)).unwrap(),
            Duration::from_secs(2)
        );
    }
    #[test]
    fn git_alternate_is_one_c_quoted_entry() {
        assert_eq!(
            git_alternate_value(Path::new("D:/semi;colon/λ/.git/objects")).unwrap(),
            OsString::from("\"D:/semi;colon/λ/.git/objects\"")
        );
        assert_eq!(
            git_alternate_value(Path::new(r#"D:\a"b\.git\objects"#)).unwrap(),
            OsString::from(r#""D:\\a\"b\\.git\\objects""#)
        );
        for value in ["", "D:/line\nbreak", "D:/tab\tpath", "D:/nul\0path"] {
            assert!(git_alternate_value(Path::new(value)).is_err());
        }
    }
}

#[cfg(test)]
mod retirement_tests;
