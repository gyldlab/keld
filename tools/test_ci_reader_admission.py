"""Real-Git and real-SSH-signature admission controls for GH-1275.

The fixture's reviewer private key never enters the candidate repository.
Every test uses a disposable Git repository and checks an observable exit
status; a candidate-written proof is never itself an approval.
"""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


TOOL = Path(__file__).with_name("ci_reader_admission.py")
sys.path.insert(0, str(TOOL.parent))
import ci_reader_admission  # noqa: E402 - locate standalone tool in both test invocation modes

NAMESPACE = "keld-ci-reader-admission-v1"
PRINCIPAL = "independent-reviewer@gyldlab.invalid"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(data: dict) -> bytes:
    return (json.dumps(data, indent=2, sort_keys=True) + "\n").encode("utf-8")


class AdmissionCase:
    def __init__(self, root: Path):
        self.root = root / "candidate"
        self.root.mkdir()
        self.external = root / "protected-review"
        self.external.mkdir()
        self.key = self.external / "reviewer"
        self.signers = self.external / "allowed_signers"
        self.review = self.external / "receipt.json"
        self.signature = self.external / "receipt.json.sig"
        self.git("init", "-q")
        self.git("config", "maintenance.auto", "false")
        self.git("config", "user.name", "ci-test")
        self.git("config", "user.email", "ci-test@example.invalid")
        subprocess.run(
            ["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(self.key)],
            check=True, capture_output=True,
        )
        self.signers.write_text(
            PRINCIPAL + " " + (self.key.with_suffix(".pub")).read_text(encoding="utf-8"),
            encoding="utf-8",
        )
        self.policy = {
            "schema": "keld.ci-inputs/v1",
            "known_inputs": ["*"],
            "reader_sets": {
                "fixture": {"patterns": ["readers/*.py"], "sha256": "0" * 64},
            },
            "consumers": [{
                "owner": "fixture document reader",
                "reader_set": "fixture",
                "outputs": ["local_probe"],
                "inputs": ["docs/input.md"],
                "documentation_inputs": [],
            }],
            "rust_source_census": {
                "patterns": ["crates/*"], "membership_sha256": "0" * 64,
            },
        }
        self.write("tools/ci-inputs.json", canonical(self.policy))
        self.write(".github/CODEOWNERS", b"/.ci-reader-proofs/ @gyldlab/keld-maintainers\n")
        self.write("readers/check.py", b"open('docs/input.md')\n")
        self.write("docs/input.md", b"fixture\n")
        self.write("crates/a.rs", b"pub fn a() {}\n")
        self.commit("baseline")
        self.baseline = self.git("rev-parse", "HEAD").strip()
        self.current_proofs = {}
        self.approved_edges = []

    def git(self, *args: str) -> str:
        return subprocess.check_output(["git", *args], cwd=self.root, text=True).strip()

    def write(self, relative: str, contents: bytes) -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(contents)

    def commit(self, title: str) -> str:
        self.git("add", "-A")
        self.git("commit", "-qm", title)
        return self.git("rev-parse", "HEAD").strip()

    def update_policy(self) -> None:
        self.write("tools/ci-inputs.json", canonical(self.policy))

    def proof_path(self, source: str) -> str:
        return ".ci-reader-proofs/readers/" + digest(source.encode("utf-8")) + ".json"

    def add_proof(self, source: str, *, state="present", edges=None, reader_sets=None,
                  override_path=None, override_sha=None) -> None:
        if reader_sets is None:
            reader_sets = ["fixture"] if source.startswith("readers/") else []
        if edges is None:
            edges = []
        payload = {
            "schema": "keld.ci-reader-proof/v1",
            "baseline": self.baseline,
            "path": override_path or source,
            "reader_sets": reader_sets,
            "state": state,
            "sha256": (override_sha or digest((self.root / source).read_bytes().replace(b"\r\n", b"\n")))
                      if state == "present" else None,
            "edges": edges,
        }
        path = self.proof_path(source)
        self.write(path, canonical(payload))
        self.current_proofs[path] = payload
        self.approved_edges.extend(
            {"reader": source, "consumer": edge["consumer"], "input": edge["input"]}
            for edge in edges
        )

    def update_reader(self) -> None:
        self.write("readers/check.py", b"open('docs/input.md')\nopen('README.md')\n")
        self.write("README.md", b"new read\n")
        self.policy["consumers"][0]["inputs"].append("README.md")
        self.update_policy()
        self.add_proof("readers/check.py", edges=[{
            "consumer": "fixture document reader", "input": "README.md",
        }])

    def sign(self, *, forged_key: Path | None = None, review_updates=None) -> None:
        head = self.git("rev-parse", "HEAD").strip()
        approval = {
            "schema": "keld.ci-reader-review/v1",
            "baseline": self.baseline,
            "head": head,
            "reviewer": PRINCIPAL,
            "decision": "approve",
            "policy_sha256": digest((self.root / "tools/ci-inputs.json").read_bytes()),
            "codeowners_sha256": digest((self.root / ".github/CODEOWNERS").read_bytes()),
            "proofs": [{
                "path": name,
                "sha256": digest((self.root / name).read_bytes()),
            } for name in sorted(self.current_proofs)],
            "approved_edges": self.approved_edges,
        }
        if review_updates:
            approval.update(review_updates)
        self.review.write_bytes(canonical(approval))
        self.signature.unlink(missing_ok=True)
        subprocess.run(
            ["ssh-keygen", "-Y", "sign", "-f", str(forged_key or self.key),
             "-n", NAMESPACE, str(self.review)],
            check=True, capture_output=True,
        )

    def run(self, *, baseline=None, head=None, signer_pin=None, omit_authority=False,
            repo=None, fixture_only=True):
        args = [
            sys.executable, "-B", str(TOOL), "--repo", str(repo or self.root),
            "--baseline", baseline or self.baseline,
            "--head", head or self.git("rev-parse", "HEAD").strip(),
            "--receipt", str(self.review), "--signature", str(self.signature),
            "--allowed-signers", str(self.signers),
        ]
        if not omit_authority:
            args += ["--allowed-signers-sha256", signer_pin or digest(self.signers.read_bytes())]
        if fixture_only:
            args.append("--fixture-verify-only")
        return subprocess.run(args, capture_output=True, text=True, encoding="utf-8")


class AdmissionTests(unittest.TestCase):
    def setUp(self):
        if not shutil.which("ssh-keygen"):
            self.skipTest("a real OpenSSH signature verifier is required")
        self.temp = tempfile.TemporaryDirectory(prefix="keld-reader-admission-")
        self.addCleanup(self.temp.cleanup)
        self.case = AdmissionCase(Path(self.temp.name))

    def assertRefused(self, result, message=None):
        self.assertNotEqual(result.returncode, 0, (result.stdout, result.stderr))
        self.assertEqual(result.stdout, "", result.stdout)
        self.assertIn("CI-READER-ADMISSION:", result.stderr)
        if message:
            self.assertIn(message, result.stderr)

    def test_independently_signed_exact_head_with_new_edge_is_admitted(self):
        self.case.update_reader()
        self.case.commit("explicit reviewed reader scope")
        self.case.sign()
        result = self.case.run()
        self.assertEqual(result.returncode, 0, (result.stdout, result.stderr))
        self.assertIn("CI-READER-ADMISSION: fixture verified (UNTRUSTED)", result.stdout)

    def test_default_cli_requires_protected_invoker_even_with_valid_signature(self):
        self.case.update_reader()
        self.case.commit("reviewed reader")
        self.case.sign()
        self.assertRefused(self.case.run(fixture_only=False), "protected")

    def test_missing_independent_authority_and_wrong_trust_pin_refuse(self):
        self.case.update_reader()
        self.case.commit("reader")
        self.case.sign()
        self.assertRefused(self.case.run(omit_authority=True))
        self.assertRefused(self.case.run(signer_pin="0" * 64), "signer")

    def test_self_issued_matching_proof_and_forged_review_refuse(self):
        self.case.update_reader()
        self.case.commit("forged reader approval")
        other_key = self.case.external / "attacker"
        subprocess.run(
            ["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(other_key)],
            check=True, capture_output=True,
        )
        self.case.sign(forged_key=other_key)
        self.assertRefused(self.case.run(), "signature")

    def test_attacker_controls_signer_and_matching_pin_but_adapter_hard_fails(self):
        self.case.update_reader()
        self.case.commit("candidate with self-issued authority")
        other_key = self.case.external / "self-issued"
        subprocess.run(
            ["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(other_key)],
            check=True, capture_output=True,
        )
        self.case.sign(forged_key=other_key)
        self.case.signers.write_text(
            PRINCIPAL + " " + other_key.with_suffix(".pub").read_text(encoding="utf-8"),
            encoding="utf-8",
        )
        # A generic CLI signature verifier cannot infer that this signer pin
        # came from the attacker. It is deliberately NOT the admission gate.
        self.assertEqual(self.case.run().returncode, 0)
        with self.assertRaisesRegex(ci_reader_admission.AdmissionError, "authority unavailable"):
            ci_reader_admission.admit_shadow_contract(self.case.root)

    def test_self_authored_proof_with_omitted_edge_refuses_even_with_reviewer_signature(self):
        self.case.update_reader()
        self.case.add_proof("readers/check.py", edges=[])
        self.case.commit("unreviewed source edge")
        # Reviewer independently says README is an edge, while candidate
        # proof hides it. A signed review must not sanctify this mismatch.
        self.case.sign(review_updates={"approved_edges": [{
            "reader": "readers/check.py",
            "consumer": "fixture document reader", "input": "README.md",
        }]})
        self.assertRefused(self.case.run(), "edge")

    def test_source_edge_missing_from_policy_refuses(self):
        self.case.update_reader()
        self.case.policy["consumers"][0]["inputs"].remove("README.md")
        self.case.update_policy()
        self.case.commit("forgotten edge")
        self.case.sign()
        self.assertRefused(self.case.run(), "edge")

    def test_stale_review_after_new_head_refuses(self):
        self.case.update_reader()
        self.case.commit("reviewed head")
        self.case.sign()
        self.case.write("docs/later.md", b"another commit\n")
        self.case.commit("later head")
        self.assertRefused(self.case.run(), "head")

    def test_missing_immutable_git_baseline_refuses(self):
        self.case.update_reader()
        self.case.commit("reader")
        self.case.sign()
        self.assertRefused(self.case.run(baseline="f" * 40), "baseline")

    def test_shallow_git_clone_refuses_even_with_visible_baseline_commit(self):
        self.case.update_reader()
        self.case.commit("approved reader")
        self.case.sign()
        clone = Path(self.temp.name) / "shallow-copy"
        subprocess.run(
            ["git", "clone", "-q", "--depth", "2", self.case.root.as_uri(), str(clone)],
            check=True, capture_output=True,
        )
        self.assertEqual(
            subprocess.check_output(
                ["git", "rev-parse", "--is-shallow-repository"], cwd=clone, text=True,
            ).strip(), "true",
        )
        self.assertEqual(
            subprocess.check_output(
                ["git", "cat-file", "-t", self.case.baseline], cwd=clone, text=True,
            ).strip(), "commit",
        )
        self.assertRefused(self.case.run(repo=clone), "shallow")

    def test_deleted_reader_without_tombstone_refuses(self):
        (self.case.root / "readers/check.py").unlink()
        self.case.commit("deleted reader")
        self.case.sign()
        self.assertRefused(self.case.run(), "tombstone")

    def test_signed_retirement_tombstone_admits_deleted_reader(self):
        (self.case.root / "readers/check.py").unlink()
        self.case.add_proof("readers/check.py", state="retired")
        self.case.commit("reviewed retirement")
        self.case.sign()
        result = self.case.run()
        self.assertEqual(result.returncode, 0, (result.stdout, result.stderr))

    def test_added_then_deleted_reader_needs_retirement_tombstone(self):
        self.case.write("readers/transient.py", b"open('README.md')\n")
        self.case.commit("new reader after baseline")
        (self.case.root / "readers/transient.py").unlink()
        self.case.commit("later delete reader")
        self.case.sign()
        self.assertRefused(self.case.run(), "tombstone")

    def test_replace_ref_cannot_hide_missing_retirement_tombstone(self):
        self.case.write("readers/transient.py", b"open('README.md')\n")
        self.case.commit("transient reader added")
        (self.case.root / "readers/transient.py").unlink()
        self.case.commit("transient reader deleted")
        self.case.sign()
        self.assertRefused(self.case.run(), "tombstone")
        self.case.git("replace", "--graft", "HEAD", self.case.baseline)
        self.assertRefused(self.case.run(), "tombstone")

    def test_legacy_grafts_cannot_override_reviewed_git_history(self):
        self.case.update_reader()
        self.case.commit("signed reader")
        self.case.sign()
        grafts = self.case.root / ".git" / "info" / "grafts"
        grafts.write_text(self.case.git("rev-parse", "HEAD") + " " + self.case.baseline + "\n")
        self.assertRefused(self.case.run())

    def test_transient_post_baseline_reader_signed_tombstone_admits(self):
        self.case.write("readers/transient.py", b"open('docs/input.md')\n")
        self.case.commit("new reviewed reader")
        (self.case.root / "readers/transient.py").unlink()
        self.case.add_proof("readers/transient.py", state="retired")
        self.case.commit("signed retired history")
        self.case.sign()
        result = self.case.run()
        self.assertEqual(result.returncode, 0, (result.stdout, result.stderr))

    def test_untracked_and_ignored_new_reader_refuse(self):
        self.case.update_reader()
        self.case.commit("reader")
        self.case.sign()
        self.case.write("readers/untracked.py", b"pass\n")
        self.assertRefused(self.case.run(), "untracked")
        (self.case.root / "readers/untracked.py").unlink()
        self.case.write(".gitignore", b"readers/ignored.py\n")
        self.case.write("readers/ignored.py", b"pass\n")
        self.assertRefused(self.case.run(), "ignored")

    def test_staged_new_reader_outside_exact_head_refuses(self):
        self.case.update_reader()
        self.case.commit("reader")
        self.case.sign()
        self.case.write("readers/staged.py", b"pass\n")
        self.case.git("add", "readers/staged.py")
        self.assertRefused(self.case.run(), "staged")

    def test_intent_to_add_reader_outside_exact_head_refuses(self):
        self.case.update_reader()
        self.case.commit("reader")
        self.case.sign()
        self.case.write("readers/intent.py", b"pass\n")
        self.case.git("add", "-N", "readers/intent.py")
        self.assertRefused(self.case.run(), "staged")

    def test_untracked_proof_not_in_reviewed_head_refuses(self):
        self.case.update_reader()
        self.case.commit("reader")
        self.case.sign()
        self.case.write(".ci-reader-proofs/readers/unapproved.json", b"{}")
        self.assertRefused(self.case.run(), "untracked")

    def test_missing_tracked_reader_and_symlink_source_refuse(self):
        self.case.update_reader()
        self.case.commit("reader")
        self.case.sign()
        (self.case.root / "readers/check.py").unlink()
        self.assertRefused(self.case.run(), "missing")
        (self.case.root / "readers/check.py").symlink_to("../docs/input.md")
        self.assertRefused(self.case.run(), "symlink")

    def test_symlinked_proof_rejected(self):
        self.case.update_reader()
        proof = next(iter(self.case.current_proofs))
        real = self.case.external / "proof.json"
        real.write_bytes((self.case.root / proof).read_bytes())
        (self.case.root / proof).unlink()
        (self.case.root / proof).symlink_to(real)
        self.case.commit("indirect source proof")
        self.case.sign()
        self.assertRefused(self.case.run(), "symlink")

    def test_proof_name_must_bind_exact_source_path(self):
        self.case.update_reader()
        self.case.add_proof("readers/check.py", override_path="../escape")
        self.case.commit("malformed proof name")
        self.case.sign()
        self.assertRefused(self.case.run(), "path")

    def test_duplicate_json_keys_refuse_even_with_valid_signature(self):
        self.case.update_reader()
        proof = next(iter(self.case.current_proofs))
        path = self.case.root / proof
        payload = path.read_text(encoding="utf-8")
        payload = payload.replace('"schema": "keld.ci-reader-proof/v1",',
                                  '"schema": "keld.ci-reader-proof/v1", "schema": "keld.ci-reader-proof/v1",')
        path.write_text(payload, encoding="utf-8")
        self.case.commit("duplicate proof keys")
        self.case.sign()
        self.assertRefused(self.case.run(), "duplicate")

    def test_unowned_proof_namespace_refuses(self):
        self.case.update_reader()
        self.case.write(".github/CODEOWNERS", b"/tools/ @gyldlab/keld-maintainers\n")
        self.case.commit("proofs missing owner")
        self.case.sign()
        self.assertRefused(self.case.run(), "CODEOWNERS")

    def test_exact_codeowners_rule_from_1313_accepts_alignment_spaces(self):
        self.case.update_reader()
        self.case.write(
            ".github/CODEOWNERS",
            b"/.github/ @gyldlab/keld-maintainers\n"
            b"/.ci-reader-proofs/                     @gyldlab/keld-maintainers\n",
        )
        self.case.commit("effective proof owner with standard alignment")
        self.case.sign()
        result = self.case.run()
        self.assertEqual(result.returncode, 0, (result.stdout, result.stderr))

    def test_later_codeowners_rule_cannot_shadow_proof_owner(self):
        self.case.update_reader()
        self.case.write(
            ".github/CODEOWNERS",
            b"/.ci-reader-proofs/ @gyldlab/keld-maintainers\n* @attacker\n",
        )
        self.case.commit("shadowed review owner")
        self.case.sign()
        self.assertRefused(self.case.run(), "CODEOWNERS")

    def test_forged_same_name_check_or_unrecognized_receipt_field_refuses(self):
        self.case.update_reader()
        self.case.commit("reader")
        self.case.sign(review_updates={"check_name": "CI provenance"})
        self.assertRefused(self.case.run(), "field")


if __name__ == "__main__":
    unittest.main()
