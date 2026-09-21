"""Real Git/process contracts for the local-only agent workspace."""
import json
import os
from pathlib import Path
import hashlib
import socket
import subprocess
import sys
import tempfile
import unittest

TOOL = Path(__file__).with_name("workspace.py").resolve()
REPO = TOOL.parent.parent


class WorkspaceTests(unittest.TestCase):
    def setUp(self):
        parent = REPO / "target" / "workspace-tests"
        parent.mkdir(parents=True, exist_ok=True)
        temporary = tempfile.TemporaryDirectory(prefix="case-", dir=parent)
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "primary with spaces"
        self.root.mkdir()
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Workspace tests")
        self.git("config", "user.email", "workspace@example.invalid")
        (self.root / ".gitignore").write_text("/.keld-work/\n/target/\n", encoding="utf-8")
        self.git("add", ".gitignore")
        self.git("commit", "-m", "baseline")
        self.base = self.git("rev-parse", "HEAD").stdout.strip()
        self.git("update-ref", "refs/remotes/origin/main", self.base)

    def git(self, *args, cwd=None, check=True):
        return subprocess.run(["git", "-C", str(cwd or self.root), *args],
                              capture_output=True, text=True, encoding="utf-8", check=check)

    def cli(self, *args, cwd=None, ok=True, env=None):
        result = subprocess.run([sys.executable, "-B", str(TOOL), *args],
                                cwd=cwd or self.root, capture_output=True,
                                text=True, encoding="utf-8", env=env, timeout=20)
        if ok:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        return result

    def start(self):
        return json.loads(self.cli("start", "kel-245", "probe", "--session", "test-session").stdout)

    def proof(self, path):
        return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}

    def closeout(self, task, proof=None):
        """Create a real minimal receipt; the production validator is the oracle."""
        proof = proof or self.root.parent / "release-proof.txt"
        if not proof.exists():
            proof.write_text("reviewed release in isolated fixture\n", encoding="utf-8")
        directory = self.root / ".git" / "keld-closeout" / "test-session"
        directory.mkdir(parents=True)
        baseline = directory / "baseline.json"
        baseline.write_text(json.dumps({
            "schema": "keld.session-baseline/v1", "session_id": "test-session", "repo": task["path"],
            "objectives": [{"id": "release", "summary": "Release isolated task"}],
            "findings": [], "resources": [], "checks": ["release proof"], "untracked": []}), encoding="utf-8")
        receipt = directory / "release.json"
        head = self.git("rev-parse", "HEAD", cwd=task["path"]).stdout.strip()
        receipt.write_text(json.dumps({
            "schema": "keld.session-closeout/v1", "session_id": "test-session", "turn_id": "release",
            "repo": task["path"], "head": head, "baseline": self.proof(baseline),
            "objectives": [{"id": "release", "summary": "Release isolated task", "status": "complete", "evidence": [self.proof(proof)]}],
            "findings": [], "findings_review": [self.proof(proof)], "resources": [],
            "checks": [{"name": "release proof", "status": "passed", "source_head": head, "evidence": [self.proof(proof)]}],
            "outcome": "complete"}), encoding="utf-8")
        return receipt

    def test_root_does_not_allocate_and_is_identical_from_linked_tree(self):
        expected = self.root / ".keld-work"
        self.assertEqual(Path(self.cli("root").stdout.strip()), expected)
        self.assertFalse(expected.exists())
        task = self.start()
        self.assertEqual(Path(self.cli("root", cwd=task["path"]).stdout.strip()), expected)
        self.assertEqual(self.git("status", "--porcelain").stdout, "")

    def test_start_is_idempotent_but_different_session_cannot_take_over(self):
        first = self.start()
        self.assertEqual(first, self.start())
        self.cli("start", "kel-245", "probe", "--session", "other", ok=False)
        self.assertEqual(first, self.start())

    def test_default_base_is_origin_main_not_invoking_branch(self):
        self.git("commit", "--allow-empty", "-m", "local only")
        task = self.start()
        self.assertEqual(task["base"], self.base)
        self.assertEqual(self.git("rev-parse", "HEAD", cwd=task["path"]).stdout.strip(), self.base)

    def test_missing_and_invalid_base_refuse_before_allocation(self):
        self.git("update-ref", "-d", "refs/remotes/origin/main")
        self.cli("start", "kel-245", "probe", "--session", "test", ok=False)
        self.assertFalse((self.root / ".keld-work").exists())
        self.cli("start", "kel-245", "probe", "--session", "test", "--base", "--help", ok=False)
        self.assertFalse((self.root / ".keld-work").exists())
        task = json.loads(self.cli("start", "kel-245", "probe", "--session", "test", "--base", self.base).stdout)
        self.assertEqual(task["base"], self.base)

    def test_bad_names_refuse_before_writes(self):
        for issue, slug, session in [("kel-0", "ok", "s"), ("kel-1", "../out", "s"),
                                     ("kel-1", "Upper", "s"), ("kel-1", "ok", "../s"),
                                     ("kel-1", "ok", "CON"), ("kel-1", "a" * 70, "s")]:
            with self.subTest(issue=issue, slug=slug, session=session):
                self.cli("start", issue, slug, "--session", session, ok=False)
        self.assertFalse((self.root / ".keld-work").exists())

    def test_existing_unmanaged_target_is_never_adopted_or_overwritten(self):
        target = self.root / ".keld-work" / "worktrees" / "kel-245-probe"
        target.mkdir(parents=True)
        (target / "keep").write_text("keep", encoding="utf-8")
        self.cli("start", "kel-245", "probe", "--session", "test", ok=False)
        self.assertEqual((target / "keep").read_text(), "keep")

    def test_bare_repository_and_symlink_root_refuse(self):
        bare = self.root.parent / "bare"
        self.git("init", "--bare", str(bare))
        self.cli("root", cwd=bare, ok=False)
        outside = self.root.parent / "outside"
        outside.mkdir()
        try:
            (self.root / ".keld-work").symlink_to(outside, target_is_directory=True)
        except OSError as error:
            self.skipTest("OS did not permit test symlink: " + str(error))
        self.cli("start", "kel-245", "probe", "--session", "test", ok=False)
        self.assertEqual(list(outside.iterdir()), [])

    def test_force_added_workspace_file_is_rejected(self):
        managed = self.root / ".keld-work"
        managed.mkdir()
        (managed / "bad.txt").write_text("private", encoding="utf-8")
        self.git("add", "-f", ".keld-work/bad.txt")
        self.cli("check", ok=False)

    def test_linked_tree_cannot_have_its_own_workspace(self):
        task = self.start()
        (Path(task["path"]) / ".keld-work").mkdir()
        self.cli("check", cwd=task["path"], ok=False)
        self.cli("run", task["task"], "--session", "test-session", "--",
                 sys.executable, "-c", "print('must not run')", ok=False)
        self.assertFalse((self.root / ".keld-work" / "sessions").exists())

    def test_primary_run_rejects_force_added_target_workspace_content(self):
        task = self.start()
        local = Path(task["path"]) / ".keld-work"
        local.mkdir()
        (local / "private.txt").write_text("preserve", encoding="utf-8")
        self.git("add", "-f", ".keld-work/private.txt", cwd=task["path"])
        # Even if the local path disappears, its tracked index entry remains invalid.
        (local / "private.txt").unlink()
        local.rmdir()
        self.cli("run", task["task"], "--session", "test-session", "--",
                 sys.executable, "-c", "print('must not run')", ok=False)
        self.assertFalse((self.root / ".keld-work" / "sessions").exists())

    def test_status_does_not_create_storage_and_sizes_are_explicit(self):
        info = json.loads(self.cli("status").stdout)
        self.assertEqual(info["tasks"], [])
        self.assertFalse((self.root / ".keld-work").exists())
        task = self.start()
        ordinary = json.loads(self.cli("status").stdout)
        self.assertNotIn("bytes", ordinary["tasks"][0])
        sized = json.loads(self.cli("status", "--sizes").stdout)
        self.assertGreater(sized["tasks"][0]["bytes"], 0)
        self.assertEqual(ordinary["tasks"][0]["task"], task["task"])

    def test_run_preserves_argv_temp_and_child_failure(self):
        task = self.start()
        script = "import os,sys,json; print(json.dumps(dict(cwd=os.getcwd(), temp=os.environ['TMPDIR'], args=sys.argv[1:]))); sys.exit(7)"
        sentinel = "quoted space; $() & Unicode-λ"
        result = self.cli("run", task["task"], "--session", "test-session", "--",
                          sys.executable, "-c", script, sentinel, ok=False)
        self.assertEqual(result.returncode, 7, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["args"], [sentinel])
        self.assertEqual(Path(payload["cwd"]), Path(task["path"]))
        self.assertTrue(Path(payload["temp"]).is_relative_to(self.root / ".keld-work" / "sessions" / "test-session" / "scratch"))
        records = list((self.root / ".keld-work" / "sessions" / "test-session" / "evidence").glob("*/result.json"))
        self.assertEqual(len(records), 1)
        evidence = json.loads(records[0].read_text())
        self.assertEqual(evidence["exit_code"], 7)
        self.assertEqual(evidence["log_limit_bytes"], 4 * 1024 * 1024)
        self.assertNotIn("args", evidence)
        self.assertNotIn("environment", evidence)
        self.assertFalse(any((self.root / ".keld-work" / "worktrees").glob("*.lock")))

    def test_run_log_cap_is_real_and_records_omitted_bytes(self):
        task = self.start()
        volume = 1024 * 1024 + 37
        result = self.cli("run", task["task"], "--session", "test-session", "--log-limit-mib", "1", "--",
                          sys.executable, "-c", f"import sys; sys.stdout.buffer.write(b'x'*{volume})")
        self.assertEqual(len(result.stdout), volume)
        directory = next((self.root / ".keld-work" / "sessions" / "test-session" / "evidence").iterdir())
        metadata = json.loads((directory / "result.json").read_text())
        self.assertEqual((directory / "stdout.log").stat().st_size, 1024 * 1024)
        self.assertEqual(metadata["stdout"]["total_bytes"], volume)
        self.assertEqual(metadata["stdout"]["omitted_bytes"], 37)
        self.assertTrue(metadata["stdout"]["truncated"])

    def test_invalid_limits_and_session_cannot_run_child(self):
        task = self.start()
        for value in ["0", "65", "unlimited", "1.5"]:
            self.cli("run", task["task"], "--session", "test-session", "--log-limit-mib", value,
                     "--", sys.executable, "-c", "raise SystemExit(0)", ok=False)
        self.cli("run", task["task"], "--session", "other", "--", sys.executable, "-c", "pass", ok=False)
        self.assertFalse((self.root / ".keld-work" / "sessions").exists())

    def test_maximum_log_limit_is_accepted(self):
        task = self.start()
        self.cli("run", task["task"], "--session", "test-session", "--log-limit-mib", "64", "--",
                 sys.executable, "-c", "print('maximum')")
        record = next((self.root / ".keld-work" / "sessions" / "test-session" / "evidence").glob("*/result.json"))
        self.assertEqual(json.loads(record.read_text())["log_limit_bytes"], 64 * 1024 * 1024)

    def test_live_second_writer_is_refused_then_next_run_succeeds(self):
        task = self.start()
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            listener.settimeout(10)
            script = f"import socket; s=socket.create_connection(('127.0.0.1',{listener.getsockname()[1]})); s.sendall(b'R'); s.recv(1); s.close()"
            process = subprocess.Popen([sys.executable, "-B", str(TOOL), "run", task["task"],
                                        "--session", "test-session", "--", sys.executable, "-c", script],
                                       cwd=self.root, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                connection, _ = listener.accept()
                with connection:
                    connection.settimeout(10)
                    self.assertEqual(connection.recv(1), b"R")
                    refused = self.cli("run", task["task"], "--session", "test-session", "--",
                                       sys.executable, "-c", "print('must not run')", ok=False)
                    self.assertIn("operation lock exists", refused.stderr)
                    self.assertNotIn("must not run", refused.stdout)
                    self.cli("reference-run", "--", sys.executable, "-c", "print('must not run')",
                             env=dict(os.environ, KELD_WORK_SESSION="test-session"), ok=False)
                    connection.sendall(b"X")
                _, errors = process.communicate(timeout=10)
                self.assertEqual(process.returncode, 0, errors)
            finally:
                if process.poll() is None:
                    process.kill()
                process.communicate(timeout=10)
        self.cli("run", task["task"], "--session", "test-session", "--", sys.executable, "-c", "pass")

    def test_reference_guard_blocks_task_admission_until_completion(self):
        task = self.start()
        own = dict(os.environ, KELD_WORK_SESSION="test-session")
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            listener.settimeout(10)
            script = f"import socket,sys; s=socket.create_connection(('127.0.0.1',{listener.getsockname()[1]})); s.sendall(b'R'); s.recv(1); s.close(); sys.exit(7)"
            process = subprocess.Popen([sys.executable, "-B", str(TOOL), "reference-run", "--",
                                        sys.executable, "-c", script], cwd=self.root, env=own,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                connection, _ = listener.accept()
                with connection:
                    connection.settimeout(10)
                    self.assertEqual(connection.recv(1), b"R")
                    self.cli("start", "kel-245", "other", "--session", "other", ok=False)
                    self.cli("run", task["task"], "--session", "test-session", "--",
                             sys.executable, "-c", "print('must not run')", ok=False)
                    self.cli("reference-run", "--", sys.executable, "-c", "pass", env=own, ok=False)
                    connection.sendall(b"X")
                _, errors = process.communicate(timeout=10)
                self.assertEqual(process.returncode, 7, errors)
            finally:
                if process.poll() is None:
                    process.kill()
                process.communicate(timeout=10)
        self.assertFalse((self.root / ".keld-work" / "reference.lock").exists())
        self.cli("run", task["task"], "--session", "test-session", "--", sys.executable, "-c", "pass")

    def test_inherited_pipe_is_reported_incomplete_without_hanging(self):
        task = self.start()
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            listener.settimeout(10)
            descendant = f"import socket; s=socket.create_connection(('127.0.0.1',{listener.getsockname()[1]})); s.sendall(b'R'); s.recv(1); s.close()"
            script = "import subprocess,sys; subprocess.Popen([sys.executable,'-c'," + repr(descendant) + "])"
            process = subprocess.Popen([sys.executable, "-B", str(TOOL), "run", task["task"],
                                        "--session", "test-session", "--", sys.executable, "-c", script],
                                       cwd=self.root, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                connection, _ = listener.accept()
                with connection:
                    connection.settimeout(10)
                    self.assertEqual(connection.recv(1), b"R")
                    # The descendant still owns the pipe. Runner completion must not depend on its exit.
                    _, errors = process.communicate(timeout=10)
                    self.assertEqual(process.returncode, 125, errors)
                    connection.sendall(b"X")
                    self.assertEqual(connection.recv(1), b"")
            finally:
                if process.poll() is None:
                    process.kill()
                process.communicate(timeout=10)
        record = next((self.root / ".keld-work" / "sessions" / "test-session" / "evidence").glob("*/result.json"))
        evidence = json.loads(record.read_text())
        self.assertEqual(evidence["state"], "capture-incomplete")
        self.assertEqual(evidence["exit_code"], 0)
        self.cli("run", task["task"], "--session", "test-session", "--", sys.executable, "-c", "pass")

    def test_foreign_metadata_and_operation_lock_refuse(self):
        task = self.start()
        lock = self.root / ".keld-work" / "worktrees" / (task["task"] + ".lock")
        lock.write_text("another owner", encoding="utf-8")
        self.cli("run", task["task"], "--session", "test-session", "--", sys.executable, "-c", "pass", ok=False)
        self.assertEqual(lock.read_text(), "another owner")

    def test_foreign_record_cannot_redirect_a_run(self):
        task = self.start()
        record = self.root / ".keld-work" / "worktrees" / (task["task"] + ".json")
        content = json.loads(record.read_text())
        content["path"] = "../outside"
        record.write_text(json.dumps(content), encoding="utf-8")
        self.cli("run", task["task"], "--session", "test-session", "--", sys.executable, "-c", "pass", ok=False)
        self.assertFalse((self.root / ".keld-work" / "sessions").exists())

    def test_reference_commands_cannot_duplicate_a_linked_checkout(self):
        self.assertEqual(Path(self.cli("reference-root").stdout.strip()), self.root)
        task = self.start()
        self.cli("reference-root", cwd=task["path"], ok=False)
        self.cli("reference-root", ok=False)
        self.assertFalse((Path(task["path"]) / "competitors").exists())
        self.assertFalse((Path(task["path"]) / "docs" / "research").exists())

    def test_primary_reference_operation_accepts_only_reconciled_owning_session(self):
        task = self.start()
        own = dict(os.environ, KELD_WORK_SESSION="test-session")
        self.assertEqual(Path(self.cli("reference-root", env=own).stdout.strip()), self.root)
        self.cli("reference-root", env=dict(own, KELD_WORK_SESSION="other"), ok=False)
        lock = self.root / ".keld-work" / "worktrees" / (task["task"] + ".lock")
        lock.write_text("active operation", encoding="utf-8")
        self.cli("reference-root", env=own, ok=False)
        lock.unlink()
        self.cli("start", "kel-245", "other", "--session", "another-session")
        self.cli("reference-root", env=own, ok=False)

    def test_spawn_failure_is_recorded_and_lock_is_released(self):
        task = self.start()
        result = self.cli("run", task["task"], "--session", "test-session", "--", "nonexistent-keld-workspace-executable", ok=False)
        record = next((self.root / ".keld-work" / "sessions" / "test-session" / "evidence").glob("*/result.json"))
        evidence = json.loads(record.read_text())
        self.assertEqual(evidence["state"], "spawn-failed")
        self.assertEqual(evidence["runner_exit_code"], result.returncode)
        self.assertFalse(any((self.root / ".keld-work" / "worktrees").glob("*.lock")))

    def test_run_uses_one_workspace_from_linked_cwd(self):
        task = self.start()
        self.cli("run", task["task"], "--session", "test-session", "--",
                 sys.executable, "-c", "print('linked')", cwd=task["path"])
        self.assertFalse((Path(task["path"]) / ".keld-work").exists())
        self.assertEqual(len(list((self.root / ".keld-work" / "sessions" / "test-session" / "evidence").iterdir())), 1)

    def test_finish_requires_real_closeout_and_clean_preview_is_read_only(self):
        task = self.start()
        self.cli("finish", task["task"], "--session", "test-session", "--receipt", str(self.root / "missing.json"), ok=False)
        receipt = self.closeout(task)
        self.cli("finish", task["task"], "--session", "test-session", "--receipt", str(receipt))
        scratch = self.root / ".keld-work" / "sessions" / "test-session" / "scratch" / "old-run"
        scratch.mkdir(parents=True)
        (scratch / "remove.txt").write_text("scratch", encoding="utf-8")
        evidence = self.root / ".keld-work" / "sessions" / "test-session" / "evidence" / "keep"
        evidence.mkdir(parents=True)
        (evidence / "proof.txt").write_text("evidence", encoding="utf-8")
        before = {str(item): item.read_bytes() for item in self.root.rglob("*") if item.is_file()}
        preview = json.loads(self.cli("clean", task["task"], "--session", "test-session").stdout)
        self.assertFalse(preview["applied"])
        self.assertEqual(preview["targets"], [str(scratch.parent)])
        self.assertEqual(before, {str(item): item.read_bytes() for item in self.root.rglob("*") if item.is_file()})
        self.cli("clean", task["task"], "--session", "test-session", "--apply")
        self.assertFalse(scratch.parent.exists())
        self.assertTrue((evidence / "proof.txt").exists())
        self.assertTrue(Path(task["path"]).exists())

    def test_finish_and_clean_refuse_dirty_or_hostile_targets(self):
        task = self.start()
        (Path(task["path"]) / "untracked.txt").write_text("preserve", encoding="utf-8")
        receipt = self.closeout(task)
        self.cli("finish", task["task"], "--session", "test-session", "--receipt", str(receipt), ok=False)
        (Path(task["path"]) / "untracked.txt").unlink()
        self.cli("finish", task["task"], "--session", "test-session", "--receipt", str(receipt))
        scratch = self.root / ".keld-work" / "sessions" / "test-session" / "scratch"
        outside = self.root.parent / "outside-sentinel"
        outside.mkdir()
        (outside / "keep.txt").write_text("keep", encoding="utf-8")
        try:
            scratch.symlink_to(outside, target_is_directory=True)
        except OSError as error:
            self.skipTest("OS did not permit test symlink: " + str(error))
        self.cli("clean", task["task"], "--session", "test-session", "--apply", ok=False)
        self.assertEqual((outside / "keep.txt").read_text(), "keep")

    def test_clean_refuses_scratch_referenced_by_its_closeout(self):
        task = self.start()
        scratch = self.root / ".keld-work" / "sessions" / "test-session" / "scratch"
        scratch.mkdir(parents=True)
        proof = scratch / "referenced-proof.txt"
        proof.write_text("must survive", encoding="utf-8")
        self.cli("finish", task["task"], "--session", "test-session", "--receipt", str(self.closeout(task, proof)))
        self.cli("clean", task["task"], "--session", "test-session", "--apply", ok=False)
        self.assertEqual(proof.read_text(), "must survive")


if __name__ == "__main__":
    unittest.main()
