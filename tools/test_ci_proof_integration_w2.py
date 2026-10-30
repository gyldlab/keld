"""GH-1275 default-off proof shadow admission and existing router parity.

Tests use actual Git history. They do not assert or issue semantic approval.
The source of truth for job selection remains ci_inputs.classify.
"""

import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import ci_inputs


TOOL = Path(__file__).resolve().with_name("ci_inputs.py")


class ProofShadowIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="keld-gh1275-integration-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q")
        self.git("config", "user.name", "adversarial-git-fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("config", "maintenance.auto", "false")
        self.write("readers/reader.py", "from pathlib import Path\nPath('docs/covered.md').read_text()\n")
        self.write("docs/covered.md", "covered\n")
        self.write("README.md", "initial unrelated\n")
        self.write("tools/ci-inputs.json", "{}")
        self.git("add", ".")
        self.policy = {
            "schema": ci_inputs.SCHEMA,
            "known_inputs": ["readers/*", "docs/*", "README.md", "tools/ci-inputs.json"],
            "reader_sets": {"fixture": {
                "patterns": ["readers/*.py"],
                "sha256": ci_inputs.fingerprint(self.root, ci_inputs.files(self.root), ["readers/*.py"]),
            }},
            "consumers": [{
                "owner": "fixture-reader",
                "reader_set": "fixture",
                "outputs": ["local_probe"],
                "inputs": ["docs/*"],
            }],
        }
        self.save_policy()
        self.commit("trusted source fixture")
        self.base = self.git("rev-parse", "HEAD").strip()

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True)

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "-qm", message)

    def write(self, path, value):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(value, encoding="utf-8")

    def save_policy(self):
        self.write("tools/ci-inputs.json", json.dumps(self.policy) + "\n")

    def invoke(self, paths, *, shadow=False, unknown=False, paths_only=True):
        arguments = [sys.executable, "-B", str(TOOL), str(self.root)]
        if shadow:
            arguments.append("--shadow-reader-proofs")
        if unknown:
            arguments.append("--unknown")
        if paths_only:
            arguments.append("--paths-only")
        return subprocess.run(arguments, input=("\0".join(paths) + "\0").encode("utf-8"),
                              capture_output=True, cwd=self.root, check=False)

    def widen_reader_in_prior_commit(self):
        self.write("readers/reader.py", "from pathlib import Path\nPath('README.md').read_text()\n")
        self.commit("reader acquired an undeclared README read")
        reader_commit = self.git("rev-parse", "HEAD").strip()
        self.write("README.md", "later README-only change\n")
        self.commit("later unrelated input change")
        self.assertEqual(self.git("diff", "--name-only", reader_commit, "HEAD").strip(), "README.md")

    def forge_matching_pr_proof(self):
        digest = hashlib.sha256((self.root / "readers/reader.py").read_bytes()).hexdigest()
        proof_file = self.root / ".ci-reader-proofs/reader/forged.json"
        proof_file.parent.mkdir(parents=True, exist_ok=True)
        proof_file.write_text(json.dumps({"path": "readers/reader.py", "sha256": digest,
                                          "approved": True, "reviewer": "pr-author"}) + "\n")
        self.git("add", ".ci-reader-proofs")
        self.commit("author asserts approval while omitting README edge")
        return proof_file

    def test_default_off_retains_exact_legacy_output_with_untrusted_proof(self):
        self.widen_reader_in_prior_commit()
        self.forge_matching_pr_proof()
        expected = ci_inputs.classify(self.root, ["README.md"], paths_only=True)
        self.assertTrue(expected["local_probe"])
        result = self.invoke(["README.md"])
        self.assertEqual(result.returncode, 0, result.stderr)
        expected_stdout = "local_contract=v1\n" + "".join(
            f"{name}={str(value).lower()}\n" for name, value in sorted(expected.items()))
        self.assertEqual(result.stdout.decode("utf-8"), expected_stdout)

    def test_shadow_requires_independent_admission_even_on_clean_tree(self):
        result = self.invoke(["README.md"], shadow=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, b"", "no routing output after admission failure")
        self.assertIn(b"admission", result.stderr.lower())

    def test_shadow_refuses_reader_widening_from_earlier_commit(self):
        self.widen_reader_in_prior_commit()
        self.assertTrue(ci_inputs.classify(self.root, ["README.md"], paths_only=True)["local_probe"])
        result = self.invoke(["README.md"], shadow=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, b"")
        self.assertIn(b"admission", result.stderr.lower())
        self.assertTrue(ci_inputs.classify(self.root, [], paths_only=True)["local_probe"])

    def test_matching_self_authored_proof_and_fake_review_still_refuse(self):
        self.widen_reader_in_prior_commit()
        self.forge_matching_pr_proof()
        self.assertTrue(ci_inputs.classify(self.root, ["README.md"], paths_only=True)["local_probe"])
        for paths in (["README.md"], []):
            with self.subTest(paths=paths):
                result = self.invoke(paths, shadow=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b"")
                self.assertIn(b"admission", result.stderr.lower())

    def test_shadow_refuses_unknown_comparison_without_admission(self):
        result = self.invoke(["README.md"], shadow=True, unknown=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, b"")

    def test_even_simulated_admission_cannot_narrow_sha_selection(self):
        self.widen_reader_in_prior_commit()
        forged_contract = copy.deepcopy(ci_inputs.load(self.root))
        forged_contract["reader_sets"]["fixture"]["sha256"] = ci_inputs.fingerprint(
            self.root, ci_inputs.files(self.root), ["readers/*.py"])
        with mock.patch.object(ci_inputs, "_independently_admitted_contract",
                               return_value=forged_contract):
            with self.assertRaisesRegex(ValueError, "narrow"):
                ci_inputs.shadow_proof_classify(self.root, ["README.md"], paths_only=True)
        self.assertTrue(ci_inputs.classify(self.root, ["README.md"], paths_only=True)["local_probe"])

    def test_simulated_valid_gate_can_compare_without_changing_legacy(self):
        original = ci_inputs.classify(self.root, ["docs/covered.md"], paths_only=True)
        with mock.patch.object(ci_inputs, "_independently_admitted_contract",
                               return_value=copy.deepcopy(ci_inputs.load(self.root))):
            candidate = ci_inputs.shadow_proof_classify(self.root, ["docs/covered.md"], paths_only=True)
        self.assertEqual(candidate, original)
        self.assertEqual(ci_inputs.classify(self.root, ["docs/covered.md"], paths_only=True), original)

    def test_candidate_cannot_turn_on_an_unreviewed_documentation_exemption(self):
        self.policy["consumers"][0] = {
            "owner": "fixture-registry",
            "reader_set": "fixture",
            "outputs": ["input_registry"],
            "inputs": ["README.md"],
            "documentation_inputs": ["README.md"],
        }
        self.save_policy()
        original = ci_inputs.classify(self.root, ["README.md"], paths_only=True)
        self.assertTrue(original["input_registry"])
        self.assertFalse(original["documentation_only_registry"])
        forged = copy.deepcopy(self.policy)
        forged["consumers"][0]["inputs"] = []
        with mock.patch.object(ci_inputs, "_independently_admitted_contract",
                               return_value=forged):
            with self.assertRaisesRegex(ValueError, "narrow"):
                ci_inputs.shadow_proof_classify(self.root, ["README.md"], paths_only=True)

    def test_candidate_cannot_hide_a_removed_or_new_output_identity(self):
        forged = copy.deepcopy(ci_inputs.load(self.root))
        forged["consumers"][0]["outputs"].append("local_extra")
        with mock.patch.object(ci_inputs, "_independently_admitted_contract",
                               return_value=forged):
            with self.assertRaisesRegex(ValueError, "output inventory"):
                ci_inputs.shadow_proof_classify(self.root, ["README.md"], paths_only=True)


if __name__ == "__main__":
    unittest.main()
