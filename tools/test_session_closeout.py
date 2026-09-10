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

    def test_just_receipt_path_is_data_not_shell_source(self):
        just = shutil.which('just')
        if not just:
            self.skipTest('Just executable is required for the local recipe probe')
        original = Path(__file__).resolve().parent.parent / 'justfile'
        (self.repo / 'justfile').write_bytes(original.read_bytes())
        (self.repo / 'tools').mkdir()
        (self.repo / 'tools/session_closeout.py').write_text(
            'import sys; print(sys.argv[-1])', encoding='utf-8')
        literal = str(self.root / 'receipt$(printf CORRUPTED).json')
        result = subprocess.run([just, '--justfile', str(self.repo / 'justfile'),
                                 'session-closeout', literal], cwd=self.repo,
                                capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), literal)

    def test_unicode_checkout_path_uses_git_utf8_metadata(self):
        renamed = self.root / 'repo-\u00e9'
        self.repo.rename(renamed)
        self.repo = renamed
        self.receipt['repo'] = str(renamed)
        baseline = json.loads(self.baseline.read_text(encoding='utf-8'))
        baseline['repo'] = str(renamed)
        self.baseline.write_text(json.dumps(baseline), encoding='utf-8')
        self.receipt['baseline'] = self.proof(self.baseline)
        self.write()
        self.assertEqual(checker.check(self.path), 'complete')

    def test_short_stable_ids_are_valid(self):
        baseline = json.loads(self.baseline.read_text(encoding='utf-8'))
        baseline['objectives'][0]['id'] = 'fs'
        self.receipt['objectives'][0]['id'] = 'fs'
        self.baseline.write_text(json.dumps(baseline), encoding='utf-8')
        self.receipt['baseline'] = self.proof(self.baseline)
        self.write()
        self.assertEqual(checker.check(self.path), 'complete')

    def test_literal_task_terms_are_not_missing_values(self):
        for text in ('Remove TODO markers', 'Audit the doc-placeholder-checker',
                     str(self.root / 'doc-placeholder-checker'), 'Handle <script> elements'):
            with self.subTest(text=text):
                checker.meaningful(text)
        for text in ('', 'TODO', 'TBD', '<owner>', '...'):
            with self.subTest(marker=text), self.assertRaises(checker.Invalid):
                checker.meaningful(text)

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
        self.reject("empty text")
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

    def removed_task_receipt(self):
        task = self.root / "owned-task"
        self.git("worktree", "add", "-b", "task-closeout", str(task), "HEAD")
        baseline = json.loads(self.baseline.read_text(encoding="utf-8"))
        baseline.update(repo=str(task), git_common_dir=str(self.repo / ".git"),
                        source_ref="refs/heads/task-closeout")
        baseline["resources"].append({"id": "task", "path": str(task)})
        self.baseline.write_text(json.dumps(baseline), encoding="utf-8")
        self.receipt.update(repo=str(task), baseline=self.proof(self.baseline))
        self.receipt["resources"].append({"id": "task", "path": str(task), "status": "removed",
                                          "reason": "Owned task source retained in named branch"})
        return task

    def test_removed_task_receipt_retains_source_proof(self):
        task = self.removed_task_receipt()
        self.git("worktree", "remove", str(task))
        self.write()
        self.assertEqual(checker.check(self.path), "complete")

    def removed_task_with_original_notes(self, content=b"Original user-owned notes\n"):
        task = self.removed_task_receipt()
        notes = task / "user-notes.txt"
        notes.write_bytes(content)
        baseline = json.loads(self.baseline.read_text(encoding="utf-8"))
        baseline["untracked"] = [{"path": "user-notes.txt", "sha256": self.proof(notes)["sha256"]}]
        self.baseline.write_text(json.dumps(baseline), encoding="utf-8")
        self.receipt["baseline"] = self.proof(self.baseline)
        preserved = self.root / "preserved-notes.txt"
        preserved.write_bytes(notes.read_bytes())
        notes.unlink()
        self.git("worktree", "remove", str(task))
        return preserved

    def test_removed_task_cannot_lose_baseline_untracked_bytes(self):
        self.removed_task_with_original_notes()
        self.reject("preserved untracked coverage mismatch")

    def test_removed_task_accepts_only_exact_external_preservation(self):
        preserved = self.removed_task_with_original_notes()
        self.receipt["preserved_untracked"] = [{"path": "user-notes.txt", "evidence": self.proof(preserved)}]
        self.write()
        self.assertEqual(checker.check(self.path), "complete")
        preserved.write_bytes(b"Changed bytes\n")
        self.reject("evidence hash mismatch")
        self.receipt["preserved_untracked"][0]["evidence"] = self.proof(preserved)
        self.reject("preserved untracked digest mismatch")

    def test_removed_task_preserves_empty_files_without_empty_proofs(self):
        preserved = self.removed_task_with_original_notes(b'')
        self.receipt['preserved_untracked'] = [{'path': 'user-notes.txt', 'evidence': self.proof(preserved)}]
        self.write()
        self.assertEqual(checker.check(self.path), 'complete')
        with self.assertRaisesRegex(checker.Invalid, 'empty evidence file'):
            checker.evidence(self.proof(preserved))

    def test_preserved_untracked_rejects_duplicates_and_internal_copy(self):
        preserved = self.removed_task_with_original_notes()
        row = {"path": "user-notes.txt", "evidence": self.proof(preserved)}
        self.receipt["preserved_untracked"] = [row, copy.deepcopy(row)]
        self.reject("duplicate preserved untracked path")
        self.receipt["preserved_untracked"] = [row]
        row["evidence"]["path"] = str(Path(self.receipt["repo"]) / "archived.txt")
        self.reject("preserved copy must be outside task checkout")

    def test_external_preservation_does_not_waive_live_original_bytes(self):
        source = self.repo / "user-notes.txt"
        source.write_bytes(b"Original user-owned notes\n")
        preserved = self.root / "preserved-notes.txt"
        preserved.write_bytes(source.read_bytes())
        baseline = json.loads(self.baseline.read_text(encoding="utf-8"))
        baseline["untracked"] = [{"path": "user-notes.txt", "sha256": self.proof(source)["sha256"]}]
        self.baseline.write_text(json.dumps(baseline), encoding="utf-8")
        self.receipt["baseline"] = self.proof(self.baseline)
        self.receipt["preserved_untracked"] = [{"path": "user-notes.txt", "evidence": self.proof(preserved)}]
        source.unlink()
        self.reject("untracked inventory changed")

    def test_removed_task_requires_metadata_and_removed_disposition(self):
        task = self.removed_task_receipt()
        self.git("worktree", "remove", str(task))
        baseline = json.loads(self.baseline.read_text(encoding="utf-8"))
        for field in ("git_common_dir", "source_ref"):
            altered = dict(baseline)
            altered.pop(field)
            self.baseline.write_text(json.dumps(altered), encoding="utf-8")
            self.receipt["baseline"] = self.proof(self.baseline)
            self.reject("requires common directory and source ref together")
        self.baseline.write_text(json.dumps(baseline), encoding="utf-8")
        self.receipt["baseline"] = self.proof(self.baseline)
        self.receipt["resources"][-1]["status"] = "retained"
        self.reject("needs exact removed resource")

    def test_removed_task_rejects_stale_registration_and_changed_ref(self):
        task = self.removed_task_receipt()
        self.assertEqual(task.resolve().parent, self.root)
        shutil.rmtree(task)
        self.reject("remains registered worktree")
        self.git("worktree", "prune")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                 "commit", "--allow-empty", "-m", "new source", "--quiet")
        self.git("update-ref", "refs/heads/task-closeout", self.git("rev-parse", "HEAD"))
        self.reject("retained source ref changed")

    def test_live_task_binds_common_directory_and_valid_ref(self):
        self.removed_task_receipt()
        baseline = json.loads(self.baseline.read_text(encoding="utf-8"))
        baseline["git_common_dir"] = str(self.root)
        self.baseline.write_text(json.dumps(baseline), encoding="utf-8")
        self.receipt["baseline"] = self.proof(self.baseline)
        self.reject("common directory mismatch")
        baseline["git_common_dir"] = str(self.repo / ".git")
        baseline["source_ref"] = "HEAD"
        self.baseline.write_text(json.dumps(baseline), encoding="utf-8")
        self.receipt["baseline"] = self.proof(self.baseline)
        self.reject("must name a retained branch")

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
                self.reject("empty text")

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
