#!/usr/bin/env python3
"""GH-1404: real installed hello macOS local-package acceptance.

Run only with independently qualified, already built macOS arm64 executables.
Every artifact and diagnostic stays under a caller-selected NEW evidence directory;
the NEW installed copy stays in one immediate child of ~/Applications. This is an
external test driver, never a packaged executable or installed boot authority.

Example (replace the pins with independent qualification records):
  python3 crates/keld-host/tests/fixtures/macos_local_package_acceptance.py
    --cli /absolute/keld --cli-sha256 64-lowercase-hex
    --host /absolute/keld-host --bun /absolute/bun
    --host-sha256 64-lowercase-hex --bun-sha256 64-lowercase-hex
    --host-version qualified-host-label --bun-version qualified-bun-label
    --evidence /absolute/NEW-evidence
    --install-root "$HOME/Applications/NEW-gh1404-install"

The stock main gains declared test-only Ready and post-Quit-reply witnesses.
Requires an interactive macOS desktop. Native AppleEvent cancellation remains a
separate failed compatibility predicate even when application shutdown completes.
"""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import os
import pathlib
import platform
import plistlib
import re
import secrets
import select
import selectors
import shutil
import signal
import stat
import subprocess
import sys
import time
from dataclasses import asdict, dataclass


APP_NAME = "Keld Package Hello 1404"
PROJECT_NAME = "keld-package-hello"
APP_ID = "dev.gyldlab.macos.acceptance"
WINDOW_TIMEOUT = 20.0
PROCESS_TIMEOUT = 20.0
ROLE_ARG = "--keld-internal-macos-guardian-v1"
HEX_256 = re.compile(r"[0-9a-f]{64}\Z")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def file_hash(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def regular_input(path_text: str, label: str, expected: str | None) -> pathlib.Path:
    path = pathlib.Path(path_text).expanduser()
    require(path.is_absolute(), f"{label} must be an absolute path")
    metadata = path.lstat()
    require(stat.S_ISREG(metadata.st_mode), f"{label} must be a regular non-symlink file")
    require(metadata.st_mode & 0o111 != 0, f"{label} has no execute bit")
    if expected is not None:
        require(HEX_256.fullmatch(expected) is not None, f"{label} SHA-256 is not canonical")
        actual = file_hash(path)
        require(actual == expected, f"{label} SHA-256 mismatch: expected {expected}, actual {actual}")
    return path.resolve(strict=True)


def new_dir(path: pathlib.Path) -> None:
    require(path.is_absolute(), f"directory must be absolute: {path}")
    parent = path.parent
    require(parent.is_dir() and not parent.is_symlink(), f"existing real parent required: {parent}")
    path.mkdir(mode=0o700, exist_ok=False)


@dataclass(frozen=True)
class ProcessFact:
    pid: int
    ppid: int
    start: str
    executable: str
    guardian: bool
    uid: int


class Evidence:
    def __init__(self, root: pathlib.Path) -> None:
        new_dir(root)
        self.root = root
        (root / "logs").mkdir()
        self.count = 0
        self.last_child_pid: int | None = None
        self.owned_roles: list[ProcessFact] = []
        self.status_path = root / "result.json"
        self.write_result({"state": "INCOMPLETE", "stage": "created", "claims": []})

    def record(self, **event: object) -> None:
        with (self.root / "events.jsonl").open("a", encoding="utf-8") as target:
            target.write(json.dumps({"monotonic_ns": time.monotonic_ns(), **event}) + "\n")

    def write_result(self, value: dict[str, object]) -> None:
        temporary = self.root / "result.partial.json"
        temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        os.replace(temporary, self.status_path)

    def command(
        self,
        label: str,
        argv: list[str],
        *,
        cwd: pathlib.Path | None = None,
        timeout: float = 60.0,
        expect_success: bool = True,
        stdin: int | None = None,
        env: dict[str, str] | None = None,
    ) -> tuple[int, str, str]:
        self.count += 1
        stem = f"{self.count:03d}-{label}"
        stdout_file = self.root / "logs" / f"{stem}.stdout"
        stderr_file = self.root / "logs" / f"{stem}.stderr"
        began = time.monotonic()
        with stdout_file.open("xb") as stdout, stderr_file.open("xb") as stderr:
            child = subprocess.Popen(
                argv,
                cwd=cwd,
                stdin=subprocess.DEVNULL if stdin is None else stdin,
                stdout=stdout,
                stderr=stderr,
                env=env,
                close_fds=True,
            )
            self.last_child_pid = child.pid
            expired = False
            try:
                code = child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                expired = True
                child.kill()  # This exact launched tool PID, never a process group.
                code = child.wait(timeout=5)
        self.record(
            step=label,
            argv=argv,
            cwd=str(cwd) if cwd else None,
            exit_code=code,
            timeout=expired,
            elapsed_seconds=round(time.monotonic() - began, 3),
            stdout_file=str(stdout_file),
            stderr_file=str(stderr_file),
            stdout_bytes=stdout_file.stat().st_size,
            stderr_bytes=stderr_file.stat().st_size,
        )
        with stdout_file.open("rb") as source:
            out = source.read(64 * 1024).decode("utf-8", "replace")
        with stderr_file.open("rb") as source:
            err = source.read(64 * 1024).decode("utf-8", "replace")
        require(not expired, f"{label} timed out; see {stderr_file}")
        if expect_success:
            require(code == 0, f"{label} exited {code}; see {stderr_file}: {err[-2000:]}")
        return code, out, err


def process_rows() -> list[tuple[int, int, str, str]]:
    output = subprocess.run(
        ["/bin/ps", "-A", "-ww", "-o", "pid=", "-o", "ppid=", "-o", "lstart=", "-o", "command="],
        check=True,
        capture_output=True,
        text=True,
        timeout=5,
    )
    rows = []
    for line in output.stdout.splitlines():
        fields = line.split(maxsplit=7)
        if len(fields) == 8 and fields[0].isdigit() and fields[1].isdigit():
            rows.append((int(fields[0]), int(fields[1]), " ".join(fields[2:7]), fields[7]))
    return rows


def executable_of(pid: int) -> str | None:
    libproc = ctypes.CDLL("/usr/lib/libproc.dylib")
    method = libproc.proc_pidpath
    method.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32]
    method.restype = ctypes.c_int
    buffer = ctypes.create_string_buffer(4096)
    if method(pid, buffer, ctypes.sizeof(buffer)) <= 0:
        return None  # The process may have exited between ps and proc_pidpath.
    return os.fsdecode(buffer.value)


