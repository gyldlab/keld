"""Draft-only GH451 source projection: run without building KELD or starting native UI.

Copies the pinned production registry/shutdown method bodies and original transaction
prototype verbatim. Minimal IPC/native value types and delivered receipts are fixtures.
This is state/ordering evidence, not full Core integration or physical OS evidence.
"""
from pathlib import Path
import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import tempfile
import threading

ROOT = Path(__file__).resolve().parent
SOURCE_HASHES = {}
COMPILE_TIMEOUT_SECONDS = 60
PROJECTION_TIMEOUT_SECONDS = 30
CLEANUP_TIMEOUT_SECONDS = 5
OWNER_TEST_PREFIX = (
    "app_session::window_registry::tests::terminal_quit_candidate::core_owner_integration::"
)
CLOSING_CASE = "real_registry_close_pending_and_closing_cannot_authorize_terminal"
DRAIN_CREATE_CASE = "actual_create_during_reversible_drain_cannot_be_hidden_by_build_failure"


def live_group_members(group):
    """Inspect live members while the unreaped leader pins the group identity."""
    census = subprocess.run(
        ["/bin/ps", "-axo", "pid=,pgid=,stat="],
        check=True, text=True, capture_output=True, timeout=5,
    )
    members = []
    for line in census.stdout.splitlines():
        pid, pgid, state = line.split(maxsplit=2)
        if int(pgid) == group and int(pid) != group and not state.startswith("Z"):
            members.append(int(pid))
    return members


def run_bounded(command, label, timeout, *, check=False):
    """Bound the owned process family and retain diagnostics on every failure."""
    with tempfile.TemporaryFile() as diagnostics:
        child = subprocess.Popen(
            command, stdout=diagnostics, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        observed = threading.Event()
        failures = []

        def observe_exit():
            try:
                # WNOWAIT keeps the group leader unreaped. Its PID and PGID
                # cannot be reused while we inspect and, if needed, kill it.
                os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOWAIT)
            except OSError as error:
                failures.append(error)
            finally:
                observed.set()

        observer = threading.Thread(target=observe_exit, daemon=True)
        observer_started = False
        try:
            observer.start()
            observer_started = True
            if not observed.wait(timeout):
                raise TimeoutError(f"timed out after {timeout}s")
            if failures:
                raise RuntimeError(f"could not observe child exit: {failures[0]}")
            survivors = live_group_members(child.pid)
            if survivors:
                raise RuntimeError(f"leader exited with live group members: {survivors}")
        except BaseException as error:
            # The leader is still an owned, unreaped child here. The PGID
            # cannot identify an unrelated process group until we reap it.
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            if observer_started:
                observed.wait(CLEANUP_TIMEOUT_SECONDS)
            try:
                child.wait(timeout=CLEANUP_TIMEOUT_SECONDS)
            except subprocess.TimeoutExpired as cleanup_error:
                diagnostics.seek(0)
                output = diagnostics.read().decode("utf-8", errors="replace")
                raise RuntimeError(
                    f"{label}: {error}; cleanup timed out after "
                    f"{CLEANUP_TIMEOUT_SECONDS}s\n{output}"
                ) from cleanup_error
            diagnostics.seek(0)
            output = diagnostics.read().decode("utf-8", errors="replace")
            if isinstance(error, Exception):
                raise RuntimeError(f"{label}: {error}\n{output}") from error
            raise
        child.wait()
        diagnostics.seek(0)
        output = diagnostics.read().decode("utf-8", errors="replace")
    result = subprocess.CompletedProcess(command, child.returncode, stdout=output)
    if check and result.returncode != 0:
        print(output, end="", flush=True)
        result.check_returncode()
    return result


def read_source(file):
    data = (ROOT / file).read_bytes()
    SOURCE_HASHES[file] = hashlib.sha256(data).hexdigest()
    return data.decode("utf-8").replace("\r\n", "\n").replace("\r", "\n")


REGISTRY = read_source("window_registry.rs")
CORE = read_source("../app_session.rs")
FIXTURES = read_source("window_registry/tests.rs")
PROTOTYPE = read_source("terminal_quit_candidate.rs")
CANDIDATE = read_source("terminal_quit_core_owner_integration_candidate.rs")
ROUTER_FIXTURES = read_source("window_router/tests.rs")
ROUTER = read_source("window_router.rs")
DISPATCH = read_source("terminal_quit_core_dispatch_candidate.rs")
REGISTER = '#[cfg(target_os = "macos")]\n#[path = "../terminal_quit_candidate.rs"]\nmod terminal_quit_candidate;'
REAL_FS = '#[cfg(all(test, not(keld_core_native_renderer_main)))]\n#[path = "terminal_quit_core_real_fs_candidate.rs"]\nmod real_fs;'
REAL_DISPATCH = '#[cfg(all(test, not(keld_core_native_renderer_main)))]\n#[path = "terminal_quit_core_dispatch_candidate.rs"]\nmod real_dispatch;'

