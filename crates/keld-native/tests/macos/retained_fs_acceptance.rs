// Private KEL-130 macOS acceptance module, included by `fs.rs` only for lib tests.
// Focused receipt command:
// `cargo test -p keld-native --lib fs::tests::macos_acceptance::macos_retained_filesystem_acceptance -- --exact --nocapture --test-threads=1`

use super::*;
use keld_guard::verified_manifest::{VerifiedManifest, load_verified_manifest};
use sha2::{Digest as _, Sha256};
use std::io;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, mpsc};
use std::time::{Duration, Instant};

const RECORD_SCHEMA: &str = "keld.kel130-macos-acceptance/v1";

struct OwnedRoot(PathBuf);

impl OwnedRoot {
    fn new(case: &str) -> Self {
        let root = Path::new("/tmp").join(format!(
            "k130m-{case}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("create short macOS fixture root");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for OwnedRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn spelling(path: &Path) -> String {
    path.to_str().expect("UTF-8 macOS fixture").to_owned()
}

fn json_array(scopes: &[String]) -> String {
    scopes
        .iter()
        .map(|scope| format!(r#""{scope}""#))
        .collect::<Vec<_>>()
        .join(",")
}

fn verified_manifest(
    fixture: &Path,
    name: &str,
    read_scopes: &[String],
    write_scopes: &[String],
) -> VerifiedManifest {
    let text = format!(
        r#"{{"app":{{"fs":{{"read":[{}],"write":[{}]}}}}}}"#,
        json_array(read_scopes),
        json_array(write_scopes)
    );
    verified_text(fixture, name, &text)
}

fn verified_text(fixture: &Path, name: &str, text: &str) -> VerifiedManifest {
    let path = fixture.join(name);
    std::fs::write(&path, text).expect("write macOS acceptance manifest");
    let digest: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    load_verified_manifest(
        std::fs::File::open(&path).expect("open macOS acceptance manifest"),
        path,
        digest,
    )
    .expect("load macOS acceptance manifest")
}

fn subtree_manifest(fixture: &Path, root: &Path) -> VerifiedManifest {
    let scope = format!("{}/**", spelling(root));
    verified_manifest(
        fixture,
        "subtree-permissions.jsonc",
        std::slice::from_ref(&scope),
        std::slice::from_ref(&scope),
    )
}

fn command_text(program: &str, arguments: &[&str]) -> String {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| unsupported("environment", &format!("command-failed-{error}")));
    if !output.status.success() {
        unsupported(
            "environment",
            &format!("command-exit-{}", output.status.code().unwrap_or(-1)),
        );
    }
    String::from_utf8(output.stdout)
        .expect("macOS command output is UTF-8")
        .trim()
        .to_owned()
}

fn emit_environment() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let head = command_text(
        "git",
        &[
            "-C",
            manifest_dir.to_str().expect("manifest path"),
            "rev-parse",
            "HEAD",
        ],
    );
    let tree = command_text(
        "git",
        &[
            "-C",
            manifest_dir.to_str().expect("manifest path"),
            "status",
            "--porcelain=v1",
        ],
    );
    let model = command_text("/usr/sbin/sysctl", &["-n", "hw.model"]);
    let virtualized = command_text("/usr/sbin/sysctl", &["-n", "kern.hv_vmm_present"]);
    let version = command_text("/usr/bin/sw_vers", &["-productVersion"]);
    let build = command_text("/usr/bin/sw_vers", &["-buildVersion"]);
    let arch = command_text("/usr/bin/uname", &["-m"]);
    assert_eq!(virtualized, "0", "real-device row cannot run in a VM");
    println!(
        "KEL130_MACOS_ACCEPTANCE schema={RECORD_SCHEMA} record=begin status=running source_head={head} tree_state={} device_id={model} os_version={version} os_build={build} arch={arch} virtualized={virtualized}",
        if tree.is_empty() { "clean" } else { "dirty" }
    );
}

fn emit(case: &str, code: &str, sentinel: &str, counters: FsTestCounters, detail: &str) {
    println!(
        "KEL130_MACOS_ACCEPTANCE schema={RECORD_SCHEMA} case={case} status=passed code={code} sentinel={sentinel} walk={} metadata={} target_open={} content_read={} create={} truncate={} content_write={} grant_index={} fresh=ok detail={detail}",
        counters.walk_entries,
        counters.metadata_probes,
        counters.target_opens,
        counters.content_reads,
        counters.create_calls,
        counters.truncate_calls,
        counters.content_writes,
        counters
            .selected_grant_index
            .map_or_else(|| "none".to_owned(), |index| index.to_string()),
    );
}

fn unsupported(case: &str, detail: &str) -> ! {
    println!(
        "KEL130_MACOS_ACCEPTANCE schema={RECORD_SCHEMA} case={case} status=unsupported detail={detail}"
    );
    panic!("required macOS acceptance primitive is unavailable: {case}: {detail}");
}

fn fresh_read(broker: &FsBroker, verified: &VerifiedManifest, path: &Path, expected: &[u8]) {
    assert_eq!(
        broker
            .read(
                verified,
                Principal::AppProcess,
                &spelling(path),
                &AtomicBool::new(false),
            )
            .expect("fresh allowed operation"),
        expected
    );
}

fn expect_code(result: Result<Vec<u8>, FsError>, code: &str) -> FsError {
    let error = result.expect_err("hostile read must fail");
    assert_eq!(error.code(), code);
    error
}

fn grammar_and_first_match() {
    let fixture = OwnedRoot::new("macos-grammar");
    let root = fixture.path().join("granted");
    std::fs::create_dir(&root).expect("create grammar root");
    let fresh = root.join("fresh");
    let selected = root.join("selected");
    std::fs::write(&fresh, b"fresh").expect("write fresh grammar control");
    std::fs::write(&selected, b"selected").expect("write first-match target");
    let root_scope = format!("{}/**", spelling(&root));
    let selected_scope = spelling(&selected);
    let verified = verified_manifest(
        fixture.path(),
        "grammar.jsonc",
        &[root_scope, selected_scope],
        &[],
    );
    let broker = FsBroker::prepare(&verified).expect("prepare grammar broker");
    let cancelled = AtomicBool::new(false);

    fs_test_reset_counters();
    assert_eq!(
        broker
            .read(
                &verified,
                Principal::AppProcess,
                &spelling(&selected),
                &cancelled,
            )
            .expect("first matching grant read"),
        b"selected"
    );
    let counters = fs_test_counters();
    assert_eq!(counters.selected_grant_index, Some(0));
    fresh_read(&broker, &verified, &fresh, b"fresh");
    emit(
        "broker-first-match",
        "ok",
        "selected",
        counters,
        "first-index-0",
    );

    let root_prefix = format!("{}/", spelling(&root));
    let maximum = format!(
        "{root_prefix}{}",
        "x".repeat(MAX_FS_PATH_BYTES - root_prefix.len())
    );
    let over = format!("{maximum}x");
    assert_eq!(maximum.len(), MAX_FS_PATH_BYTES);
    assert_eq!(over.len(), MAX_FS_PATH_BYTES + 1);

    fs_test_reset_counters();
    let maximum_error = broker
        .read(&verified, Principal::AppProcess, &maximum, &cancelled)
        .expect_err("synthetic maximum path names no file");
    assert_ne!(maximum_error.code(), "KELD-NATIVE-004");
    let counters = fs_test_counters();
    fresh_read(&broker, &verified, &fresh, b"fresh");
    emit(
        "path-4096",
        maximum_error.code(),
        "unchanged",
        counters,
        "size-boundary-admitted",
    );

    for (case, path, expected_code) in [
        ("path-4097", over, "KELD-NATIVE-004"),
        (
            "path-nul",
            format!("{}/bad\0name", spelling(&root)),
            "KELD-NATIVE-004",
        ),
        ("path-empty", String::new(), "KELD-GUARD002"),
        ("path-dot", ".".to_owned(), "KELD-GUARD002"),
        ("path-relative", "relative".to_owned(), "KELD-GUARD002"),
        (
            "path-repeated-separator",
            format!("{}//fresh", spelling(&root)),
            "KELD-GUARD002",
        ),
        (
            "path-dot-component",
            format!("{}/./fresh", spelling(&root)),
            "KELD-GUARD002",
        ),
        (
            "path-parent-component",
            format!("{}/../fresh", spelling(&root)),
            "KELD-GUARD002",
        ),
        (
            "path-backslash",
            format!("{}\\fresh", spelling(&root)),
            "KELD-GUARD002",
        ),
    ] {
        fs_test_reset_counters();
        let error = expect_code(
            broker.read(&verified, Principal::AppProcess, &path, &cancelled),
            expected_code,
        );
        let counters = fs_test_counters();
        assert_eq!(counters.walk_entries, 0, "{case} entered traversal");
        assert_eq!(counters.target_opens, 0, "{case} opened a target");
        assert_eq!(counters.content_reads, 0, "{case} performed content I/O");
        fresh_read(&broker, &verified, &fresh, b"fresh");
        emit(case, error.code(), "unchanged", counters, "boundary-deny");
    }
}

fn scope_grammar() {
    let fixture = OwnedRoot::new("macos-scope-grammar");
    let root = fixture.path().join("granted");
    std::fs::create_dir(&root).expect("create scope grammar root");
    let fresh = root.join("fresh");
    std::fs::write(&fresh, b"fresh").expect("seed scope grammar control");
    let control = subtree_manifest(fixture.path(), &root);
    let control_broker = FsBroker::prepare(&control).expect("prepare scope grammar control");
    let root_text = spelling(&root);
    let invalid = [
        (
            "scope-relative",
            r#"{"app":{"fs":{"read":["relative/**"]}}}"#.to_owned(),
        ),
        (
            "scope-variable",
            r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#.to_owned(),
        ),
        (
            "scope-empty-component",
            format!(r#"{{"app":{{"fs":{{"read":["{root_text}//x"]}}}}}}"#),
        ),
        (
            "scope-dot-component",
            format!(r#"{{"app":{{"fs":{{"read":["{root_text}/./x"]}}}}}}"#),
        ),
        (
            "scope-parent-component",
            format!(r#"{{"app":{{"fs":{{"read":["{root_text}/../x"]}}}}}}"#),
        ),
        (
            "scope-nul",
            format!(r#"{{"app":{{"fs":{{"read":["{root_text}/bad\u0000name"]}}}}}}"#),
        ),
        (
            "scope-duplicate",
            format!(r#"{{"app":{{"fs":{{"read":["{root_text}/**","{root_text}/**"]}}}}}}"#),
        ),
        (
            "scope-exact-root",
            r#"{"app":{"fs":{"read":["/"]}}}"#.to_owned(),
        ),
    ];
    for (index, (case, text)) in invalid.into_iter().enumerate() {
        let verified = verified_text(fixture.path(), &format!("invalid-{index}.jsonc"), &text);
        fs_test_reset_counters();
        let error = FsBroker::prepare(&verified).expect_err("invalid scope must reject");
        assert_eq!(error.code(), "KELD-NATIVE-008");
        let counters = fs_test_counters();
        assert_eq!(counters, FsTestCounters::new());
        fresh_read(&control_broker, &control, &fresh, b"fresh");
        emit(case, error.code(), "unchanged", counters, "prepare-deny");
    }

    let excessive = (0..65)
        .map(|index| format!("{root_text}/scope-{index}"))
        .collect::<Vec<_>>();
    let verified = verified_manifest(fixture.path(), "excessive.jsonc", &excessive, &[]);
    fs_test_reset_counters();
    let error = FsBroker::prepare(&verified).expect_err("65 scopes must reject");
    assert_eq!(error.code(), "KELD-NATIVE-008");
    let counters = fs_test_counters();
    fresh_read(&control_broker, &control, &fresh, b"fresh");
    emit(
        "scope-count-65",
        error.code(),
        "unchanged",
        counters,
        "maximum-64",
    );

    let missing = fixture.path().join("missing");
    let partial = verified_manifest(
        fixture.path(),
        "partial.jsonc",
        &[
            format!("{root_text}/**"),
            format!("{}/**", spelling(&missing)),
        ],
        &[],
    );
    let baseline = owner_handle_count();
    let error = FsBroker::prepare(&partial).expect_err("second scope open must fail");
    assert_eq!(error.code(), "KELD-NATIVE-008");
    assert_eq!(
        owner_handle_count(),
        baseline,
        "partial prepare leaked a root"
    );
    fresh_read(&control_broker, &control, &fresh, b"fresh");
    emit(
        "scope-partial-prepare-unwind",
        error.code(),
        "owner-handles-restored",
        FsTestCounters::new(),
        "valid-first-missing-second",
    );
}

fn mount_and_special_files() {
    let fixture = OwnedRoot::new("macos-special");
    let fresh = fixture.path().join("fresh");
    let fifo = fixture.path().join("fifo");
    let socket = fixture.path().join("socket");
    std::fs::write(&fresh, b"fresh").expect("write special-file control");
    let mkfifo = Command::new("/usr/bin/mkfifo")
        .arg(&fifo)
        .status()
        .unwrap_or_else(|error| unsupported("fifo", &format!("mkfifo-failed-{error}")));
    if !mkfifo.success() {
        unsupported("fifo", "mkfifo-nonzero");
    }
    let listener = std::os::unix::net::UnixListener::bind(&socket)
        .unwrap_or_else(|error| unsupported("unix-socket", &format!("bind-failed-{error}")));

    let root_scope = "/**".to_owned();
    let root_verified = verified_manifest(
        fixture.path(),
        "mount.jsonc",
        std::slice::from_ref(&root_scope),
        &[],
    );
    let root_broker = FsBroker::prepare(&root_verified).expect("prepare volume-root broker");
    fs_test_reset_counters();
    let mount_error = expect_code(
        root_broker.read(
            &root_verified,
            Principal::AppProcess,
            "/dev/null",
            &AtomicBool::new(false),
        ),
        "KELD-NATIVE-003",
    );
    let counters = fs_test_counters();
    assert_eq!(
        counters.metadata_probes, 1,
        "mount crossing must stop at the first foreign-device component"
    );
    assert_eq!(counters.target_opens, 0);
    assert_eq!(counters.content_reads, 0);
    fresh_read(&root_broker, &root_verified, &fresh, b"fresh");
    emit(
        "mount-crossing-devfs",
        mount_error.code(),
        "no-target-open",
        counters,
        "root-device-differs-from-devfs",
    );

    let block = Path::new("/dev/disk0");
    if !block.exists() {
        unsupported("block-device", "dev-disk0-missing");
    }
    let scopes = [
        "/dev/null".to_owned(),
        spelling(block),
        spelling(&fifo),
        spelling(&socket),
        spelling(&fresh),
    ];
    let verified = verified_manifest(fixture.path(), "special.jsonc", &scopes, &[]);
    let broker = FsBroker::prepare(&verified).expect("prepare special-file broker");
    for (case, path) in [
        ("character-device", Path::new("/dev/null")),
        ("block-device", block),
        ("fifo", fifo.as_path()),
        ("unix-socket", socket.as_path()),
    ] {
        fs_test_reset_counters();
        let error = expect_code(
            broker.read(
                &verified,
                Principal::AppProcess,
                &spelling(path),
                &AtomicBool::new(false),
            ),
            "KELD-NATIVE-003",
        );
        let counters = fs_test_counters();
        assert_eq!(counters.target_opens, 0, "{case} reached target open");
        assert_eq!(counters.content_reads, 0, "{case} reached content I/O");
        fresh_read(&broker, &verified, &fresh, b"fresh");
        emit(
            case,
            error.code(),
            "unchanged",
            counters,
            "nonregular-before-open",
        );
    }
    drop(listener);
}

fn parent_and_component_races() {
    let fixture = OwnedRoot::new("macos-parent-race");
    let root = fixture.path().join("granted");
    let moved = fixture.path().join("moved");
    std::fs::create_dir(&root).expect("create retained root");
    std::fs::write(root.join("victim"), b"retained").expect("seed retained victim");
    std::fs::write(root.join("fresh"), b"fresh").expect("seed retained fresh");
    let verified = subtree_manifest(fixture.path(), &root);
    let broker = FsBroker::prepare(&verified).expect("prepare retained root broker");
    std::fs::rename(&root, &moved).expect("rename retained root");
    std::fs::create_dir(&root).expect("create ambient replacement root");
    std::fs::write(root.join("victim"), b"outside").expect("seed ambient replacement");
    fs_test_reset_counters();
    let retained = broker
        .read(
            &verified,
            Principal::AppProcess,
            &spelling(&root.join("victim")),
            &AtomicBool::new(false),
        )
        .expect("read retained parent object");
    assert_eq!(retained, b"retained");
    assert_eq!(
        std::fs::read(root.join("victim")).expect("ambient sentinel"),
        b"outside"
    );
    let counters = fs_test_counters();
    fresh_read(&broker, &verified, &root.join("fresh"), b"fresh");
    emit(
        "parent-replacement",
        "ok",
        "ambient-outside",
        counters,
        "retained-object-read",
    );

    run_component_race(false);
    run_component_race(true);
    run_create_new_race();
}

fn run_component_race(final_component: bool) {
    let fixture = OwnedRoot::new(if final_component {
        "macos-final-race"
    } else {
        "macos-component-race"
    });
    let root = fixture.path().join("granted");
    let outside = fixture.path().join("outside");
    std::fs::create_dir(&root).expect("create race root");
    std::fs::write(root.join("fresh"), b"fresh").expect("seed race control");
    if final_component {
        std::fs::write(root.join("victim"), b"inside").expect("seed final victim");
        std::fs::write(&outside, b"outside").expect("seed outside file");
    } else {
        std::fs::create_dir(root.join("victim")).expect("seed victim directory");
        std::fs::write(root.join("victim/sentinel"), b"inside").expect("seed inside file");
        std::fs::create_dir(&outside).expect("seed outside directory");
        std::fs::write(outside.join("sentinel"), b"outside").expect("seed outside sentinel");
    }
    let grant = super::race_grant(&root);
    let cancelled = AtomicBool::new(false);
    let progress = Progress::new(&cancelled);
    let reached = Arc::new(Barrier::new(2));
    let swapped = Arc::new(Barrier::new(2));
    let worker_reached = Arc::clone(&reached);
    let worker_swapped = Arc::clone(&swapped);
    let victim = root.join("victim");
    let original = root.join("original");
    let outside_worker = outside.clone();
    let worker = std::thread::spawn(move || {
        worker_reached.wait();
        if final_component {
            std::fs::remove_file(&victim).expect("remove final victim");
        } else {
            std::fs::rename(&victim, &original).expect("move intermediate victim");
        }
        symlink(&outside_worker, &victim).expect("install outside race link");
        worker_swapped.wait();
    });
    let requested = if final_component {
        root.join("victim")
    } else {
        root.join("victim/sentinel")
    };
    fs_test_reset_counters();
    let error = walk_with_observer(
        &grant,
        &spelling(&requested),
        OpenPurpose::Read,
        &progress,
        |name, is_final| {
            if name == "victim" && is_final == final_component {
                reached.wait();
                swapped.wait();
            }
        },
    )
    .expect_err("namespace race must fail closed");
    worker.join().expect("join race worker");
    assert_eq!(error.code(), "KELD-NATIVE-002");
    let outside_path = if final_component {
        outside.clone()
    } else {
        outside.join("sentinel")
    };
    assert_eq!(
        std::fs::read(outside_path).expect("outside race sentinel"),
        b"outside"
    );
    let counters = fs_test_counters();
    super::assert_fresh_race_read(&grant, &root, &progress);
    emit(
        if final_component {
            "final-race"
        } else {
            "component-race"
        },
        error.code(),
        "outside-unchanged",
        counters,
        "barrier-swap",
    );
}

fn run_create_new_race() {
    let fixture = OwnedRoot::new("macos-create-race");
    let root = fixture.path().join("granted");
    std::fs::create_dir(&root).expect("create new-file race root");
    let target = root.join("victim");
    let fresh = root.join("fresh");
    std::fs::write(&fresh, b"fresh").expect("seed create-race control");
    let verified = subtree_manifest(fixture.path(), &root);
    let broker = FsBroker::prepare(&verified).expect("prepare create-race broker");
    let target_for_hook = target.clone();
    AFTER_WALK_TEST_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            std::fs::write(&target_for_hook, b"racer").expect("install raced leaf");
        }));
    });
    fs_test_reset_counters();
    let error = broker
        .write(
            &verified,
            Principal::AppProcess,
            &spelling(&target),
            b"broker",
            &AtomicBool::new(false),
        )
        .expect_err("create-new race must reject");
    assert_eq!(error.code(), "KELD-NATIVE-002");
    assert_eq!(std::fs::read(&target).expect("race sentinel"), b"racer");
    let counters = fs_test_counters();
    assert_eq!(counters.create_calls, 1);
    assert_eq!(counters.truncate_calls, 0);
    assert_eq!(counters.content_writes, 0);
    fresh_read(&broker, &verified, &fresh, b"fresh");
    emit(
        "create-new-race",
        error.code(),
        "racer-unchanged",
        counters,
        "already-exists-no-effect",
    );
}

std::thread_local! {
    static ACCEPTANCE_NOW: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

fn acceptance_now() -> Instant {
    ACCEPTANCE_NOW.with(|now| now.get().expect("acceptance clock initialized"))
}

fn set_acceptance_now(now: Instant) {
    ACCEPTANCE_NOW.with(|clock| clock.set(Some(now)));
}

#[derive(Clone, Copy)]
enum WriterAction {
    CancelAfterFirst,
    ExpireAfterFirst,
    ErrorAfterPartial,
    ErrorImmediately,
    CancelAfterFull,
    ExpireAfterFull,
}

struct TracedWriter<'a> {
    action: WriterAction,
    cancelled: &'a AtomicBool,
    deadline: Instant,
    writes: usize,
    bytes: Vec<u8>,
}

struct EffectCase {
    name: &'static str,
    action: WriterAction,
    expected_cause: &'static str,
    committed: u64,
}

impl io::Write for TracedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.writes += 1;
        match self.action {
            WriterAction::ErrorImmediately => Err(io::Error::other("injected-first-write")),
            WriterAction::ErrorAfterPartial if self.writes > 1 => {
                Err(io::Error::other("injected-second-write"))
            }
            action => {
                let count = if matches!(action, WriterAction::ErrorAfterPartial)
                    || matches!(
                        action,
                        WriterAction::CancelAfterFirst | WriterAction::ExpireAfterFirst
                    ) {
                    bytes.len().min(3)
                } else {
                    bytes.len()
                };
                self.bytes.extend_from_slice(&bytes[..count]);
                match action {
                    WriterAction::CancelAfterFirst | WriterAction::CancelAfterFull => {
                        self.cancelled.store(true, Ordering::Release);
                    }
                    WriterAction::ExpireAfterFirst | WriterAction::ExpireAfterFull => {
                        set_acceptance_now(self.deadline);
                    }
                    WriterAction::ErrorAfterPartial | WriterAction::ErrorImmediately => {}
                }
                Ok(count)
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn effect_and_deadline_ordering() {
    let fixture = OwnedRoot::new("macos-effects");
    let root = fixture.path().join("granted");
    std::fs::create_dir(&root).expect("create effects root");
    let fresh = root.join("fresh");
    std::fs::write(&fresh, b"fresh").expect("seed effects control");
    let verified = subtree_manifest(fixture.path(), &root);
    let broker = FsBroker::prepare(&verified).expect("prepare effects broker");
    let payload = b"12345678";

    let cancelled = AtomicBool::new(true);
    let now = Instant::now();
    let progress = Progress {
        cancelled: &cancelled,
        deadline: now,
        now: acceptance_now,
    };
    set_acceptance_now(now);
    fs_test_reset_counters();
    let pre_effect = progress
        .check_write(
            false,
            0,
            payload.len(),
            Some(io::Error::other("simultaneous")),
        )
        .expect_err("cancel wins before effect");
    assert_eq!(pre_effect.code(), "KELD-NATIVE-006");
    let counters = fs_test_counters();
    fresh_read(&broker, &verified, &fresh, b"fresh");
    emit(
        "pre-effect-cancel-deadline-error",
        pre_effect.code(),
        "unchanged",
        counters,
        "cancel-wins",
    );

    for case in [
        EffectCase {
            name: "partial-cancel",
            action: WriterAction::CancelAfterFirst,
            expected_cause: "cancelled",
            committed: 3,
        },
        EffectCase {
            name: "partial-deadline",
            action: WriterAction::ExpireAfterFirst,
            expected_cause: "deadline",
            committed: 3,
        },
        EffectCase {
            name: "partial-io-error",
            action: WriterAction::ErrorAfterPartial,
            expected_cause: "io",
            committed: 3,
        },
        EffectCase {
            name: "first-write-error",
            action: WriterAction::ErrorImmediately,
            expected_cause: "io",
            committed: 0,
        },
        EffectCase {
            name: "full-write-late-cancel",
            action: WriterAction::CancelAfterFull,
            expected_cause: "cancelled",
            committed: payload.len() as u64,
        },
        EffectCase {
            name: "full-write-late-deadline",
            action: WriterAction::ExpireAfterFull,
            expected_cause: "deadline",
            committed: payload.len() as u64,
        },
    ] {
        run_effect_case(&case, payload, &broker, &verified, &fresh);
    }
}

fn run_effect_case(
    case: &EffectCase,
    payload: &[u8],
    broker: &FsBroker,
    verified: &VerifiedManifest,
    fresh: &Path,
) {
    let start = Instant::now();
    let deadline = start + Duration::from_secs(1);
    set_acceptance_now(start);
    let cancelled = AtomicBool::new(false);
    let progress = Progress {
        cancelled: &cancelled,
        deadline,
        now: acceptance_now,
    };
    let mut writer = TracedWriter {
        action: case.action,
        cancelled: &cancelled,
        deadline,
        writes: 0,
        bytes: Vec::new(),
    };
    fs_test_reset_counters();
    let error = write_content(&mut writer, payload, &progress)
        .expect_err("post-commit condition must be effect-may-have-occurred");
    let FsError::WriteEffect {
        cause,
        committed_bytes,
        requested_bytes,
    } = &error
    else {
        panic!("{} returned wrong effect class: {error:?}", case.name);
    };
    let actual_cause = match cause {
        WriteInterruption::Cancelled => "cancelled",
        WriteInterruption::Deadline => "deadline",
        WriteInterruption::Io(_) => "io",
    };
    assert_eq!(actual_cause, case.expected_cause);
    assert_eq!(*committed_bytes, case.committed);
    assert_eq!(*requested_bytes, payload.len() as u64);
    assert_eq!(writer.bytes.len() as u64, case.committed);
    let counters = fs_test_counters();
    fresh_read(broker, verified, fresh, b"fresh");
    emit(
        case.name,
        error.code(),
        "trace-matches-committed",
        counters,
        case.expected_cause,
    );
}

fn owner_handle_count() -> usize {
    std::fs::read_dir("/dev/fd")
        .expect("open owner handle census")
        .count()
}

fn root_identity(path: &Path) -> (u64, u64) {
    let metadata = std::fs::metadata(path).expect("root identity metadata");
    (
        std::os::unix::fs::MetadataExt::dev(&metadata),
        std::os::unix::fs::MetadataExt::ino(&metadata),
    )
}

#[test]
fn macos_inheritance_census_child() {
    if std::env::var_os("KELD_KEL130_MACOS_INHERITANCE_CHILD").is_none() {
        return;
    }
    let expected_dev: u64 = std::env::var("KELD_KEL130_ROOT_DEV")
        .expect("root dev")
        .parse()
        .expect("root dev integer");
    let expected_ino: u64 = std::env::var("KELD_KEL130_ROOT_INO")
        .expect("root ino")
        .parse()
        .expect("root ino integer");
    let expected_inherited: usize = std::env::var("KELD_KEL130_EXPECT_INHERITED")
        .unwrap_or_else(|_| "0".to_owned())
        .parse()
        .expect("expected inherited count");
    let inherited = std::fs::read_dir("/dev/fd")
        .expect("read child descriptors")
        .filter_map(Result::ok)
        .filter_map(|entry| std::fs::File::open(entry.path()).ok())
        .filter_map(|file| file.metadata().ok())
        .filter(|metadata| {
            std::os::unix::fs::MetadataExt::dev(metadata) == expected_dev
                && std::os::unix::fs::MetadataExt::ino(metadata) == expected_ino
        })
        .count();
    println!(
        "KEL130_MACOS_ACCEPTANCE schema={RECORD_SCHEMA} case=handle-inheritance status=passed inherited_root_handles={inherited} expected={expected_inherited}"
    );
    assert_eq!(
        inherited, expected_inherited,
        "child root-handle census disagrees with the expected control"
    );
}

fn run_blocked_call(
    broker: &Arc<FsBroker>,
    verified: &Arc<VerifiedManifest>,
    file: &Path,
    prepared: usize,
) -> (usize, FsError) {
    let cancelled = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);
    let (result_tx, result_rx) = mpsc::sync_channel(0);
    let thread_broker = Arc::clone(broker);
    let thread_verified = Arc::clone(verified);
    let thread_cancelled = Arc::clone(&cancelled);
    let requested = spelling(file);
    let writer = std::thread::spawn(move || {
        AFTER_WALK_TEST_HOOK.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                ready_tx.send(()).expect("publish live call");
                release_rx.recv().expect("release live call");
            }));
        });
        let result = thread_broker.write(
            &thread_verified,
            Principal::AppProcess,
            &requested,
            b"changed",
            &thread_cancelled,
        );
        result_tx.send(result).expect("publish call result");
    });
    ready_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("blocked call reached retained handle");
    let live_call = owner_handle_count();
    assert!(
        live_call > prepared,
        "blocked call must own an additional handle"
    );
    cancelled.store(true, Ordering::Release);
    assert!(matches!(
        result_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    release_tx.send(()).expect("release blocked call");
    let error = result_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("terminal result after release")
        .expect_err("released call observes cancellation");
    writer.join().expect("join blocked writer");
    assert_eq!(error.code(), "KELD-NATIVE-006");
    assert_eq!(std::fs::read(file).expect("blocked sentinel"), b"unchanged");
    assert_eq!(
        owner_handle_count(),
        prepared,
        "call handle closes before result"
    );
    (live_call, error)
}

fn assert_inheritance_controls(root: &Path) {
    let (root_dev, root_ino) = root_identity(root);
    let test_executable = std::env::current_exe().expect("test executable");
    let negative_control = Command::new("/bin/sh")
        .args([
            "-c",
            "exec 9<\"$KELD_KEL130_ROOT_PATH\"; exec \"$KELD_KEL130_TEST_EXE\" --exact fs::tests::macos_acceptance::macos_inheritance_census_child --nocapture",
        ])
        .env("KELD_KEL130_ROOT_PATH", root)
        .env("KELD_KEL130_TEST_EXE", &test_executable)
        .env("KELD_KEL130_MACOS_INHERITANCE_CHILD", "1")
        .env("KELD_KEL130_ROOT_DEV", root_dev.to_string())
        .env("KELD_KEL130_ROOT_INO", root_ino.to_string())
        .env("KELD_KEL130_EXPECT_INHERITED", "1")
        .output()
        .expect("run inheritance census negative control");
    assert!(
        negative_control.status.success(),
        "inheritance census negative control failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&negative_control.stdout),
        String::from_utf8_lossy(&negative_control.stderr)
    );
    print!("{}", String::from_utf8_lossy(&negative_control.stdout));

    let inheritance = Command::new(test_executable)
        .args([
            "--exact",
            "fs::tests::macos_acceptance::macos_inheritance_census_child",
            "--nocapture",
        ])
        .env("KELD_KEL130_MACOS_INHERITANCE_CHILD", "1")
        .env("KELD_KEL130_ROOT_DEV", root_dev.to_string())
        .env("KELD_KEL130_ROOT_INO", root_ino.to_string())
        .env("KELD_KEL130_EXPECT_INHERITED", "0")
        .output()
        .expect("run inheritance child");
    assert!(
        inheritance.status.success(),
        "inheritance child failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&inheritance.stdout),
        String::from_utf8_lossy(&inheritance.stderr)
    );
    print!("{}", String::from_utf8_lossy(&inheritance.stdout));
}

#[test]
fn macos_handle_lifecycle_child() {
    if std::env::var_os("KELD_KEL130_MACOS_LIFECYCLE_CHILD").is_none() {
        return;
    }
    let fixture = OwnedRoot::new("macos-lifecycle-child");
    let root = fixture.path().join("granted");
    std::fs::create_dir(&root).expect("create lifecycle root");
    let file = root.join("file");
    let fresh = root.join("fresh");
    std::fs::write(&file, b"unchanged").expect("seed lifecycle file");
    std::fs::write(&fresh, b"fresh").expect("seed lifecycle control");
    let verified = Arc::new(subtree_manifest(fixture.path(), &root));
    let baseline = owner_handle_count();
    let deliberate = std::fs::File::open(&file).expect("deliberate census handle");
    assert_eq!(
        owner_handle_count(),
        baseline + 1,
        "census negative control"
    );
    drop(deliberate);
    assert_eq!(owner_handle_count(), baseline);

    let empty = verified_manifest(fixture.path(), "empty.jsonc", &[], &[]);
    let empty_broker = FsBroker::prepare(&empty).expect("prepare empty broker");
    assert_eq!(
        owner_handle_count(),
        baseline,
        "empty broker retained a handle"
    );
    drop(empty_broker);

    let broker = FsBroker::prepare(&verified).expect("prepare lifecycle broker");
    let prepared = owner_handle_count();
    assert!(prepared > baseline, "prepared broker must retain roots");
    fresh_read(&broker, &verified, &fresh, b"fresh");
    assert_eq!(
        owner_handle_count(),
        prepared,
        "returned read leaked a call handle"
    );

    let first = std::rc::Rc::new(broker);
    let last = std::rc::Rc::clone(&first);
    drop(first);
    assert_eq!(
        owner_handle_count(),
        prepared,
        "one Rc drop is not destruction"
    );
    drop(last);
    assert_eq!(
        owner_handle_count(),
        baseline,
        "last Rc drop must restore baseline"
    );

    let broker = Arc::new(FsBroker::prepare(&verified).expect("prepare blocked-call broker"));
    let prepared = owner_handle_count();
    let (live_call, error) = run_blocked_call(&broker, &verified, &file, prepared);
    fresh_read(&broker, &verified, &fresh, b"fresh");
    assert_inheritance_controls(&root);

    let survivor = Arc::clone(&broker);
    drop(broker);
    assert_eq!(
        owner_handle_count(),
        prepared,
        "one Arc drop is not destruction"
    );
    drop(survivor);
    drop(verified);
    assert_eq!(
        owner_handle_count(),
        baseline,
        "actual broker destructor restores baseline"
    );
    println!(
        "KEL130_MACOS_ACCEPTANCE schema={RECORD_SCHEMA} case=handle-lifecycle status=passed code={} sentinel=unchanged baseline={baseline} prepared={prepared} live_call={live_call} after_call={prepared} after_destructor={baseline} fresh=ok detail=blocked-no-terminal-until-release",
        error.code()
    );
}

fn handle_lifecycle() {
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "fs::tests::macos_acceptance::macos_handle_lifecycle_child",
            "--nocapture",
        ])
        .env("KELD_KEL130_MACOS_LIFECYCLE_CHILD", "1")
        .output()
        .expect("run lifecycle child");
    assert!(
        output.status.success(),
        "lifecycle child failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
}

