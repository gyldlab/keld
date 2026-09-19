"""Exact-field substitutions against real Git ancestry and retained evidence bytes."""

import copy
from dataclasses import replace
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from retained_fs_artifact import Invalid, Publication, validate


def sha(data):
    return hashlib.sha256(data).hexdigest()


class ArtifactTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix="keld-t1-validator-")
        cls.repo = Path(cls.temp.name)
        cls.environment = {**os.environ, "GIT_AUTHOR_NAME": "Keld test",
                           "GIT_AUTHOR_EMAIL": "test@invalid.example",
                           "GIT_COMMITTER_NAME": "Keld test",
                           "GIT_COMMITTER_EMAIL": "test@invalid.example"}
        cls.git("init", "--quiet")
        payload = b'{"schema":"keld.kel130-retained-filesystem-decisions/v1","fixture":true}'
        spec = b"# Synthetic contract fixture\n" + payload + b"\n"
        (cls.repo / "spec.md").write_bytes(spec)
        cls.git("add", "spec.md")
        cls.git("commit", "--quiet", "-m", "T0")
        t0 = cls.git("rev-parse", "HEAD")
        cls.t0 = {"schema": "keld.execution-artifact/v1", "node_id": "retained-filesystem-contract",
                  "issue_id": "KEL-130", "task_id": "KEL-130/T0", "status": "passed",
                  "landed_head": t0, "spec_path": "spec.md",
                  "spec_blob": cls.git("rev-parse", t0 + ":spec.md"),
                  "spec_sha256": sha(spec), "decision_digest": sha(payload)}
        cls.git("commit", "--quiet", "--allow-empty", "-m", "T1")
        cls.landed = cls.git("rev-parse", "HEAD")
        cls.git("commit", "--quiet", "--allow-empty", "-m", "main")
        cls.main = cls.git("rev-parse", "HEAD")
        cls.git("commit", "--quiet", "--allow-empty", "-m", "unlanded")
        cls.unlanded = cls.git("rev-parse", "HEAD")
        rows = []
        for system in ("macOS", "Windows", "Linux"):
            data = ("synthetic native fixture for " + system + "\n").encode()
            (cls.repo / (system + ".log")).write_bytes(data)
            rows.append({"os": system, "native": True, "device_id": system + "-fixture",
                         "os_build": "synthetic-test-only", "source_head": cls.landed,
                         "command": ["native-fixture", "--all"], "exit_code": 0,
                         "observable": "synthetic test receipt, never product evidence",
                         "raw_evidence": {"path": system + ".log", "sha256": sha(data)}})
        cls.candidate = {"schema": "keld.execution-artifact/v1", "node_id": "retained-filesystem",
                         "issue_id": "KEL-130", "task_id": "KEL-130/T1", "status": "passed",
                         "publisher_id": "authenticated-test-author", "claim_id": "winning-test-claim",
                         "landed_head": cls.landed,
                         "contract": {key: cls.t0[key] for key in ("landed_head", "spec_blob", "spec_sha256", "decision_digest")},
                         "tasks": {key: "passed" for key in ("T1a", "T1b", "T1c", "T1d")},
                         "native_evidence": rows}

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    @classmethod
    def git(cls, *args):
        return subprocess.check_output(["git", "-C", str(cls.repo), *args], env=cls.environment).decode().strip()

    def check(self, candidate=None, **overrides):
        data = json.dumps(self.candidate if candidate is None else candidate).encode()
        publication = Publication("authenticated-test-comment", "authenticated-test-author",
                                  "winning-test-claim", frozenset({"authenticated-test-author"}), sha(data))
        arguments = dict(approved_t0=self.t0, publication=publication, repo=self.repo,
                         current_main=self.main, evidence_root=self.repo)
        arguments.update(overrides)
        return validate(data, **arguments)

    def reject(self, candidate):
        with self.assertRaises((Invalid, OSError)):
            self.check(candidate)

    def test_exact_terminal_artifact_passes(self):
        self.assertEqual(self.check(), self.candidate)

    def test_identity_field_substitutions(self):
        for key in ("schema", "node_id", "issue_id", "task_id", "status", "publisher_id", "claim_id"):
            with self.subTest(field=key):
                changed = copy.deepcopy(self.candidate)
                changed[key] = "substituted"
                self.reject(changed)

    def test_missing_required_fields(self):
        for key in self.candidate:
            with self.subTest(field=key):
                changed = copy.deepcopy(self.candidate)
                del changed[key]
                self.reject(changed)

    def test_contract_field_substitutions(self):
        for key, value in self.candidate["contract"].items():
            with self.subTest(field=key):
                changed = copy.deepcopy(self.candidate)
                changed["contract"][key] = "0" * len(value)
                self.reject(changed)

    def test_missing_or_incomplete_task_rows(self):
        for task in ("T1a", "T1b", "T1c", "T1d"):
            for status in (None, "findings"):
                with self.subTest(task=task, status=status):
                    changed = copy.deepcopy(self.candidate)
                    if status is None:
                        del changed["tasks"][task]
                    else:
                        changed["tasks"][task] = status
                    self.reject(changed)

    def test_missing_or_duplicate_native_rows(self):
        changed = copy.deepcopy(self.candidate)
        changed["native_evidence"].pop()
        self.reject(changed)
        for key in ("os", "device_id"):
            changed = copy.deepcopy(self.candidate)
            changed["native_evidence"][1][key] = changed["native_evidence"][0][key]
            self.reject(changed)

    def test_native_row_fields_are_required(self):
        for index in range(3):
            for key in self.candidate["native_evidence"][index]:
                with self.subTest(index=index, key=key):
                    changed = copy.deepcopy(self.candidate)
                    del changed["native_evidence"][index][key]
                    self.reject(changed)

    def test_stale_ancestor_and_mixed_source_heads_reject(self):
        for indexes in ((0, 1, 2), (1,)):
            for source in (self.t0["landed_head"], self.unlanded, "0" * 40):
                with self.subTest(indexes=indexes, source=source):
                    changed = copy.deepcopy(self.candidate)
                    for index in indexes:
                        changed["native_evidence"][index]["source_head"] = source
                    self.reject(changed)

    def test_t0_and_unlanded_heads_are_not_predecessors(self):
        for head in (self.t0["landed_head"], self.unlanded, "0" * 40):
            changed = copy.deepcopy(self.candidate)
            changed["landed_head"] = head
            for row in changed["native_evidence"]:
                row["source_head"] = head
            self.reject(changed)

    def test_raw_digest_and_exit_native_status_substitutions(self):
        for index in range(3):
            for key, value in (("exit_code", 1), ("exit_code", False), ("native", False),
                               ("os", "WSL"), ("command", []), ("observable", "")):
                changed = copy.deepcopy(self.candidate)
                changed["native_evidence"][index][key] = value
                self.reject(changed)
            changed = copy.deepcopy(self.candidate)
            changed["native_evidence"][index]["raw_evidence"]["sha256"] = "0" * 64
            self.reject(changed)

    def test_unauthorized_or_different_publication_rejects(self):
        data = json.dumps(self.candidate).encode()
        good = Publication("comment", "authenticated-test-author", "winning-test-claim",
                           frozenset({"authenticated-test-author"}), sha(data))
        for publication in (replace(good, authorized_author_ids=frozenset()),
                            replace(good, author_id="impostor"),
                            replace(good, winning_claim_id="other-claim"),
                            replace(good, artifact_sha256="0" * 64)):
            with self.assertRaises(Invalid):
                self.check(publication=publication)

    def test_publication_requires_frozenset_of_nonempty_string_ids(self):
        author = "authenticated-test-author"
        data = json.dumps(self.candidate).encode()
        good = Publication("comment", author, "winning-test-claim", frozenset({author}), sha(data))
        malformed = [
            "not-" + author, [author], (author,), {author}, {author: True}, None,
            frozenset({author, 7}), frozenset({author, None}),
            frozenset({author, ""}), frozenset({author, " "}),
        ]
        for ids in malformed:
            with self.subTest(ids=ids), self.assertRaises(Invalid):
                self.check(publication=replace(good, authorized_author_ids=ids))
        with self.assertRaises(Invalid):
            self.check(publication=replace(good, authorized_author_ids=frozenset({"not-" + author})))
        self.assertEqual(self.check(publication=good), self.candidate)

    def test_contract_receipt_cannot_override_actual_git_bytes(self):
        for key in ("spec_blob", "spec_sha256", "decision_digest"):
            t0 = copy.deepcopy(self.t0)
            changed = copy.deepcopy(self.candidate)
            t0[key] = "0" * len(t0[key])
            changed["contract"][key] = t0[key]
            with self.assertRaises(Invalid):
                self.check(changed, approved_t0=t0)


if __name__ == "__main__":
    unittest.main()
