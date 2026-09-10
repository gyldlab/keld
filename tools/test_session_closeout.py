"""Real filesystem/git negative controls for session closeout admission."""

import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

import session_closeout as checker


class CloseoutTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.git("init", "--quiet")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                 "commit", "--allow-empty", "-m", "baseline", "--quiet")
        self.log = self.root / "proof.txt"
        self.log.write_text("Observed normal window close in the test fixture.\n", encoding="utf-8")
        self.resource = self.root / "temporary-build"
        self.baseline = self.root / "baseline.json"
        self.baseline.write_text(json.dumps({
            "schema": "keld.session-baseline/v1", "session_id": "audit-214",
            "repo": str(self.repo), "objectives": [{"id": "dotnet-audit", "summary": "Compare .NET desktop behaviour"}],
            "findings": [{"id": "shutdown", "summary": "Normal window close is incomplete"}],
            "resources": [{"id": "build", "path": str(self.resource)}],
            "checks": ["fixture behaviour"], "untracked": []}), encoding="utf-8")
        self.receipt = {
            "schema": "keld.session-closeout/v1", "session_id": "audit-214",
            "repo": str(self.repo), "head": self.git("rev-parse", "HEAD"),
            "baseline": self.proof(self.baseline),
            "objectives": [{"id": "dotnet-audit", "summary": "Compare .NET desktop behaviour",
                            "status": "complete", "evidence": [self.proof(self.log)]}],
            "findings": [{"id": "shutdown", "summary": "Normal window close is incomplete",
                          "status": "tracked", "issue": "KEL-185",
                          "owner": "Lifecycle owner", "next_action": "Resolve and rerun normal-close fixture",
                          "remote_receipt": "https://linear.app/gyldlab-keld/issue/KEL-185/close",
                          "evidence": [self.proof(self.log)]}],
            "findings_review": [self.proof(self.log)],
            "resources": [{"id": "build", "path": str(self.resource),
                           "status": "removed", "reason": "Obsolete fixture build removed"}],
            "checks": [{"name": "fixture behaviour", "status": "passed",
                        "source_head": self.git("rev-parse", "HEAD"),
                        "evidence": [self.proof(self.log)]}], "outcome": "complete"}
        self.path = self.root / "receipt.json"

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.repo), *args], check=True,
                              capture_output=True, text=True).stdout.strip()

    def proof(self, path):
        return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}

    def write(self):
        self.path.write_text(json.dumps(self.receipt), encoding="utf-8")

    def reject(self, message):
        self.write()
        with self.assertRaisesRegex(checker.Invalid, message):
            checker.check(self.path)

    def test_valid_cli_is_read_only(self):
        self.write()
        before = {str(p): p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        result = subprocess.run([sys.executable, "-B", str(Path(checker.__file__)), "check", str(self.path)],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("remote state and inventory completeness not authenticated", result.stdout)
        self.assertEqual(before, {str(p): p.read_bytes() for p in self.root.rglob("*") if p.is_file()})

    def test_original_dotnet_objective_cannot_be_replaced(self):
        self.receipt["objectives"][0]["id"] = "later-repair"
        self.reject("dropped baseline objectives")

    def test_baseline_meaning_and_resource_path_cannot_be_substituted(self):
        original = copy.deepcopy(self.receipt)
        for section, field, replacement in (("objectives", "summary", "Different task entirely"),
                                             ("findings", "summary", "Different finding entirely"),
                                             ("resources", "path", str(self.root / "other"))):
            with self.subTest(section=section):
                self.receipt = copy.deepcopy(original)
                self.receipt[section][0][field] = replacement
                self.reject("changed baseline " + section)

    def test_required_check_cannot_be_dropped(self):
        self.receipt["checks"][0]["name"] = "different passing check"
        self.reject("dropped baseline checks")

    def test_dirty_tracked_files_require_explicit_handoff(self):
        tracked = self.repo / "tracked.txt"
        tracked.write_text("baseline", encoding="utf-8")
        self.git("add", "tracked.txt")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                 "commit", "-m", "tracked input", "--quiet")
        self.receipt["head"] = self.git("rev-parse", "HEAD")
        self.receipt["checks"][0]["source_head"] = self.receipt["head"]
        tracked.write_text("uncommitted change", encoding="utf-8")
        self.reject("dirty tracked files")
        self.receipt["outcome"] = "handoff"
        self.reject("dirty tracked files")
        self.receipt["checks"][0].update(status="not-run", owner="Session owner",
                                         next_action="Commit changes and rerun verification")
        self.write()
        self.assertEqual(checker.check(self.path), "handoff")

    def test_new_untracked_file_rejects_complete_but_known_baseline_is_preserved(self):
        extra = self.repo / "forgotten-source.txt"
        extra.write_text("new work", encoding="utf-8")
        self.reject("unexplained untracked files")
        baseline = json.loads(self.baseline.read_text(encoding="utf-8"))
        baseline["untracked"] = [{"path": "forgotten-source.txt", "sha256": self.proof(extra)["sha256"]}]
        self.baseline.write_text(json.dumps(baseline), encoding="utf-8")
        self.receipt["baseline"] = self.proof(self.baseline)
        self.write()
        self.assertEqual(checker.check(self.path), "complete")
        self.assertEqual(extra.read_text(encoding="utf-8"), "new work")

    def test_existing_untracked_source_cannot_change_or_disappear(self):
        source = self.repo / "existing.py"
        source.write_text("assert True\n", encoding="utf-8")
        baseline = json.loads(self.baseline.read_text(encoding="utf-8"))
        baseline["untracked"] = [{"path": "existing.py", "sha256": self.proof(source)["sha256"]}]
        self.baseline.write_text(json.dumps(baseline), encoding="utf-8")
        self.receipt["baseline"] = self.proof(self.baseline)
        self.write()
        self.assertEqual(checker.check(self.path), "complete")
        source.write_text("assert False\n", encoding="utf-8")
        self.reject("untracked file hash mismatch")
        source.unlink()
        self.reject("untracked inventory changed")

    def test_untracked_baseline_cannot_escape_repository(self):
        baseline = json.loads(self.baseline.read_text(encoding="utf-8"))
        for path in ("../outside.py", str(self.log), "nested/../existing.py", "nested\\..\\existing.py"):
            with self.subTest(path=path):
                baseline["untracked"] = [{"path": path, "sha256": self.proof(self.log)["sha256"]}]
                self.baseline.write_text(json.dumps(baseline), encoding="utf-8")
                self.receipt["baseline"] = self.proof(self.baseline)
                self.reject("invalid baseline untracked path")

    def test_old_check_cannot_be_reused_by_only_updating_receipt_head(self):
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                 "commit", "--allow-empty", "-m", "later source", "--quiet")
        self.receipt["head"] = self.git("rev-parse", "HEAD")
        self.reject("stale check source_head")

    def test_implemented_finding_still_requires_tracking_evidence(self):
        self.receipt["findings"][0]["status"] = "implemented"
        self.write()
        self.assertEqual(checker.check(self.path), "complete")
        self.receipt["findings"][0].pop("remote_receipt")
        self.reject("requires remote receipt")

    def test_empty_resource_inventory_and_separate_turn_id(self):
        baseline = json.loads(self.baseline.read_text(encoding="utf-8"))
        baseline["resources"] = []
        self.baseline.write_text(json.dumps(baseline), encoding="utf-8")
        self.receipt.update(resources=[], baseline=self.proof(self.baseline), turn_id="turn-1")
        self.write()
        self.assertEqual(checker.check(self.path), "complete")
        self.receipt["turn_id"] = "../escape"
        self.reject("invalid turn_id")

    def test_tracked_finding_needs_owner_and_real_followup(self):
        self.receipt["findings"][0].pop("owner")
        self.reject("empty or short text")
        self.receipt["findings"][0]["owner"] = "<owner>"
        self.reject("placeholder text")
        self.receipt["findings"][0]["owner"] = "TODO assign lifecycle owner"
        self.reject("placeholder text")

    def test_tracked_finding_needs_matching_remote_receipt(self):
        self.receipt["findings"][0].pop("remote_receipt")
        self.reject("requires remote receipt")
        self.receipt["findings"][0]["remote_receipt"] = "https://linear.app/team/issue/KEL-999"
        self.reject("matching Linear issue")

    def test_fake_deleted_resource(self):
        self.resource.mkdir()
        self.reject("still exists")

    def test_removed_worktree_must_also_be_unregistered(self):
        self.git("worktree", "add", "--detach", str(self.resource), "HEAD")
        self.assertEqual(self.resource.resolve().parent, self.root)
        shutil.rmtree(self.resource)
        self.reject("remains registered worktree")
        self.git("worktree", "prune")
        self.write()
        self.assertEqual(checker.check(self.path), "complete")

    def test_untracked_symlink_requires_handoff(self):
        source = self.repo / "existing.py"
        try:
            source.symlink_to(self.log)
        except OSError:
            self.skipTest("OS does not grant symlink creation")
        baseline = json.loads(self.baseline.read_text(encoding="utf-8"))
        baseline["untracked"] = [{"path": "existing.py", "sha256": self.proof(self.log)["sha256"]}]
        self.baseline.write_text(json.dumps(baseline), encoding="utf-8")
        self.receipt["baseline"] = self.proof(self.baseline)
        self.reject("unsupported untracked reparse/symlink")

    def test_broken_link_is_not_deleted(self):
        try:
            self.resource.symlink_to(self.root / "missing")
        except OSError:
            self.skipTest("OS does not grant symlink creation")
        self.reject("still exists")

    def test_stale_evidence_and_head(self):
        self.log.write_text("Changed evidence", encoding="utf-8")
        self.reject("hash mismatch")
        self.receipt["head"] = "0" * 40
        self.reject("stale git HEAD")

    def test_duplicate_and_unknown_fields(self):
        self.receipt["objectives"].append(copy.deepcopy(self.receipt["objectives"][0]))
        self.reject("duplicate inventory ID")
        self.receipt["objectives"].pop()
        self.receipt["assume_complete"] = True
        self.reject("unknown fields")

    def test_complete_rejects_each_unresolved_atom_but_handoff_accepts(self):
        original = copy.deepcopy(self.receipt)
        for section, status in (("objectives", "deferred"), ("findings", "blocked"),
                                ("resources", "blocked"), ("checks", "not-run")):
            with self.subTest(section=section):
                self.receipt = copy.deepcopy(original)
                row = self.receipt[section][0]
                row.update(status=status, owner="KEL-214 owner", next_action="Rerun named acceptance check")
                self.reject("complete outcome contains unresolved")
                self.receipt["outcome"] = "handoff"
                self.write()
                self.assertEqual(checker.check(self.path), "handoff")
                row.pop("next_action")
                self.reject("empty or short text")

    def test_baseline_hash_binds_inventory(self):
        self.baseline.write_text("{}", encoding="utf-8")
        self.reject("hash mismatch")

    def test_json_duplicate_key_rejected(self):
        self.path.write_text('{"schema":"first","schema":"second"}', encoding="utf-8")
        with self.assertRaisesRegex(checker.Invalid, "duplicate JSON key"):
            checker.check(self.path)

    def test_empty_check_list_and_placeholder_retention(self):
        self.receipt["checks"] = []
        self.reject("checks must be nonempty")
        self.receipt["resources"][0].update(status="retained", reason="TODO")
        self.reject("placeholder")


if __name__ == "__main__":
    unittest.main()