class ProcBsdInfo(ctypes.Structure):
    # sys/proc_info.h: the SDK C regression checks the actual size and offsets.
    _fields_ = [(key, ctypes.c_uint32) for key in (
        "pbi_flags", "pbi_status", "pbi_xstatus", "pbi_pid", "pbi_ppid",
        "pbi_uid", "pbi_gid", "pbi_ruid", "pbi_rgid", "pbi_svuid", "pbi_svgid", "rfu_1",
    )] + [("pbi_comm", ctypes.c_char * 16), ("pbi_name", ctypes.c_char * 32)] + [
        (key, ctypes.c_uint32) for key in
        ("pbi_nfiles", "pbi_pgid", "pbi_pjobc", "e_tdev", "e_tpgid")
    ] + [("pbi_nice", ctypes.c_int32),
         ("pbi_start_tvsec", ctypes.c_uint64), ("pbi_start_tvusec", ctypes.c_uint64)]


def native_bsd_info(pid: int) -> ProcBsdInfo:
    """Exact SDK-sized kernel identity read; a failed read is UNKNOWN."""
    method = ctypes.CDLL("/usr/lib/libproc.dylib").proc_pidinfo
    method.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_int]
    method.restype = ctypes.c_int
    info = ProcBsdInfo()
    size = ctypes.sizeof(info)
    if (method(pid, 3, 0, ctypes.byref(info), size) != size
            or info.pbi_pid != pid or info.pbi_start_tvusec >= 1_000_000):
        raise RuntimeError(f"unknown kernel birth identity for PID {pid}")
    return info


def native_process_identity(pid: int) -> tuple[int, int, int, int, int, str]:
    """Consecutive kernel identity and executable observations for exact matching."""
    def read() -> tuple[int, int, int, int, int, str]:
        info = native_bsd_info(pid)
        path = executable_of(pid)
        if not path:
            raise RuntimeError(f"unknown executable identity for PID {pid}")
        return (info.pbi_pid, info.pbi_ppid, info.pbi_uid,
                info.pbi_start_tvsec, info.pbi_start_tvusec, path)

    first = read()
    if first != read():
        raise RuntimeError(f"PID {pid} changed identity during native observation")
    return first


def native_process_birth(pid: int) -> tuple[int, int, int]:
    """Identity invariant across exec and reparenting, independent of image."""
    first, second = native_bsd_info(pid), native_bsd_info(pid)
    a = (first.pbi_pid, first.pbi_start_tvsec, first.pbi_start_tvusec)
    b = (second.pbi_pid, second.pbi_start_tvsec, second.pbi_start_tvusec)
    if a != b:
        raise RuntimeError(f"PID {pid} changed birth identity during observation")
    return a


