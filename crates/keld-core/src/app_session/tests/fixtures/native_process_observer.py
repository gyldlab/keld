"""KEL-140 observation only: promoted KEL-118 exact-process kqueue primitive.

The Core outer parent owns the control endpoint and both isolated groups.
This fixture owns its queues/tooling. It never signals, reaps or restarts targets.
All waits take the existing caller's bound; there is no observer watchdog.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import select
import socket
import subprocess
import sys

CAPTURE_LIMIT = 262144


class Peer:
    """The retained KEL-118 bounded record responsibility."""

    def __init__(self, connection, token, record_limit):
        self.connection = connection
        self.token = token
        self.record_limit = record_limit
        self.pending = bytearray()

    def send(self, record):
        encoded = json.dumps(dict(record, token=self.token),
                             separators=(",", ":")).encode() + b"\n"
        if len(encoded) > self.record_limit:
            raise ValueError("observer record exceeds its fixed bound")
        self.connection.sendall(encoded)

    def read(self):
        while b"\n" not in self.pending:
            chunk = self.connection.recv(1)
            if not chunk:
                return None
            self.pending.extend(chunk)
            if len(self.pending) > self.record_limit:
                raise ValueError("observer control exceeds its fixed bound")
        raw, _, tail = self.pending.partition(b"\n")
        self.pending = bytearray(tail)
        record = json.loads(raw)
        if record.get("token") != self.token:
            raise ValueError("observer control token mismatch")
        return record


def sdk_flags(bound):
    result = subprocess.run(
        ["/usr/bin/xcrun", "--show-sdk-path"], check=True, capture_output=True,
        text=True, timeout=bound, close_fds=True)
    if len(result.stdout) + len(result.stderr) > CAPTURE_LIMIT:
        raise ValueError("SDK discovery capture exceeds its fixed bound")
    header = (Path(result.stdout.strip()).resolve(strict=True)
              / "usr/include/sys/event.h")
    data = header.read_bytes()
    if len(data) > CAPTURE_LIMIT:
        raise ValueError("SDK event header exceeds its fixed bound")
    text = data.decode("utf-8")
    flags = {}
    for name in ("NOTE_EXITSTATUS", "EV_RECEIPT"):
        values = re.findall(
            r"^#define\s+" + name + r"\s+(0x[0-9a-fA-F]+|[0-9]+)\b",
            text, re.MULTILINE)
        if len(values) != 1:
            raise ValueError("ambiguous SDK declaration: " + name)
        flags[name] = int(values[0], 0)
    return flags, {"path": str(header),
                   "sha256": hashlib.sha256(data).hexdigest(),
                   "flags": flags}


class ProcessExit:
    """One active Python owner of the retained KEL-118 exit-event primitive."""

    def __init__(self, pid, flags):
        self.pid = pid
        self.status_flag = flags["NOTE_EXITSTATUS"]
        self.queue = select.kqueue()
        os.set_inheritable(self.queue.fileno(), False)
        self.terminal = None
        try:
            change = select.kevent(
                pid, filter=select.KQ_FILTER_PROC,
                flags=select.KQ_EV_ADD | select.KQ_EV_ENABLE | flags["EV_RECEIPT"],
                fflags=select.KQ_NOTE_EXIT | self.status_flag)
            receipts = self.queue.control([change], 1, 0)
            if (len(receipts) != 1 or receipts[0].ident != pid or
                    receipts[0].filter != select.KQ_FILTER_PROC or
                    not receipts[0].flags & select.KQ_EV_ERROR or
                    receipts[0].data != 0):
                raise ValueError("exact exit/status registration failed: "
                                 + repr(receipts))
            self.receipt = self.raw_event(receipts[0])
        except BaseException:
            self.queue.close()
            raise

    @staticmethod
    def raw_event(event):
        return {"ident": event.ident, "filter": event.filter,
                "flags": event.flags, "fflags": event.fflags, "data": event.data}

    def poll(self):
        if self.terminal is None:
            events = self.queue.control([], 1, 0)
            if events:
                event = events[0]
                required = select.KQ_NOTE_EXIT | self.status_flag
                if (event.ident != self.pid or
                        event.filter != select.KQ_FILTER_PROC or
                        event.flags & select.KQ_EV_ERROR or
                        event.fflags & required != required):
                    raise ValueError("exact kernel exit/status event rejected: "
                                     + repr(event))
                raw = event.data
                decoded = os.waitstatus_to_exitcode(raw)
                self.terminal = {
                    "pid": self.pid, "raw": raw,
                    "code": decoded if decoded >= 0 else None,
                    "signal": -decoded if decoded < 0 else None,
                    "event": self.raw_event(event)}
        return {"pid": self.pid, "terminal": self.terminal}

    def close(self):
        self.queue.close()


def group_census(group, bound):
    result = subprocess.run(
        ["/bin/ps", "-axo", "pid=,ppid=,pgid=,uid=,state=,comm="],
        check=True, capture_output=True, text=True, timeout=bound, close_fds=True)
    if len(result.stdout) + len(result.stderr) > CAPTURE_LIMIT:
        raise ValueError("full process census exceeds its fixed bound")
    members = []
    parent_row = None
    for line in result.stdout.splitlines():
        fields = line.split(None, 5)
        if len(fields) != 6:
            raise ValueError("incomplete process census row: " + repr(line))
        pid, parent, current_group, uid = map(int, fields[:4])
        row = {"pid": pid, "ppid": parent, "group": current_group,
               "uid": uid, "state": fields[4], "image": fields[5]}
        if pid == os.getppid():
            parent_row = row
        if current_group == group:
            members.append(row)
    return {"group": group, "members": members,
            "parent": parent_row,
            "observer_uid": os.getuid(), "observer_euid": os.geteuid()}


def enroll(request, flags, bound):
    leader, bun, group = (request[name] for name in ("leader", "bun", "group"))
    if (not all(isinstance(pid, int) and pid > 1 for pid in (leader, bun, group))
            or leader != group or leader == bun or
            group == os.getpgrp() or os.getpid() in (leader, bun)):
        raise ValueError("observer enrollment identity mismatch")
    before = group_census(group, bound)
    by_pid = {row["pid"]: row for row in before["members"]}
    if (leader not in by_pid or bun not in by_pid or
            by_pid[leader]["ppid"] != os.getppid() or
            by_pid[bun]["ppid"] != leader or
            any("Z" in by_pid[pid]["state"] for pid in (leader, bun))):
        raise ValueError("enrolled leader/Bun is not the live owned family")
    observers = []
    try:
        observers = [ProcessExit(leader, flags)]
        observers.append(ProcessExit(bun, flags))
        states = [observer.poll() for observer in observers]
        after = group_census(group, bound)
        after_rows = {row["pid"]: row for row in after["members"]}
        identity_fields = ("pid", "ppid", "group", "uid", "image")
        if (any(state["terminal"] is not None for state in states) or
                any(pid not in after_rows or "Z" in after_rows[pid]["state"] or
                    any(after_rows[pid][field] != by_pid[pid][field]
                        for field in identity_fields) for pid in (leader, bun))):
            raise ValueError("enrollment changed across exact registration")
        return observers, {"phase": "prearmed", "leader": leader, "bun": bun,
                           "group": group, "before": before, "after": after,
                           "receipts": [observer.receipt for observer in observers]}
    except BaseException:
        for observer in observers:
            observer.close()
        raise


def held_helper(control, token, record_limit):
    """A genuine helper that survives creator death and observation EOF."""
    connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    hold_reader, hold_writer = socket.socketpair()
    try:
        connection.connect(control)
        peer = Peer(connection, token, record_limit)
        peer.send({"phase": "helper-held", "role": "helper",
                   "pid": os.getpid(), "ppid": os.getppid(), "group": os.getpgrp(),
                   "source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()})
        request = peer.read()
        if request is None:
            # EOF is not a release or a successful family-cleanup observation.
            hold_reader.recv(1)
            return 113
        if request.get("phase") != "release":
            raise ValueError("invalid held-helper release")
        peer.send({"phase": "helper-released"})
        return 0
    finally:
        connection.close()
        hold_reader.close()
        hold_writer.close()


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--held-helper":
        control, token, supplied_limit = sys.argv[2:]
        return held_helper(control, token, int(supplied_limit))
    if sys.platform != "darwin" or not hasattr(select, "kqueue"):
        raise ValueError("real macOS kqueue is required")
    control, token, supplied_bound, supplied_limit = sys.argv[1:]
    bound = float(supplied_bound)
    record_limit = int(supplied_limit)
    if not 0 < bound < float("inf") or record_limit <= 0:
        raise ValueError("invalid caller observation bound")
    flags, header = sdk_flags(bound)
    connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    observers = []
    helper = None
    try:
        connection.connect(control)
        peer = Peer(connection, token, record_limit)
        peer.send({"phase": "observer-ready", "role": "observer",
                   "seq": 0,
                   "pid": os.getpid(), "group": os.getpgrp(),
                   "python": str(Path(sys.executable).resolve(strict=True)),
                   "source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                   "sdk_header": header})
        sequence = 0
        while (request := peer.read()) is not None:
            sequence += 1
            if request.get("seq") != sequence:
                raise ValueError("observer control sequence mismatch")
            phase = request.get("phase")
            if phase == "hold-helper":
                if observers or helper is not None:
                    raise ValueError("helper control requires an empty observer")
                helper = subprocess.Popen(
                    [sys.executable, __file__, "--held-helper",
                     control, token, str(record_limit)],
                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                    close_fds=True)
                peer.send({"phase": phase, "seq": sequence, "helper": helper.pid})
            elif phase == "arm":
                if helper is not None:
                    raise ValueError("helper control cannot enroll Native peers")
                if observers:
                    raise ValueError("duplicate observer enrollment")
                observers, acknowledgment = enroll(request, flags, bound)
                acknowledgment["seq"] = sequence
                peer.send(acknowledgment)
            elif phase in ("poll", "census", "finish"):
                if not observers:
                    raise ValueError("observer used before enrollment")
                reply = {"phase": phase, "seq": sequence,
                         "states": [item.poll() for item in observers]}
                if phase == "census":
                    reply["census"] = group_census(observers[0].pid, bound)
                peer.send(reply)
                if phase == "finish":
                    return 0
            else:
                raise ValueError("unknown observer phase")
    except Exception as error:
        print("KELD_AC7_PROCESS_OBSERVER_FAILURE " + repr(error), file=sys.stderr)
        return 112
    finally:
        # Only this direct fixture/tooling child; never a registered Native PID.
        if helper is not None:
            if helper.poll() is None:
                helper.kill()
            helper.wait()
        for observer in observers:
            observer.close()
        connection.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
