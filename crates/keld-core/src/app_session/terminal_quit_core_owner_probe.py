"""Draft-only GH451 source projection: run without building KELD or starting native UI.

Copies the pinned production registry/shutdown method bodies and original transaction
prototype verbatim. Minimal IPC/native value types and delivered receipts are fixtures.
This is state/ordering evidence, not full Core integration or physical OS evidence.
"""
from pathlib import Path
import hashlib
import json
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent
REGISTRY = (ROOT / "window_registry.rs").read_text()
CORE = (ROOT.parent / "app_session.rs").read_text()
FIXTURES = (ROOT / "window_registry/tests.rs").read_text()
PROTOTYPE = (ROOT / "terminal_quit_candidate.rs").read_text()
CANDIDATE = (ROOT / "terminal_quit_core_owner_integration_candidate.rs").read_text()

def between(text, start, end):
    return text.split(start, 1)[1].split(end, 1)[0]

def method(text, name, indent="    "):
    pattern = (r"^" + indent + r"(?:pub\(super\) )?fn " + name
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

def generate(candidate_path, registry_body):
    return "\n".join((
        stub, causes, shutdown_struct, "impl SessionShutdownState {", shutdown_methods, "}",
        "mod window_registry {",
        "use std::{collections::BTreeMap, fmt};",
        "use keld_ipc::{CorrelationId, window::*}; use keld_wv::WebviewId; use super::PrimaryLinkIdentity;",
        registry_types, phase_types,
        "struct PendingOriginalDocumentDestroy; struct PendingOriginalLoad; struct OriginalWindowSelection;",
        renderer_types, entry_types, capacity, "impl Entry {", entry_methods, "}",
        registry_struct, "impl WindowRegistry {", registry_body, "}",
        "mod tests { use super::*;", helpers, "mod terminal_quit_candidate {", PROTOTYPE,
        "#[path = " + json.dumps(str(candidate_path)) + "] mod core_owner_integration;",
        "}}}}",
    ))

def build_and_run(tmp, label, candidate, registry_body, only=None, expected=0):
    path = tmp / f"{label}_candidate.rs"
    path.write_text(candidate)
    source = tmp / f"{label}.rs"
    source.write_text(generate(path, registry_body))
    binary = tmp / label
    subprocess.run(["rustc", "--edition=2024", "--test", str(source), "-o", str(binary)], check=True)
    command = [str(binary)]
    if only:
        command.extend(["--exact", "app_session::window_registry::tests::terminal_quit_candidate::core_owner_integration::" + only])
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(f"=== {label} (exit {result.returncode}, expected {expected}) ===", flush=True)
    print(result.stdout, flush=True)
    if result.returncode != expected or (expected != 0 and "1 failed" not in result.stdout):
        raise RuntimeError(f"{label}: unexpected result")

def main():
    if sys.platform != "darwin":
        raise SystemExit("Run this pinned macOS projection on the macOS owner host.")
    for file in ("window_registry.rs", "../app_session.rs", "window_registry/tests.rs", "terminal_quit_candidate.rs", "terminal_quit_core_owner_integration_candidate.rs"):
        path = ROOT / file
        print(hashlib.sha256(path.read_bytes()).hexdigest(), file)
    with tempfile.TemporaryDirectory(prefix="gh451-core-owner-w1-") as folder:
        tmp = Path(folder)
        build_and_run(tmp, "exact_projection", CANDIDATE, registry_methods)
        assert CANDIDATE.count("assert!(window.terminal.admit_create());") == 1
        build_and_run(tmp, "mutation_skip_create_invalidation",
                      CANDIDATE.replace("assert!(window.terminal.admit_create());", ""),
                      registry_methods, "actual_create_during_reversible_drain_cannot_be_hidden_by_build_failure", 101)
        census = "fn has_live_windows"
        assert registry_methods.count(census) == 1
        before, after = registry_methods.split(census, 1)
        original = "!matches!(entry.phase, Phase::Destroyed(_))"
        assert after.count(original) == 1
        mutation = before + census + after.replace(original, "!matches!(entry.phase, Phase::Destroyed(_) | Phase::Closing(_))")
        build_and_run(tmp, "mutation_ignore_closing", CANDIDATE, mutation,
                      "real_registry_close_pending_and_closing_cannot_authorize_terminal", 101)

if __name__ == "__main__":
    main()