def between(text, start, end):
    return text.split(start, 1)[1].split(end, 1)[0]

def method(text, name, indent="    "):
    pattern = (r"^" + indent + r"(?:pub\(super\) |pub\(in crate::app_session\) )?fn " + name
               + r"\(.*?^" + indent + r"}\n")
    match = re.search(pattern, text, re.M | re.S)
    if not match:
        raise ValueError(f"missing unique method {name}")
    return match.group()

def methods(text, names):
    return "".join(method(text, name) for name in names.split())

# Exact definitions from the registry; omit unrelated IPC/native routing types.
registry_types = between(REGISTRY, "use super::PrimaryLinkIdentity;\n", "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub(super) struct AbandonedClose")
phase_types = between(REGISTRY, "#[derive(Clone, Copy)]\nstruct NativeWindow", "/// A bounded snapshot")
phase_types = "#[derive(Clone, Copy)]\nstruct NativeWindow" + phase_types
entry_types = "struct Entry" + between(REGISTRY, "struct Entry", "/// One physical native release")
renderer_types = "struct RendererDocumentState" + between(REGISTRY, "struct RendererDocumentState", "struct Entry")
registry_struct = "pub(super) struct WindowRegistry" + between(REGISTRY, "pub(super) struct WindowRegistry", "impl WindowRegistry {")
registry_impl = REGISTRY.split("impl WindowRegistry {", 1)[1]
entry_impl = between(REGISTRY, "impl Entry {", "pub(super) struct WindowRegistry")
registry_methods = methods(registry_impl, "new mark_ready create native_built native_failed require_ready checked_entry admitted_entry native_close begin_close close_reply close_reply_with_retirement native_released has_live_windows only_opening_windows replay")
entry_methods = methods(entry_impl, "retire_renderer_document adoptable_for view")
capacity = re.search(r"^pub\(super\) const PROPOSED_MAX_CUMULATIVE_WINDOW_IDS:.*;$", REGISTRY, re.M).group()
shutdown_struct = "#[derive(Clone)]\nstruct SessionShutdownState" + between(CORE, "struct SessionShutdownState", "/// The [`SessionShutdownState::quit_admission`]")
shutdown_impl = CORE.split("impl SessionShutdownState {", 1)[1]
shutdown_methods = methods(shutdown_impl, "new cause is_running reserve_guarded accept_reserved_guarded transition_guard stop_reader register_generation_reader begin_tail")
causes = "\n".join(re.search(r"^const SESSION_" + cause + r":.*;$", CORE, re.M).group() for cause in ("RUNNING", "LIFECYCLE_QUIT", "CLI_LEASE_LOST"))
helpers = method(FIXTURES, "owner", "") + method(FIXTURES, "snapshot", "")

# Stand-in value shapes are limited to this projection; no policy or state-machine
# method is implemented by these stubs.
stub = r"""
#![allow(dead_code)]
extern crate self as keld_ipc;
extern crate self as keld_wv;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct CorrelationId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct WebviewId(pub u32);
pub mod wkwebview {
    pub struct MacNativeDocumentStamp;
    pub struct MacNativeDocumentLease;
    pub struct MacNavigationUrl;
}
pub mod window {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct WindowId(pub u32);
    #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct WindowGeneration(pub u64);
    #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct WindowRef { pub id: WindowId, pub incarnation: WindowGeneration }
    #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct Rect { pub x:i32, pub y:i32, pub width:u32, pub height:u32 }
    #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct WindowState { pub bounds:Rect, pub focused:bool, pub maximized:bool, pub full_screen:bool }
    #[derive(Debug, Clone, PartialEq)] pub enum WindowEvent {
        Created { window:WindowRef, state:WindowState, pending_close:Option<u64> },
        CloseRequested { window:WindowRef, close_seq:u64 },
        Destroyed { window:WindowRef },
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum WindowResponse { Created { window:WindowRef } }
    #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum CloseVerdict { Allow, Veto }
}
mod app_session {
use std::sync::{Arc, Condvar, Mutex, atomic::{AtomicU8, AtomicBool, Ordering}};
#[derive(Clone, Copy, PartialEq, Eq)] enum PrimaryLinkIdentity { Fixture }
"""

