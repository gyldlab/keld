"""KEL-118 product fixture: EOF/EPIPE never supplies guardian cleanup."""
import json
import os
import socket
import sys
import time


def main():
    token = os.environ["KEL118_FIXTURE_TOKEN"]
    deadline = time.monotonic() + 120
    peer = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    # A private live endpoint remains readable only on data/close; its opposite
    # endpoint stays owned here. It holds after controller EOF without spinning.
    hold_reader, hold_writer = socket.socketpair()
    pending = bytearray()

    def budget():
        value = deadline - time.monotonic()
        if value <= 0:
            raise TimeoutError("fixture watchdog; cannot qualify group reap")
        return value

    def stdout_marker(message):
        try:
            os.write(1, message.encode() + b"\n")
            return "written"
        except BrokenPipeError:
            # Landed capture retirement may close the reader. Keep fd1 and the
            # process alive; EPIPE must never make the killpg mutation pass.
            return "broken-pipe"

    def send(value):
        peer.settimeout(budget())
        peer.sendall(json.dumps(value, separators=(",", ":")).encode() + b"\n")

    def report_failure(code, error):
        record = {"phase": "fixture-failure", "pid": os.getpid(),
                  "code": code, "error": type(error).__name__ + ": " + str(error)}
        encoded = json.dumps(record, separators=(",", ":")).encode() + b"\n"
        try:
            peer.settimeout(0.1)
            peer.sendall(encoded)
        except OSError:
            pass  # Original failure/code remains; a retired observer is diagnostic only.
        try:
            os.write(2, b"KEL118_PRODUCT_FIXTURE_FAILURE " + encoded)
        except OSError:
            pass  # Captured reader may already be retired; never convert this to success.

    try:
        peer.settimeout(budget())
        peer.connect(os.environ["KEL118_FIXTURE_CONTROL"])
        send({"phase": "child-ready", "token": token, "pid": os.getpid(),
              "ppid": os.getppid(), "pgid": os.getpgrp(),
              "python": sys.executable, "pythonVersion": sys.version.split()[0]})
        stdout_marker("KEL118_PRODUCT_DESCENDANT_READY")
        while True:
            peer.settimeout(budget())
            chunk = peer.recv(4096)
            if not chunk:
                # No release/EOF exit path: only guardian/fallback termination
                # or the failing watchdog can end this held descendant.
                while True:
                    hold_reader.settimeout(budget())
                    hold_reader.recv(1)
            pending.extend(chunk)
            if len(pending) > 4096:
                raise ValueError("bounded fixture control exceeded")
            while b"\n" in pending:
                raw, _, tail = pending.partition(b"\n")
                pending = bytearray(tail)
                request = json.loads(raw)
                if (set(request) != {"command", "token", "nonce"} or
                        request["command"] != "ping" or request["token"] != token or
                        not isinstance(request["nonce"], str) or len(request["nonce"]) != 32):
                    raise ValueError("fixture control identity/shape mismatch")
                nonce = request["nonce"]
                outcome = stdout_marker("KEL118_PRODUCT_AFTER_PARENT_EXIT " + nonce)
                send({"phase": "child-alive", "pid": os.getpid(),
                      "nonce": nonce, "stdout": outcome})
    except (BrokenPipeError, ConnectionResetError):
        # Losing the observation connection cannot terminate the product child.
        try:
            while True:
                hold_reader.settimeout(budget())
                hold_reader.recv(1)
        except TimeoutError as error:
            report_failure(111, error)
            return 111
    except TimeoutError as error:
        report_failure(111, error)
        return 111  # Distinct from required guardian SIGKILL9; always fails proof.
    except Exception as error:
        report_failure(112, error)
        return 112  # A fixture error is never a successful group cleanup.
    finally:
        peer.close()
        hold_reader.close()
        hold_writer.close()


if __name__ == "__main__":
    raise SystemExit(main())
