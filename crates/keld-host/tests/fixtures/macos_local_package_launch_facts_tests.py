#!/usr/bin/env python3
"""Portable tests of the real launch-facts oracle wiring, not native acceptance.

Run directly with Python. OS observations are controlled at the existing smoke
observer boundary; the actual driver, bounded logs and clean-exit check run.
"""
from contextlib import ExitStack
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import MagicMock, patch

import macos_local_package_acceptance as smoke
import macos_local_package_launch_facts as facts


class LaunchFactsExitTests(unittest.TestCase):
    def exercise(self, exit_code=0, fatal="", wait_error=None, quit_error=None):
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ("logs", "broker-allowed", "broker-denied"):
                (root / name).mkdir()
            (root / "broker-denied/sentinel.txt").write_bytes(b"GH1404_DENIED_SENTINEL\n")
            app = root / "Example.app"
            host, bun = app / "Contents/MacOS/keld-host", app / "Contents/Helpers/bun"
            roles = [smoke.ProcessFact(100, 1, "1.000001", str(host), False, 1000),
                     smoke.ProcessFact(101, 100, "1.000002", str(host), True, 1000),
                     smoke.ProcessFact(102, 101, "1.000003", str(bun), False, 1000)]
            witness = {"facts": {"early": {}}}
            output = "\n".join([
                facts.FIRST + "{}", facts.BEFORE_READY + json.dumps(witness["facts"]),
                facts.READY + json.dumps({"pid": 102, "ppid": 101, "ready": True,
                    "appPath": str(app / "Contents/Resources/app"),
                    "resourcesPath": str(app / "Contents/Resources"), "execPath": str(bun),
                    "argv": [str(bun), str(app / "Contents/Resources/app/src/main.ts")]}),
                facts.BROKER + json.dumps({"read": "KELD-GUARD002", "write": "KELD-GUARD002"}),
                "GH1404_QUIT_REPLY_CONFIRMED",
            ])
            order = []
            exit_status = {"exit_code": exit_code, "source": "controlled observer"}
            exit_observer = MagicMock()

            def enroll(identity):
                self.assertEqual(identity, roles[0])
                order.append("enroll")
                return exit_observer

            def wait():
                order.append("wait")
                if wait_error:
                    raise RuntimeError(wait_error)
                return exit_status

            def quit(*_):
                order.append("quit")
                if quit_error:
                    raise RuntimeError(quit_error)
                return {"state": "ACCEPTED"}

            def ready(*_):
                (root / "broker-allowed/sentinel.txt").write_bytes(b"GH1404_ALLOWED_WRITE\n")
                (root / "logs/test-app.stdout").write_text(output)
                (root / "logs/test-app.stderr").write_text(fatal)
                return witness

            exit_observer.wait.side_effect = wait
            exit_observer.close.side_effect = lambda: order.append("close")
            evidence = MagicMock()
            evidence.root = root
            with ExitStack() as stack:
                window = stack.enter_context(patch.object(smoke, "WindowObserver"))
                for obj, name, kwargs in [
                    (smoke, "NativeExitStatus", {"side_effect": enroll}),
                    (smoke, "live_roles", {"return_value": roles}),
                    (smoke, "current_exact_role", {"return_value": True}),
                    (smoke, "request_native_quit", {"side_effect": quit}),
                    (smoke, "wait_roles_gone", {"side_effect": lambda *_: order.append("gone")}),
                    (facts, "witness_snapshot", {"return_value": {}}),
                    (facts, "wait_ready", {"side_effect": ready}),
                    (facts, "verify_facts", {}),
                ]:
                    stack.enter_context(patch.object(obj, name, **kwargs))
                try:
                    result = facts.launch_facts(evidence, app, Path("/unused"), root, "test", "test")
                finally:
                    window.return_value.close.assert_called_once()
                    if quit_error:
                        self.assertEqual(order, ["enroll", "quit", "close"])
                    elif wait_error:
                        self.assertEqual(order, ["enroll", "quit", "gone", "wait", "close"])
            self.assertEqual(order, ["enroll", "quit", "gone", "wait", "close"])
            return result

    def test_success_records_exact_host_exit_after_pre_quit_registration(self):
        result = self.exercise()
        self.assertEqual(result["host_exit"]["exit_code"], 0)
        self.assertTrue(result["unsignaled_retirement"])

    def test_process_absence_does_not_hide_nonzero_or_signaled_exit(self):
        for status in (1, -9):
            with self.subTest(status=status), self.assertRaisesRegex(RuntimeError, "did not exit successfully"):
                self.exercise(exit_code=status)

    def test_success_status_does_not_hide_fatal_retirement_log(self):
        for fatal in ("KELD-CORE-037: failed native retirement", "KELD-WV-004: event loop error"):
            with self.subTest(fatal=fatal), self.assertRaisesRegex(RuntimeError, "failed native retirement"):
                self.exercise(fatal=fatal)

    def test_missing_kernel_proof_refuses_and_closes_observer(self):
        with self.assertRaisesRegex(RuntimeError, "missing kernel exit proof"):
            self.exercise(wait_error="missing kernel exit proof")

    def test_quit_failure_closes_preinstalled_observer(self):
        with self.assertRaisesRegex(RuntimeError, "Quit failed"):
            self.exercise(quit_error="Quit failed")


if __name__ == "__main__":
    unittest.main()