def generate(tests_path, registry_body):
    return "\n".join((
        stub, causes, shutdown_struct, "impl SessionShutdownState {", shutdown_methods, "}",
        "mod window_registry {",
        "use std::{collections::BTreeMap, fmt};",
        "use keld_ipc::{CorrelationId, window::*}; use keld_wv::WebviewId; use super::PrimaryLinkIdentity;",
        registry_types, phase_types,
        "struct PendingOriginalDocumentDestroy; struct PendingOriginalLoad; struct OriginalWindowSelection;",
        renderer_types, entry_types, capacity, "impl Entry {", entry_methods, "}",
        registry_struct, "impl WindowRegistry {", registry_body, "}",
        "#[path = " + json.dumps(str(tests_path)) + "] mod tests;",
        "}}",
    ))

def verify_projection_result(result, label, *, only=None, expected_panic=None):
    lines = result.stdout.splitlines()
    if expected_panic is None:
        if result.returncode == 0 and all(
            lines.count(f"test {OWNER_TEST_PREFIX}{name} ... ok") == 1
            for name in (DRAIN_CREATE_CASE, CLOSING_CASE)
        ):
            return
        raise RuntimeError(f"{label}: exact baseline tests did not both pass")

    selector = OWNER_TEST_PREFIX + only
    named_panic = rf"thread '{re.escape(selector)}'(?: \([0-9]+\))? panicked at .+:"
    panic_rows = [index for index, line in enumerate(lines)
                  if re.fullmatch(r"thread '.+'(?: \([0-9]+\))? panicked at .+:", line)]
    failure_heading = f"---- {selector} stdout ----"
    if (
        result.returncode == 101
        and lines.count("running 1 test") == 1
        and lines.count(f"test {selector} ... FAILED") == 1
        and lines.count(failure_heading) == 1
        and lines.count(f"    {selector}") == 1
        and sum(line.startswith("test result: FAILED. 0 passed; 1 failed; 0 ignored;")
                for line in lines) == 1
        and len(panic_rows) == 1
    ):
        index = panic_rows[0]
        if (
            lines.index(failure_heading) < index
            and re.fullmatch(named_panic, lines[index])
            and lines[index + 1:index + 2]
            == [f"assertion `left == right` failed: {expected_panic}"]
        ):
            return
    raise RuntimeError(f"{label}: did not observe the exact selected assertion {expected_panic}")


def build_and_run(tmp, label, candidate, registry_body, only=None, expected_panic=None):
    root = tmp / label
    session = root / "app_session"
    tests = session / "window_registry/tests.rs"
    tests.parent.mkdir(parents=True)
    # Reproduce the actual registered relative module paths and Rust visibility.
    # The real socket/FS test is deliberately excluded: this projection cannot
    # build the full Core router or claim its results.
    assert FIXTURES.count(REGISTER) == 1
    assert PROTOTYPE.count(REAL_FS) == 1
    tests.write_text("use super::*;\n" + helpers + "\n" + REGISTER + "\n")
    # Neither actual-router component module can run in a projection.
    assert PROTOTYPE.count(REAL_DISPATCH) == 1
    (session / "terminal_quit_candidate.rs").write_text(PROTOTYPE.replace(REAL_FS, "").replace(REAL_DISPATCH, ""))
    (session / "terminal_quit_core_owner_integration_candidate.rs").write_text(candidate)
    source = root / "projection.rs"
    source.write_text(generate(tests, registry_body))
    binary = root / "probe"
    run_bounded(["rustc", "--edition=2024", "--test", "-D", "warnings", str(source), "-o", str(binary)],
                f"{label} compile", COMPILE_TIMEOUT_SECONDS, check=True)
    command = [str(binary), "--color", "never", "--format", "pretty"]
    if only:
        command.extend(["--exact", OWNER_TEST_PREFIX + only])
    result = run_bounded(command, label, PROJECTION_TIMEOUT_SECONDS)
    expected = 101 if expected_panic is not None else 0
    print(f"=== {label} (exit {result.returncode}, expected {expected}) ===", flush=True)
    print(result.stdout, flush=True)
    verify_projection_result(result, label, only=only, expected_panic=expected_panic)