def definitely_exited(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return True
    except PermissionError:
        # A protected or foreign process is not proven absent. The caller must
        # still refuse an unreadable owned candidate; never escalate access.
        return False
    return False


def installed_processes(host: pathlib.Path, bun: pathlib.Path) -> list[ProcessFact]:
    expected_host = str(host.resolve(strict=True))
    expected_bun = str(bun.resolve(strict=True))
    facts = []
    for pid, ppid, start, command in process_rows():
        if "keld-host" not in command and "bun" not in command:
            continue
        try:
            native_pid, native_ppid, uid, sec, usec, executable = native_process_identity(pid)
        except RuntimeError as error:
            if definitely_exited(pid):
                continue  # ps observed an owned candidate that has since exited.
            if expected_host in command or expected_bun in command:
                raise RuntimeError(f"live installed candidate PID {pid} has unknown identity") from error
            continue
        if executable not in (expected_host, expected_bun):
            continue
        require(native_pid == pid and native_ppid == ppid and uid == os.geteuid(),
                f"owned process PID/parent/UID changed or belongs to another user: {pid}")
        facts.append(ProcessFact(pid, native_ppid, f"{sec}.{usec:06d}",
                                 executable, ROLE_ARG in command, uid))
    return facts


def live_roles(host: pathlib.Path, bun: pathlib.Path, evidence: Evidence, phase: str) -> list[ProcessFact]:
    deadline = time.monotonic() + PROCESS_TIMEOUT
    last: list[ProcessFact] = []
    while time.monotonic() < deadline:
        last = installed_processes(host, bun)
        hosts = [p for p in last if p.executable == str(host) and not p.guardian]
        guardians = [p for p in last if p.executable == str(host) and p.guardian]
        buns = [p for p in last if p.executable == str(bun)]
        if (len(hosts), len(guardians), len(buns)) == (1, 1, 1):
            if guardians[0].ppid == hosts[0].pid and buns[0].ppid == guardians[0].pid:
                evidence.record(step=f"{phase}-native-processes", facts=[asdict(p) for p in last])
                evidence.owned_roles.extend(last)
                return last
        time.sleep(0.05)  # Poll observed OS state; elapsed time alone never passes.
    evidence.record(step=f"{phase}-process-timeout", observed=[asdict(p) for p in last])
    raise RuntimeError(f"{phase}: exact Host -> Guardian -> Bun process chain not observed")


def wait_roles_gone(host: pathlib.Path, bun: pathlib.Path, identities: list[ProcessFact],
                    evidence: Evidence, phase: str) -> None:
    deadline = time.monotonic() + PROCESS_TIMEOUT
    present: list[ProcessFact] = []
    unknown: str | None = None
    while time.monotonic() < deadline:
        try:
            present = installed_processes(host, bun)
            survivors = [p for p in identities if current_birth_alive(p)]
        except RuntimeError as error:
            if unknown != str(error):
                evidence.record(step=f"{phase}-native-identity-unknown", reason=str(error))
            unknown = str(error)
        else:
            unknown = None
            if not survivors and not present:
                evidence.record(step=f"{phase}-native-processes-retired", original=[asdict(p) for p in identities])
                return
        time.sleep(0.05)
    evidence.record(step=f"{phase}-still-running", facts=[asdict(p) for p in present], unknown_identity=unknown)
    raise RuntimeError(f"{phase}: owned Host, Guardian or Bun not retired after native UI quit")


def current_exact_role(identity: ProcessFact) -> bool:
    try:
        pid, ppid, uid, sec, usec, executable = native_process_identity(identity.pid)
    except RuntimeError as error:
        if definitely_exited(identity.pid):
            return False
        raise RuntimeError(f"PID {identity.pid} still exists but identity is unknown") from error
    return (pid == identity.pid and ppid == identity.ppid and uid == identity.uid
            and f"{sec}.{usec:06d}" == identity.start
            and executable == identity.executable)


class NativeExitStatus:
    """Observe the exact live Host's kernel exit status, not `open`'s status.

    Apple XNU bsd/sys/event.h defines NOTE_EXITSTATUS as 0x04000000, including
    same-owner processes the observer may signal. Register before requesting
    Quit. The kernel event remains bound to that process after it exits.
    This observer never signals a process or changes application authority.
    """

    NOTE_EXITSTATUS = 0x04000000

    def __init__(self, identity: ProcessFact):
        require(sys.platform == "darwin", "native exit observation requires macOS")
        require(current_exact_role(identity), "exit observer requires the witnessed live process")
        self.identity = identity
        self.queue = select.kqueue()
        try:
            event = select.kevent(
                identity.pid, filter=select.KQ_FILTER_PROC,
                flags=select.KQ_EV_ADD | select.KQ_EV_ONESHOT,
                fflags=select.KQ_NOTE_EXIT | self.NOTE_EXITSTATUS,
            )
            changes = self.queue.control([event], 0, 0)
            require(not changes, "unexpected exit-observer registration result")
            require(current_exact_role(identity), "process changed during exit-observer registration")
        except BaseException:
            self.queue.close()
            raise

    def wait(self, timeout: float = PROCESS_TIMEOUT) -> dict[str, object]:
        require(timeout > 0, "exit observation needs a finite positive deadline")
        events = self.queue.control(None, 1, timeout)
        require(len(events) == 1, "no kernel process-exit status observed")
        event = events[0]
        require(event.ident == self.identity.pid and event.filter == select.KQ_FILTER_PROC
                and not event.flags & select.KQ_EV_ERROR
                and event.fflags & select.KQ_NOTE_EXIT
                and event.fflags & self.NOTE_EXITSTATUS,
                "kernel event does not certify the witnessed process exit status")
        return {"process": asdict(self.identity), "raw_wait_status": event.data,
                "exit_code": os.waitstatus_to_exitcode(event.data),
                "source": "kernel EVFILT_PROC / NOTE_EXITSTATUS registered before Quit"}

    def close(self) -> None:
        self.queue.close()


def require_clean_host_exit(status: dict[str, object], stderr: str) -> None:
    """Kernel success and the owning Host log are independent Quit postconditions."""
    require(status.get("exit_code") == 0, "the exact Host did not exit successfully")
    require("KELD-CORE-037:" not in stderr
            and "KELD-WV-004: event loop error" not in stderr,
            "Host log records failed native retirement; process absence is not clean Quit")


def current_birth_alive(identity: ProcessFact) -> bool:
    """For retirement only: exec/reparent do not retire the original process."""
    try:
        pid, sec, usec = native_process_birth(identity.pid)
    except RuntimeError as error:
        if definitely_exited(identity.pid):
            return False
        raise RuntimeError(f"PID {identity.pid} birth identity is unknown") from error
    return pid == identity.pid and f"{sec}.{usec:06d}" == identity.start


def cleanup_verified_roles(evidence: Evidence, install_root: pathlib.Path) -> None:
    """Emergency cleanup after failure, limited to witnessed exact owned PID/start/path.

    Signal only roles whose identity was authenticated as part of the positive
    process chain and which still execute inside this newly created install root.
    A partial unproven chain is recorded as a blocker, not guessed or killed.
    """
    witnesses = list(reversed(evidence.owned_roles))
    for signo in (signal.SIGTERM, signal.SIGKILL):
        for fact in witnesses:
            if not pathlib.Path(fact.executable).is_relative_to(install_root):
                continue
            try:
                exact = current_exact_role(fact)
            except RuntimeError as error:
                evidence.record(step="failure-cleanup-identity-unknown", pid=fact.pid, reason=str(error))
                continue  # Unknown identity never authorizes a signal.
            if exact:
                try:
                    os.kill(fact.pid, signo)
                    evidence.record(step="failure-cleanup-signal", pid=fact.pid,
                                    start=fact.start, executable=fact.executable,
                                    signal=signal.Signals(signo).name)
                except ProcessLookupError:
                    pass
        deadline = time.monotonic() + 4
        while time.monotonic() < deadline:
            try:
                retired = not any(current_birth_alive(fact) for fact in witnesses)
            except RuntimeError:
                retired = False  # Bounded observation may recover; unknown is not gone.
            if retired:
                evidence.record(step="failure-cleanup-retired", observed_roles=len(witnesses))
                return
            time.sleep(0.05)
    remaining = []
    for fact in witnesses:
        try:
            if current_birth_alive(fact):
                remaining.append(asdict(fact))
        except RuntimeError as error:
            remaining.append({**asdict(fact), "unknown_identity": str(error)})
    if remaining:
        evidence.record(step="failure-cleanup-blocked", still_live=remaining)


class WindowObserver:
    def __init__(self, executable: pathlib.Path, timeout: int = 20) -> None:
        self.timeout = timeout
        self.proc = subprocess.Popen(
            [str(executable), "observe", APP_NAME, "1", str(timeout)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        self.buffer = bytearray()
        try:
            require(self.readline(10) == "READY", "CoreGraphics observer did not arm before launch")
        except BaseException:
            self.close()
            raise

    def readline(self, seconds: float) -> str:
        require(self.proc.stdout is not None, "observer stdout missing")
        deadline = time.monotonic() + seconds
        with selectors.DefaultSelector() as selector:
            selector.register(self.proc.stdout, selectors.EVENT_READ)
            while time.monotonic() < deadline:
                if b"\n" in self.buffer:
                    line, _, rest = self.buffer.partition(b"\n")
                    self.buffer = bytearray(rest)
                    require(len(line) <= 256, "oversized CoreGraphics record")
                    return line.decode("ascii", "strict")
                events = selector.select(max(0, deadline - time.monotonic()))
                if not events:
                    break
                chunk = os.read(self.proc.stdout.fileno(), 256)
                require(bool(chunk), "CoreGraphics observer exited without a record")
                self.buffer.extend(chunk)
                require(len(self.buffer) <= 256, "oversized CoreGraphics record")
        raise RuntimeError("CoreGraphics observer record deadline exceeded")

    def witness(self, pid: int) -> list[int]:
        require(self.proc.stdin is not None, "observer stdin missing")
        self.proc.stdin.write(f"{pid}\n".encode("ascii"))
        self.proc.stdin.flush()
        result = self.readline(self.timeout + 1)
        require(re.fullmatch(r"WINDOWS [0-9]+", result) is not None,
                f"independent CoreGraphics presentation failed: {result}")
        require(self.proc.wait(timeout=5) == 0, "CoreGraphics observer did not exit cleanly")
        return [int(result.split()[1])]

    def witness_no_window(self, pid: int) -> None:
        """Prearmed negative control: exact PID must never present a real window."""
        require(self.proc.stdin is not None, "observer stdin missing")
        self.proc.stdin.write(f"{pid}\n".encode("ascii"))
        self.proc.stdin.flush()
        try:
            unexpected = self.readline(self.timeout + 2)
        except RuntimeError:
            unexpected = None
        require(unexpected is None, f"negative executable presented a native window: {unexpected}")
        require(self.proc.wait(timeout=5) == 3, "negative observer did not complete its no-window deadline")
        require(self.proc.stderr is not None and
                b"never observed on-screen" in self.proc.stderr.read(1024),
                "negative CoreGraphics observer failed for an unrelated reason")

    def close(self) -> None:
        if self.proc.poll() is None:
            self.proc.terminate()  # Only this observer, never the application.
            try:
                self.proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=3)
        for stream in (self.proc.stdin, self.proc.stdout, self.proc.stderr):
            if stream is not None:
                stream.close()


def stage_hello(args: argparse.Namespace, evidence: Evidence) -> tuple[pathlib.Path, pathlib.Path]:
    source = evidence.root / "source"
    source.mkdir()
    evidence.command("create", [str(args.cli), "create", PROJECT_NAME], cwd=source)
    project = source / PROJECT_NAME
    require((project / "src/kipc-transport.ts").is_file(), "canonical separate transport missing")
    require((project / "src/main.ts").is_file(), "stock hello main missing")
    require(b"./kipc-transport.ts" in (project / "src/main.ts").read_bytes(),
            "stock hello must import its separate canonical transport")
    (project / "src/acceptance-resource.txt").write_text("gh1404 installed resource v1\n", encoding="utf-8")
    renderer = project / "index.html"
    html = renderer.read_text(encoding="utf-8")
    expected_title = f"<title>{PROJECT_NAME}</title>"
    require(html.count(expected_title) == 1, "stock renderer title differs from expected scaffold")
    renderer.write_text(html.replace(expected_title, f"<title>{APP_NAME}</title>"), encoding="utf-8")
    (project / "keld.config.ts").write_text(
        'export default { name: "' + APP_NAME + '", id: "' + APP_ID +
        '", version: "1.0.0", entry: "src/main.ts", renderer: "index.html" } as const;\n',
        encoding="utf-8",
    )
    (project / "keld.permissions.jsonc").write_text('{"profile":"legacy"}\n', encoding="utf-8")
    output = evidence.root / "artifacts"
    (project / "keld.build.ts").write_text(
        "export default {\n"
        '  target: "macos-arm64", signing: "adhoc-local", bundleVersion: "1",\n'
        '  minimumSystemVersion: "14.0", resources: ["src"],\n'
        "  host: " + json.dumps({"path": str(args.host), "sha256": args.host_sha256,
                                  "version": args.host_version}, ensure_ascii=False) + ",\n"
        "  bun: " + json.dumps({"path": str(args.bun), "sha256": args.bun_sha256,
                                 "version": args.bun_version}, ensure_ascii=False) + ",\n"
        "  outDir: " + json.dumps(str(output), ensure_ascii=False) + "\n} as const;\n",
        encoding="utf-8",
    )
    evidence.record(step="scaffold-pins", config=str(project / "keld.config.ts"),
                    build=str(project / "keld.build.ts"),
                    canonical_transport_sha256=file_hash(project / "src/kipc-transport.ts"),
                    ordinary_resource_sha256=file_hash(project / "src/acceptance-resource.txt"),
                    policy_sha256=file_hash(project / "keld.permissions.jsonc"))
    return project, output


def inspect_package(evidence: Evidence, output: pathlib.Path, project: pathlib.Path,
                    args: argparse.Namespace) -> tuple[pathlib.Path, pathlib.Path]:
    app = output / f"{APP_NAME}.app"
    dmg = output / f"{APP_NAME}.dmg"
    receipt_path = output / "artifact-receipt.json"
    require(app.is_dir() and not app.is_symlink() and dmg.is_file() and receipt_path.is_file(),
            "published app/DMG/external receipt missing")
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    require(receipt.get("schema") == 1 and receipt.get("artifactClass") == "local-adhoc",
            "unexpected local receipt class")
    require(receipt.get("dmgSha256") == file_hash(dmg), "external receipt DMG digest mismatch")
    require(receipt.get("application", {}).get("id") == APP_ID, "receipt app identity mismatch")
    for label, expected in (("inputHost", args.host_sha256), ("inputBun", args.bun_sha256)):
        require(receipt.get(label, {}).get("file", {}).get("sha256") == expected,
                f"receipt input digest mismatch: {label}")
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    require(info.get("CFBundleExecutable") == "keld-host", "wrong main executable")
    require(info.get("CFBundleIdentifier") == APP_ID, "wrong signed app identity")
    require(info.get("CFBundleName") == APP_NAME, "wrong native app name")
    require(info.get("KELDLocalBundle") == {"schema": 1, "target": "macos-arm64",
                                               "bunSha256": args.bun_sha256},
            "local boot seal metadata differs from exact closed contract")
    host = app / "Contents/MacOS/keld-host"
    bun = app / "Contents/Helpers/bun"
    require(host.is_file() and bun.is_file(), "installed binaries are missing")
    require(stat.S_IMODE(host.stat().st_mode) == 0o755, "installed Host mode differs from 0755")
    require(stat.S_IMODE(bun.stat().st_mode) == 0o755, "installed Bun mode differs from 0755")
    require(file_hash(bun) == args.bun_sha256, "installed Bun differs from qualified original")
    require(file_hash(host) == receipt.get("finalHostSha256"), "signed Host receipt digest mismatch")
    for rel in ("src/main.ts", "src/kipc-transport.ts", "src/acceptance-resource.txt", "index.html"):
        require(file_hash(app / "Contents/Resources/app" / rel) == file_hash(project / rel),
                f"published app resource mismatch: {rel}")
    evidence.command("codesign-app", ["/usr/bin/codesign", "--verify", "--deep", "--strict",
                                        "--verbose=2", str(app)], timeout=50)
    evidence.command("dmg-verify", ["/usr/bin/hdiutil", "verify", "-nocache", str(dmg)], timeout=90)
    evidence.record(step="published-content", dmg_sha256=file_hash(dmg),
                    host_sha256=file_hash(host), bun_sha256=file_hash(bun),
                    receipt_sha256=file_hash(receipt_path))
    return app, dmg


def copy_from_readonly_image(evidence: Evidence, dmg: pathlib.Path,
                             app: pathlib.Path, install_root: pathlib.Path) -> pathlib.Path:
    mountpoint = evidence.root / "readonly-image-mount"
    mountpoint.mkdir()
    device: str | None = None
    try:
        _, plist_text, _ = evidence.command(
            "dmg-attach", ["/usr/bin/hdiutil", "attach", "-readonly", "-nobrowse", "-noautoopen", "-plist",
                           "-mountpoint", str(mountpoint), str(dmg)], timeout=90)
        entities = plistlib.loads(plist_text.encode("utf-8")).get("system-entities", [])
        matches = [item for item in entities if item.get("mount-point") == str(mountpoint)]
        require(len(matches) == 1, "hdiutil did not report one exact owned mountpoint")
        devices = [item.get("dev-entry") for item in entities
                   if isinstance(item.get("dev-entry"), str)
                   and re.fullmatch(r"/dev/disk[0-9]+(?:s[0-9]+)?", item["dev-entry"])]
        device = next((value for value in devices
                       if re.fullmatch(r"/dev/disk[0-9]+", value)), devices[0] if devices else None)
        require(isinstance(device, str),
                "hdiutil returned an invalid exact mounted device")
        require(os.path.ismount(mountpoint), "hdiutil mountpoint is not mounted")
        require(os.statvfs(mountpoint).f_flag & os.ST_RDONLY != 0, "DMG mount is not read-only")
        require((mountpoint / "Applications").is_symlink()
                and os.readlink(mountpoint / "Applications") == "/Applications",
                "DMG Applications shortcut does not point to /Applications")
        image_app = mountpoint / app.name
        require(image_app.is_dir() and not image_app.is_symlink(), "DMG app payload absent")
        new_dir(install_root)
        installed = install_root / app.name
        evidence.command("ditto-install", ["/usr/bin/ditto", str(image_app), str(installed)], timeout=90)
        for rel in ("Contents/MacOS/keld-host", "Contents/Helpers/bun",
                    "Contents/Resources/app/src/main.ts",
                    "Contents/Resources/app/src/kipc-transport.ts"):
            require(file_hash(installed / rel) == file_hash(app / rel),
                    f"DMG round-trip content mismatch: {rel}")
        evidence.command("codesign-installed", ["/usr/bin/codesign", "--verify", "--deep",
                                              "--strict", "--verbose=2", str(installed)])
        evidence.record(step="installed-readonly-dmg", exact_device=device,
                        mounted_at=str(mountpoint), installed_app=str(installed))
        return installed
    finally:
        if device is None:
            # An unsuccessful/garbled attach can still leave a mounted disk.
            # Recover only by the exact DMG path created in this evidence root.
            try:
                evidence.command("dmg-attachment-recovery", ["/usr/bin/hdiutil", "info", "-plist"],
                                 timeout=20)
                info_path = (evidence.root / "logs" /
                             f"{evidence.count:03d}-dmg-attachment-recovery.stdout")
                content = info_path.read_bytes()
                require(len(content) <= 2 * 1024 * 1024, "hdiutil info plist exceeds evidence bound")
                images = plistlib.loads(content).get("images", [])
                attached = [item for item in images if item.get("image-path") == str(dmg)]
                require(len(attached) <= 1, "multiple exact DMG attachments; manual cleanup needed")
                if attached:
                    entities = attached[0].get("system-entities", [])
                    devices = [item.get("dev-entry") for item in entities
                               if isinstance(item.get("dev-entry"), str)
                               and re.fullmatch(r"/dev/disk[0-9]+(?:s[0-9]+)?", item["dev-entry"])]
                    device = next((value for value in devices
                                   if re.fullmatch(r"/dev/disk[0-9]+", value)),
                                  devices[0] if devices else None)
                    require(device is not None, "attachment exists without exact device identity")
                    evidence.record(step="dmg-device-recovered", exact_image=str(dmg), exact_device=device)
            except Exception as error:
                evidence.record(step="dmg-cleanup-recovery-blocked", reason=str(error))
        if device is not None:
            evidence.command("dmg-detach", ["/usr/bin/hdiutil", "detach", device], timeout=45)
            require(not os.path.ismount(mountpoint), "owned disk image remained mounted after detach")


def instrument_hello_lifecycle(project: pathlib.Path, evidence: Evidence) -> tuple[pathlib.Path, str]:
    """Add observers, never an authority input or substitute shutdown path."""
    directory = evidence.root / "ready-witnesses"
    directory.mkdir(mode=0o700)
    nonce = secrets.token_hex(32)
    helper = pathlib.Path(__file__).with_name("macos_local_package_crash_witness.ts")
    shutil.copyfile(helper, project / "src/lifecycle-witness.ts")
    main = project / "src/main.ts"
    original = main.read_text(encoding="utf-8")
    subscribe = "    const windowsClosed = lastWindowClosed(session);"
    close = "    await windowsClosed;"
    quit_call = "    await session.quit();"
    require(all(original.count(needle) == 1 for needle in (subscribe, close, quit_call)),
            "stock lifecycle source differs from the exact observer insertion points")
    source = 'import { crashReadyWitness } from "./lifecycle-witness.ts";\n' + original
    source = source.replace(subscribe, subscribe + "\n    const observeReady = crashReadyWitness(session, "
                            + json.dumps(str(directory)) + ", " + json.dumps(nonce) + ");")
    source = source.replace(close, "    await observeReady(response);\n" + close)
    source = source.replace(quit_call, quit_call + '\n    console.log("GH1404_QUIT_REPLY_CONFIRMED");')
    main.write_text(source, encoding="utf-8")
    evidence.record(step="declared-lifecycle-observers",
                    stock_main_sha256=hashlib.sha256(original.encode()).hexdigest(),
                    instrumented_main_sha256=file_hash(main), helper_sha256=file_hash(helper),
                    witness_directory=str(directory), nonce=nonce,
                    purpose="observe Host Ready plus echo and the real awaited Quit Reply; no fake lifecycle")
    return directory, nonce


def ready_snapshot(directory: pathlib.Path) -> dict[str, tuple[int, int, int, int]]:
    paths = list(directory.iterdir())
    require(len(paths) <= 32, "Ready witness directory exceeded its test bound")
    result = {}
    for path in paths:
        metadata = path.lstat()
        require(stat.S_ISREG(metadata.st_mode), "non-regular Ready witness")
        result[path.name] = (metadata.st_dev, metadata.st_ino, metadata.st_mtime_ns, metadata.st_size)
    return result


def wait_ready_echo(evidence: Evidence, role: ProcessFact, directory: pathlib.Path,
                    nonce: str, before: dict[str, tuple[int, int, int, int]], phase: str) -> None:
    leaf = directory / f"{role.pid}.json"
    deadline = time.monotonic() + PROCESS_TIMEOUT
    while time.monotonic() < deadline:
        require(current_exact_role(role), f"{phase}: Bun exited before the Ready/echo witness")
        identity = ready_snapshot(directory).get(leaf.name)
        if identity is not None and identity != before.get(leaf.name):
            require(identity[3] <= 1024, "Ready witness exceeded 1 KiB")
            value = json.loads(leaf.read_bytes())
            require(ready_snapshot(directory).get(leaf.name) == identity, "Ready witness changed during read")
            require(value == {"nonce": nonce, "pid": role.pid, "ready": True,
                              "echo": {"message": "keld", "count": 1}},
                    "stale or invalid authenticated Ready/echo witness")
            evidence.record(step=phase + "-ready-echo-before-quit", role=asdict(role), witness=value)
            return
        time.sleep(0.05)  # Poll a fresh atomic witness, never pass from elapsed time.
    raise RuntimeError(f"{phase}: authenticated Ready/echo witness was not observed")


def request_native_quit(evidence: Evidence, app: pathlib.Path, host: ProcessFact,
                        phase: str) -> dict[str, object]:
    require(current_exact_role(host), "installed Host identity changed before native quit")
    # The path is an argument, not interpolated AppleScript source. Only this
    # independently observed, newly installed application is addressed.
    script = "on run argv\n  tell application (item 1 of argv) to quit\nend run"
    code, out, err = evidence.command(phase + "-native-command-quit",
        ["/usr/bin/osascript", "-e", script, "--", str(app)], timeout=18, expect_success=False)
    canceled = code == 1 and re.search(r"\(-128\)\s*$", err) is not None
    require(code == 0 or canceled, f"{phase}: unexpected native Quit failure: {err}")
    reply = {"state": "CANCELED" if canceled else "ACCEPTED", "exit_code": code,
             "stdout": out, "stderr": err}
    evidence.record(step=phase + "-native-request-result", **reply)
    # CANCELED is not success. Continue only to observe the independent product
    # shutdown predicate before any failure-cleanup signal can obscure it.
    return reply


def launch_and_quit(evidence: Evidence, app: pathlib.Path, census: pathlib.Path,
                    phase: str, ready_directory: pathlib.Path, nonce: str) -> dict[str, object]:
    host, bun = app / "Contents/MacOS/keld-host", app / "Contents/Helpers/bun"
    stdout = evidence.root / "logs" / f"{phase}-app.stdout"
    stderr = evidence.root / "logs" / f"{phase}-app.stderr"
    for path in (stdout, stderr):
        with path.open("xb"):
            pass
    observer = WindowObserver(census)
    try:
        before = ready_snapshot(ready_directory)
        evidence.command(f"{phase}-launchservices", ["/usr/bin/open", "-n", "--stdout", str(stdout),
                                                      "--stderr", str(stderr), str(app)], timeout=20)
        roles = live_roles(host, bun, evidence, phase)
        host_fact = next(p for p in roles if p.executable == str(host) and not p.guardian)
        bun_fact = next(p for p in roles if p.executable == str(bun))
        ids = observer.witness(host_fact.pid)
        wait_ready_echo(evidence, bun_fact, ready_directory, nonce, before, phase)
        evidence.record(step=f"{phase}-native-window", host=asdict(host_fact), window_ids=ids,
                        app_stdout=str(stdout), app_stderr=str(stderr))
        exit_observer = NativeExitStatus(host_fact)
        try:
            native_reply = request_native_quit(evidence, app, host_fact, phase)
            wait_roles_gone(host, bun, roles, evidence, phase)
            host_exit = exit_observer.wait()
        finally:
            exit_observer.close()
        # Guardian output is retained until shutdown, so test final fresh logs
        # after observing normal native Quit and exact process retirement.
        output = bytearray()
        for path in (stdout, stderr):
            require(path.stat().st_size <= 1024 * 1024, "application log exceeded evidence bound")
            with path.open("rb") as source:
                output.extend(source.read(1024 * 1024))
        require_clean_host_exit(host_exit, stderr.read_text(errors="replace"))
        evidence.record(step=phase + "-kernel-host-success", **host_exit)
        lines = output.splitlines()
        for expected in (b'ipc-echo ok: message="keld" count=1',
                         f"{PROJECT_NAME}: main process ready (IPC echo ok)".encode(),
                         b"GH1404_QUIT_REPLY_CONFIRMED"):
            require(lines.count(expected) == 1,
                    f"{phase}: missing or duplicate genuine lifecycle observation: {expected!r}")
        # onEnd reports IPC-022 for any plain peer close, even when no CALL is
        # outstanding. Preserve the rejection without inventing a lost reply.
        require(b"KELD-IPC-022" not in output,
                f"{phase}: normal quit ended the authenticated link unexpectedly; clean lifecycle is not accepted")
        result = {"phase": phase, "app": str(app), "roles": [asdict(p) for p in roles],
                  "native_quit": native_reply, "host_exit": host_exit,
                  "ready_echo_before_quit": True,
                  "application_quit_reply": True, "unsignaled_retirement": True}
        evidence.record(step=phase + "-application-lifecycle-observed", **result)
        return result
    finally:
        observer.close()


def main() -> int:
    global APP_ID
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for option in ("cli", "cli-sha256", "host", "bun", "host-sha256", "bun-sha256",
                   "host-version", "bun-version", "evidence", "install-root"):
        parser.add_argument("--" + option, required=True)
    args = parser.parse_args()
    require(sys.platform == "darwin" and platform.machine() == "arm64",
            "real acceptance requires native arm64 macOS")
    for field in ("host_version", "bun_version"):
        value = getattr(args, field)
        require(0 < len(value.encode("utf-8")) <= 128 and not any(ord(ch) < 32 for ch in value),
                f"{field} must be one declared, nonempty qualification label")
    args.cli = regular_input(args.cli, "CLI", args.cli_sha256)
    args.host = regular_input(args.host, "Host", args.host_sha256)
    args.bun = regular_input(args.bun, "Bun", args.bun_sha256)
    evidence_path = pathlib.Path(args.evidence).expanduser()
    install_root = pathlib.Path(args.install_root).expanduser()
    APP_ID += ".r" + hashlib.sha256(str(evidence_path).encode("utf-8")).hexdigest()[:12]
    applications = pathlib.Path.home() / "Applications"
    require(applications.is_dir() and not applications.is_symlink(),
            "~/Applications must exist as a real directory")
    require(install_root.parent == applications and install_root.name not in ("", ".", ".."),
            "--install-root must be a new immediate subdirectory of ~/Applications")
    require(evidence_path.is_absolute() and install_root.is_absolute(),
            "--evidence and --install-root must be absolute paths")
    require(not evidence_path.exists() and not install_root.exists(),
            "both evidence and install-root must be NEW paths")
    require(evidence_path != install_root and evidence_path not in install_root.parents
            and install_root not in evidence_path.parents, "evidence and install roots must not overlap")
    evidence = Evidence(evidence_path)
    summary: dict[str, object] = {"state": "INCOMPLETE", "stage": "preflight",
                                  "evidence": str(evidence_path), "installed_root": str(install_root),
                                  "application_id": APP_ID, "launches": []}
    try:
        evidence.record(step="qualified-inputs", cli=str(args.cli), cli_sha256=file_hash(args.cli),
                        host=str(args.host), host_sha256=args.host_sha256, host_version=args.host_version,
                        bun=str(args.bun), bun_sha256=args.bun_sha256, bun_version=args.bun_version,
                        qualification="caller-supplied independent pins; labels DECLARED",
                        application_id=APP_ID)
        help_status, help_out, help_err = evidence.command(
            "open-help", ["/usr/bin/open", "-h"], expect_success=False)
        require(help_status in (0, 1) and "--stdout" in help_out + help_err
                and "--stderr" in help_out + help_err and "[-n]" in help_out + help_err,
                "native open help does not advertise stdout/stderr redirection")
        project, output = stage_hello(args, evidence)
        ready_directory, nonce = instrument_hello_lifecycle(project, evidence)
        summary["stage"] = "build"
        evidence.write_result(summary)
        evidence.command("build", [str(args.cli), "build", "--target", "macos-arm64"],
                         cwd=project, timeout=180)
        app, dmg = inspect_package(evidence, output, project, args)
        summary["stage"] = "install"
        evidence.write_result(summary)
        installed = copy_from_readonly_image(evidence, dmg, app, install_root)
        moved_source = project.with_name(project.name + "-renamed-before-launch")
        os.rename(project, moved_source)
        require(not project.exists(), "original project path remains after rename")
        evidence.record(step="source-renamed-before-launch", old=str(project), new=str(moved_source))
        census_source = pathlib.Path(__file__).with_name("native_window_census.swift")
        census_dir = evidence.root / "native-tools"
        census_dir.mkdir()
        shutil.copyfile(census_source, census_dir / census_source.name)
        census = census_dir / "native-window-census"
        evidence.command("compile-census", ["/usr/bin/xcrun", "swiftc", "-O",
                                             str(census_dir / census_source.name), "-o", str(census)], timeout=120)
        summary["stage"] = "initial-launch"
        evidence.write_result(summary)
        summary["launches"].append(launch_and_quit(evidence, installed, census, "installed", ready_directory, nonce))
        relocated_dir = install_root / "relocated"
        relocated_dir.mkdir()
        relocated = relocated_dir / "Renamed-Installed-Hello.app"
        os.rename(installed, relocated)
        evidence.command("codesign-relocated", ["/usr/bin/codesign", "--verify", "--deep",
                                                "--strict", "--verbose=2", str(relocated)])
        summary["stage"] = "relocated-relaunch"
        evidence.write_result(summary)
        summary["launches"].append(launch_and_quit(evidence, relocated, census, "relocated", ready_directory, nonce))
        native_accepted = all(row["native_quit"]["state"] == "ACCEPTED" for row in summary["launches"])
        summary.update(state="PASS" if native_accepted else "PARTIAL", stage="complete", installed_app=str(relocated),
                       app=str(app), dmg=str(dmg), receipt=str(output / "artifact-receipt.json"),
                       scope="framework-owned hello packaging with declared causal observers, installed application lifecycle and relocation",
                       installed_lifecycle_verified=True, native_quit_accepted=native_accepted,
                       limitations=[] if native_accepted else ["AppleEvent Quit returns cancellation; application-controlled Quit completes separately (GH-451)"],
                       unrun="original draw.io workflows, hostile signed/unsigned negative variants, "
                             "inherited dev lease/strict controls and crash/death matrix")
        evidence.write_result(summary)
        print(json.dumps(summary, indent=2))
        return 0 if native_accepted else 2
    except BaseException as exc:
        summary.update(state="FAIL", reason=f"{type(exc).__name__}: {exc}")
        evidence.write_result(summary)
        evidence.record(step="acceptance-failed-before-cleanup", stage=summary["stage"], reason=str(exc))
        try:
            cleanup_verified_roles(evidence, install_root)
        except Exception as cleanup_error:
            evidence.record(step="failure-cleanup-error", error=str(cleanup_error))
        evidence.write_result(summary)
        evidence.record(step="acceptance-failed", stage=summary["stage"], reason=str(exc))
        print(json.dumps(summary, indent=2), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