fn privilege_metadata_strip() {
    let fixture = OwnedRoot::new("macos-privilege-metadata");
    let root = fixture.path().join("granted");
    std::fs::create_dir(&root).expect("create privilege root");
    let target = root.join("target");
    let fresh = root.join("fresh");
    std::fs::write(&target, b"before").expect("seed privilege target");
    std::fs::write(&fresh, b"fresh").expect("seed privilege control");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o4755))
        .expect("set privilege metadata");
    let before = std::fs::metadata(&target).expect("privilege metadata before");
    if before.mode() & 0o4000 == 0 {
        unsupported("privilege-metadata", "setuid-bit-could-not-be-established");
    }
    let verified = subtree_manifest(fixture.path(), &root);
    let broker = FsBroker::prepare(&verified).expect("prepare privilege broker");
    fs_test_reset_counters();
    broker
        .write(
            &verified,
            Principal::AppProcess,
            &spelling(&target),
            b"after",
            &AtomicBool::new(false),
        )
        .expect("in-place privilege write");
    let after = std::fs::metadata(&target).expect("privilege metadata after");
    assert_eq!(
        std::os::unix::fs::MetadataExt::dev(&before),
        std::os::unix::fs::MetadataExt::dev(&after)
    );
    assert_eq!(
        std::os::unix::fs::MetadataExt::ino(&before),
        std::os::unix::fs::MetadataExt::ino(&after)
    );
    assert_eq!(before.uid(), after.uid());
    assert_eq!(before.gid(), after.gid());
    assert_eq!(
        after.mode() & 0o4000,
        0,
        "broker must not restore stripped set-ID"
    );
    assert_eq!(after.mode() & 0o777, 0o755, "ordinary mode bits changed");
    assert_eq!(
        std::fs::read(&target).expect("privilege target bytes"),
        b"after"
    );
    let counters = fs_test_counters();
    assert_eq!(counters.truncate_calls, 1);
    assert_eq!(counters.content_writes, 1);
    fresh_read(&broker, &verified, &fresh, b"fresh");
    emit(
        "privilege-metadata-strip",
        "ok",
        "same-inode-owner-setid-cleared",
        counters,
        "no-restore",
    );
}

#[test]
fn macos_retained_filesystem_acceptance() {
    emit_environment();
    grammar_and_first_match();
    scope_grammar();
    mount_and_special_files();
    parent_and_component_races();
    effect_and_deadline_ordering();
    handle_lifecycle();
    privilege_metadata_strip();
    println!(
        "KEL130_MACOS_ACCEPTANCE schema={RECORD_SCHEMA} record=end status=passed cases=40 terminal=false rerun_after_exact_landing=true"
    );
}