def compile_dispatch_projection(tmp):
    """Compile the actual draft adapter and guard, not a simulated reader test.

    Only unexercised router/FS/native owner type shapes are substituted. The
    real authenticated component tests still require the full Core crate.
    """
    root = tmp / "dispatch_typecheck"
    session = root / "app_session"
    tests = session / "window_registry/tests.rs"
    tests.parent.mkdir(parents=True)
    registry_observation = method(FIXTURES, "fixture_terminal_observation")
    window_observation = method(ROUTER_FIXTURES, "fixture_terminal_observation")
    selection = method(ROUTER, "original_menu_quit_selected")
    caller_guard = method(CORE, "with_primary_caller_locked", "")
    hook = "type ConditionalQuitFixtureHandler" + between(
        CORE, "type ConditionalQuitFixtureHandler", "/// How a post-Quit drain ended"
    )
    tests.write_text("use super::*;\n" + helpers + "\n" + REGISTER +
                     "\nimpl WindowRegistry {\n" + registry_observation + "}\n")
    (session / "terminal_quit_candidate.rs").write_text(PROTOTYPE.replace(REAL_FS, ""))
    (session / "terminal_quit_core_owner_integration_candidate.rs").write_text(CANDIDATE)
    (session / "terminal_quit_core_dispatch_candidate.rs").write_text(DISPATCH)
    owner_shapes = r"""
use crate::CorrelationId;
use std::{io, sync::{Weak, atomic::AtomicU32}};
use window_registry::{WindowOwner, WindowRegistry};
type HostAppError = io::Error;
fn app_detail(_: &str, detail: impl Into<String>) -> HostAppError {
    io::Error::other(detail.into())
}
#[derive(Clone)]
struct PrimaryCallerContext {
    attempt: u32, identity: PrimaryLinkIdentity, reader_stop: Arc<AtomicBool>,
}
struct ActivePrimaryGeneration {
    attempt: u32, identity: PrimaryLinkIdentity, reader_stop: Arc<AtomicBool>,
}
struct FsDispatchState {
    in_flight: usize, call_outstanding: bool,
    pending_call: Option<()>, failed_write_attempt: Option<u32>,
}
struct FsDispatchSession { state: Mutex<FsDispatchState> }
impl FsDispatchSession {
    fn drain(&self) -> Result<(), HostAppError> {
        unreachable!("compile-only projection; no router or FS execution")
    }
}
struct WindowSession {
    registry: WindowRegistry, native_supported: bool, original_selection: Option<()>,
    subscribed: Option<WindowOwner>, native_quit_ready_attempt: u32,
    native_quit_source_attempt: Arc<AtomicU32>,
}
#[derive(Default)]
struct QuitDrainTestHooks {
    conditional_dispatch: Option<ConditionalQuitFixtureDispatch>,
    conditional_dispatch_used: bool,
}
struct PrimaryRouterHandle {
    current: Arc<Mutex<Option<ActivePrimaryGeneration>>>,
    shutdown: SessionShutdownState,
    windows: Option<Arc<Mutex<WindowSession>>>,
    fs: Option<Weak<FsDispatchSession>>,
    window_ready: Arc<AtomicBool>,
    quit_drain_hooks: Arc<Mutex<QuitDrainTestHooks>>,
}
"""
    generated = generate(tests, registry_methods)
    assert generated.count("enum PrimaryLinkIdentity { Fixture }") == 1
    generated = generated.replace("enum PrimaryLinkIdentity { Fixture }",
                                  "enum PrimaryLinkIdentity { Fixture, Role(u32) }")
    assert generated.count("mod window_registry {") == 1
    generated = generated.replace("mod window_registry {", "\n".join((
        owner_shapes, hook, caller_guard, "impl WindowSession {", selection,
        window_observation, "}", "mod window_registry {",
    )))
    source = root / "dispatch_projection.rs"
    source.write_text(generated)
    run_bounded(["rustc", "--edition=2024", "--test", "--emit=metadata",
                 "-D", "warnings", str(source), "-o", str(root / "dispatch.rmeta")],
                "dispatch projection compile", COMPILE_TIMEOUT_SECONDS, check=True)
    print("=== actual dispatch adapter typecheck (source projection; no Core execution) PASS ===", flush=True)

def main():
    if sys.platform != "darwin":
        raise SystemExit("Run this pinned macOS projection on the macOS owner host.")
    for file, digest in SOURCE_HASHES.items():
        print(digest, file)
    with tempfile.TemporaryDirectory(prefix="gh451-core-owner-w1-") as folder:
        tmp = Path(folder)
        build_and_run(tmp, "exact_projection", CANDIDATE, registry_methods)
        assert CANDIDATE.count("assert!(window.terminal.admit_create());") == 1
        build_and_run(tmp, "mutation_skip_create_invalidation",
                      CANDIDATE.replace("assert!(window.terminal.admit_create());", ""),
                      registry_methods, DRAIN_CREATE_CASE,
                      "GH451_PROJECTION_ASSERT_DRAIN_CREATE_INVALIDATES")
        census = "fn has_live_windows"
        assert registry_methods.count(census) == 1
        before, after = registry_methods.split(census, 1)
        original = "!matches!(entry.phase, Phase::Destroyed(_))"
        assert after.count(original) == 1
        mutation = before + census + after.replace(original, "!matches!(entry.phase, Phase::Destroyed(_) | Phase::Closing(_))")
        build_and_run(tmp, "mutation_ignore_closing", CANDIDATE, mutation,
                      CLOSING_CASE, "GH451_PROJECTION_ASSERT_CLOSING_NOT_EMPTY")
        compile_dispatch_projection(tmp)

if __name__ == "__main__":
    main()
