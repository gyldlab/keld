"""Real Git and reader-drift controls for selective local CI."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

import ci_inputs
import ci_local


def tracked_snapshot(source: Path, destination: Path) -> None:
    """Make a real Git fixture from tracked working bytes, without touching source.

    Scopes/consumers are copied unchanged. Only the fixture's reader digests are
    bound to its controlled census, so clean-input controls do not inherit an
    unrelated unknown file from the developer's checkout. Live drift is tested
    separately and must still select all its affected consumers.
    """
    destination.mkdir(parents=True, exist_ok=True)
    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=source)
    for raw in tracked.split(b"\0"):
        if not raw:
            continue
        relative = raw.decode("utf-8")
        original = source / relative
        if not original.exists() and not original.is_symlink():
            continue
        copied = destination / relative
        copied.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(original, copied, follow_symlinks=False)
    for args in (("init", "-q"), ("config", "core.autocrlf", "false"), ("add", ".")):
        subprocess.run(["git", *args], cwd=destination, check=True, capture_output=True)
    contract = ci_inputs.load(destination)
    census = ci_inputs.files(destination)
    for reader_set in contract["reader_sets"].values():
        reader_set["sha256"] = ci_inputs.fingerprint(destination, census, reader_set["patterns"])
    (destination / "tools/ci-inputs.json").write_text(json.dumps(contract, indent=2) + "\n", encoding="utf-8")


class InputContractTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="keld-ci-inputs-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q")
        self.git("config", "user.name", "ci-contract-test")
        self.git("config", "user.email", "ci-contract@example.invalid")
        self.write("readers/check.py", "from pathlib import Path\nPath('docs/input.md').read_text()\n")
        self.write("docs/input.md", "input\n")
        self.write("README.md", "unrelated\n")
        self.write("tools/ci-inputs.json", "{}")
        self.git("add", ".")
        self.contract = {
            "schema": ci_inputs.SCHEMA,
            "known_inputs": ["readers/*", "docs/*", "README.md", "tools/ci-inputs.json"],
            "reader_sets": {"fixture": {"patterns": ["readers/*.py"],
                "sha256": ci_inputs.fingerprint(self.root, ci_inputs.files(self.root), ["readers/*.py"])}},
            "consumers": [{
                "owner": "fixture document reader",
                "outputs": ["local_probe"],
                "inputs": ["docs/*"],
                "reader_set": "fixture",
            }],
        }
        self.save_contract()
        self.git("add", ".")
        self.git("commit", "-qm", "baseline")
        self.base = self.git("rev-parse", "HEAD").strip()

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True, encoding="utf-8")

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def save_contract(self):
        self.write("tools/ci-inputs.json", json.dumps(self.contract))

    def changes(self):
        changed = self.git("diff", "--no-renames", "--name-only", "-z", self.base, "--")
        changed += self.git("ls-files", "--others", "--exclude-standard", "-z")
        return [name for name in changed.split("\0") if name]

    def route(self, **kwargs):
        return ci_inputs.classify(self.root, self.changes(), **kwargs)

    def test_empty_and_proven_unrelated_changes_omit_expensive_consumer(self):
        self.assertFalse(self.route()["local_probe"])
        self.write("README.md", "unrelated edit\n")
        self.assertFalse(self.route()["local_probe"])
        for gate in ("fmt-check", "clippy", "test", "agent-context", "audit-docs"):
            self.assertTrue(self.route()["local_" + gate])

    def test_relevant_staged_unstaged_and_committed_changes_select(self):
        self.write("docs/input.md", "changed\n")
        self.assertTrue(self.route()["local_probe"])
        self.git("add", "docs/input.md")
        self.assertTrue(self.route()["local_probe"])
        self.git("commit", "-qm", "relevant")
        self.assertTrue(self.route()["local_probe"])

    def test_untracked_unknown_deleted_and_missing_comparison_fail_closed(self):
        self.write("README-untracked.md", "new\n")
        self.assertTrue(self.route()["input_all"])
        self.git("add", "README-untracked.md")
        self.git("commit", "-qm", "unknown tracked input")
        self.assertTrue(self.route()["input_all"])
        (self.root / "README.md").unlink()
        self.assertTrue(self.route()["local_probe"])
        self.assertTrue(self.route(comparison_unknown=True)["input_all"])

    def test_known_deleted_file_alone_fails_closed(self):
        (self.root / "README.md").unlink()
        self.assertTrue(self.route()["input_all"])

    def test_known_router_policy_change_keeps_the_workflow_fallback_distinct(self):
        self.contract["consumers"][0]["contract"] = "reviewed scope update"
        self.save_contract()
        result = self.route()
        self.assertTrue(result["input_router"])
        self.assertTrue(result["local_probe"])
        self.assertFalse(result["input_all"])

    def test_git_ignored_recursive_input_is_not_assumed_unrelated(self):
        self.write(".gitignore", "docs/generated/\n")
        self.git("add", ".gitignore")
        self.git("commit", "-qm", "ignore generated files")
        self.base = self.git("rev-parse", "HEAD").strip()
        self.write("docs/generated/local.md", "untracked but visible to a recursive reader\n")
        self.assertEqual(self.changes(), [])
        self.assertTrue(self.route()["input_all"])

    def test_reader_widening_invalidates_old_exclusion(self):
        self.write("readers/check.py", "from pathlib import Path\nPath('README.md').read_text()\n")
        self.git("add", "readers/check.py")
        self.git("commit", "-qm", "reader now consumes README")
        # Compare after the reader landed, so only the formerly unrelated input
        # appears in the current diff. A changed-path-only guard misses this.
        self.base = self.git("rev-parse", "HEAD").strip()
        self.write("README.md", "now relevant\n")
        self.assertTrue(self.route()["local_probe"])

    def test_new_and_removed_readers_invalidate_source_inventory(self):
        self.write("readers/new.py", "open('README.md')\n")
        self.git("add", "readers/new.py")
        self.git("commit", "-qm", "new reader")
        self.base = self.git("rev-parse", "HEAD").strip()
        self.assertTrue(self.route()["local_probe"])
        (self.root / "readers/check.py").unlink()
        self.assertTrue(self.route()["local_probe"])

    def test_missing_or_malformed_contract_fails_before_selection(self):
        for text in ("invalid", '{}', '[]', 'null', '42', '{"schema":"wrong","consumers":[]}'):
            self.write("tools/ci-inputs.json", text)
            with self.assertRaises(ValueError):
                self.route()
        (self.root / "tools/ci-inputs.json").unlink()
        with self.assertRaises(ValueError):
            self.route()


class ExecutorTests(unittest.TestCase):
    def route(self, **overrides):
        data = {"local_contract": "v1", "local_default": "true",
                "local_fmt-check": "true", "local_clippy": "true", "local_test": "true",
                "local_probe": "false"}
        data.update(overrides)
        return "\n".join(f"{key}={value}" for key, value in data.items())

    def test_unknown_gate_runs_and_malformed_gate_fails(self):
        gates = ["fmt-check", "clippy", "test", "probe", "new-gate"]
        selected = ci_local.selection(self.route(), gates)
        self.assertFalse(selected["probe"])
        self.assertTrue(selected["new-gate"])
        for malformed in ("yes", "", "0"):
            with self.assertRaises(ValueError):
                ci_local.selection(self.route(local_probe=malformed), gates)
        with self.assertRaises(ValueError):
            ci_local.selection(self.route() + "\nlocal_probe=true", gates)

    def test_mandatory_rust_gate_cannot_be_routed_away(self):
        for gate in ("fmt-check", "clippy", "test"):
            with self.assertRaises(ValueError):
                ci_local.selection(self.route(**{"local_" + gate: "false"}), ["fmt-check", "clippy", "test"])

    def test_real_just_inventory_preserves_bodyful_prerequisites_and_order(self):
        with tempfile.TemporaryDirectory(prefix="keld-ci-inventory-") as temporary:
            root = Path(temporary)
            (root / "justfile").write_text(
                'ci-inventory: policy fmt-check clippy test\n[parallel]\npolicy: probe-test probe\n'
                'probe-test:\n    echo test\nprobe: probe-test\n    echo live\n'
                'fmt-check:\n    echo fmt\nclippy:\n    echo lint\ntest:\n    echo test\n',
                encoding="utf-8")
            recipes = ci_local.inventory(root)
            self.assertEqual(ci_local.leaves(recipes, "ci-inventory"),
                             ["probe-test", "probe", "fmt-check", "clippy", "test"])

    def test_real_execution_omits_only_selected_work_and_propagates_failure(self):
        with tempfile.TemporaryDirectory(prefix="keld-ci-execute-") as temporary:
            root = Path(temporary)
            source = (
                'ci-inventory: policy fmt-check clippy test doc deny\n'
                '[parallel]\npolicy: expensive live\n'
                'expensive:\n    echo expensive >> observed\n'
                'live: expensive\n    echo live >> observed\n'
                'fmt-check:\n    echo fmt >> observed\n'
                'clippy:\n    echo clippy >> observed\n'
                'test:\n    echo test >> observed\n'
                'doc:\n    echo doc >> observed\n'
                'deny:\n    echo deny >> observed\n')
            path = root / "justfile"
            path.write_text(source, encoding="utf-8")
            recipes = ci_local.inventory(root)
            selected = dict.fromkeys(ci_local.leaves(recipes, "ci-inventory"), True)
            selected["expensive"] = False
            self.assertEqual(ci_local.execute(root, recipes, "ci-inventory", selected), 0)
            self.assertEqual((root / "observed").read_text().splitlines(),
                             ["live", "fmt", "clippy", "test", "doc", "deny"])
            (root / "observed").unlink()
            # Public direct invocation retains its prerequisite.
            subprocess.run(["just", "live"], cwd=root, check=True)
            self.assertEqual((root / "observed").read_text().splitlines(), ["expensive", "live"])
            (root / "observed").unlink()
            path.write_text(source.replace("echo expensive >> observed", "exit 7"), encoding="utf-8")
            selected["expensive"] = True
            self.assertNotEqual(ci_local.execute(root, ci_local.inventory(root), "ci-inventory", selected), 0)
            # The live parallel sibling drained, while every serial gate stayed off.
            self.assertEqual((root / "observed").read_text().splitlines(), ["live"])

    def test_parallel_or_omitted_rust_gate_is_rejected_before_execution(self):
        with tempfile.TemporaryDirectory(prefix="keld-ci-invalid-") as temporary:
            root = Path(temporary)
            suffix = "fmt-check clippy test doc deny"
            bodies = "".join(f"{gate}:\n    echo should-not-run\n" for gate in suffix.split())
            for header in (f"[parallel]\nci-inventory: {suffix}\n", "ci-inventory: fmt-check clippy doc deny\n"):
                (root / "justfile").write_text(header + bodies, encoding="utf-8")
                with self.assertRaises(ValueError):
                    ci_local.validate_inventory(ci_local.inventory(root), "ci-inventory")


class RouterFailureBoundaryTests(unittest.TestCase):
    def test_real_github_and_local_entrypoints_never_publish_after_helper_failure(self):
        source = Path(__file__).resolve().parent
        bash = shutil.which("bash")
        self.assertIsNotNone(bash, "router tests require the same Bash used by CI")
        for fault in ("helper-exit", "[]", "null", "42", "invalid"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory(prefix="keld-ci-boundary-") as temporary:
                root = Path(temporary)
                repo = root / "repo"
                (repo / "tools").mkdir(parents=True)
                (repo / ".github").mkdir()
                (root / "bin").mkdir()
                for name in ("ci_changes.sh", "ci_inputs.py"):
                    shutil.copyfile(source / name, repo / "tools" / name)
                (repo / "tools/ci-inputs.json").write_text("{}" if fault == "helper-exit" else fault, encoding="utf-8")
                if fault == "helper-exit":
                    (repo / "tools/ci_inputs.py").write_text("raise SystemExit(37)\n", encoding="utf-8")
                owners = repo / ".github/CODEOWNERS"
                owners.write_text("* @before\n", encoding="utf-8")
                cargo = root / "bin/cargo"
                cargo.write_text('#!/usr/bin/env bash\nprintf \'%s\\n\' \'{"packages":[]}\'\n', encoding="utf-8")
                cargo.chmod(0o755)
                environment = os.environ.copy()
                environment["PATH"] = str(root / "bin") + os.pathsep + environment["PATH"]
                for args in (("init", "-q"), ("config", "user.name", "CI boundary"),
                             ("config", "user.email", "boundary@example.invalid"), ("add", "."),
                             ("commit", "-qm", "base")):
                    subprocess.run(["git", *args], cwd=repo, check=True, capture_output=True)
                base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
                owners.write_text("* @after\n", encoding="utf-8")
                subprocess.run(["git", "commit", "-qam", "CODEOWNERS only"], cwd=repo, check=True, capture_output=True)
                head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
                environment.update(KELD_CI_EVENT_NAME="pull_request", KELD_CI_BASE_SHA=base,
                                   KELD_CI_HEAD_SHA=head, KELD_CI_BASE_REF=base)
                for mode in ("github", "local"):
                    output = root / (mode + "-output")
                    environment["GITHUB_OUTPUT"] = str(output)
                    result = subprocess.run([bash, "tools/ci_changes.sh", mode], cwd=repo,
                                            env=environment, capture_output=True, text=True, encoding="utf-8")
                    self.assertNotEqual(result.returncode, 0, (fault, mode, result.stdout, result.stderr))
                    self.assertEqual(result.stdout, "", (fault, mode))
                    self.assertFalse(output.exists(), (fault, mode))


class ProductionConsumerTests(unittest.TestCase):
    def test_bound_production_readers_and_real_cross_tree_inputs(self):
        source = Path(__file__).resolve().parent.parent
        live_contract = ci_inputs.load(source)
        live_census = ci_inputs.files(source)
        live_selection = ci_inputs.classify(source, [], paths_only=True)
        for name, reader_set in live_contract["reader_sets"].items():
            try:
                current = ci_inputs.fingerprint(source, live_census, reader_set["patterns"])
            except (OSError, ValueError):
                current = None
            if current != reader_set["sha256"]:
                for consumer in live_contract["consumers"]:
                    if consumer["reader_set"] == name:
                        for output in consumer["outputs"]:
                            self.assertTrue(live_selection[output],
                                            f"unbound {name} must not omit {output}")
        fixture = tempfile.TemporaryDirectory(prefix="keld-ci-production-")
        self.addCleanup(fixture.cleanup)
        root = Path(fixture.name)
        tracked_snapshot(source, root)
        contract = ci_inputs.load(root)
        census = ci_inputs.files(root)
        outputs = [output for consumer in contract["consumers"] for output in consumer["outputs"]]
        self.assertEqual(len(outputs), len(set(outputs)), "each consumer output has exactly one owner")
        self.assertTrue({"input_rust", "input_ts", "input_registry", "input_mermaid"} <= set(outputs),
                        "cross-tree consumer owners cannot be removed from the production contract")
        for name, reader_set in contract["reader_sets"].items():
            self.assertEqual(
                ci_inputs.fingerprint(root, census, reader_set["patterns"]), reader_set["sha256"],
                f"{name}: reader inventory changed; review its actual reads, update inputs, "
                "then renew the digest. Routing remains conservative until that review.")
        examples = {
            "crates/keld-ipc/src/lib.rs": ("input_ts",),
            "crates/keld-ipc/src/frame.rs": ("input_ts",),
            "crates/keld-ipc/src/echo.rs": ("input_ts",),
            "crates/keld-ipc/src/lifecycle.rs": ("input_ts",),
            "crates/keld-cli/templates/hello/src/main-body.ts": ("input_ts", "input_rust"),
            "tools/atomic_protocol.rs": ("input_registry", "input_rust"),
            "docs/architecture/03-security.md": ("input_rust", "local_llms-check"),
            "docs/engineering/keld-error-codes.md": ("input_rust", "local_llms-check"),
            "crates/keld-compat/fixtures/lifecycle-corpus/report.md": ("input_rust",),
            "llms-full.txt": ("input_rust", "local_llms-check"),
            "docs/engineering/product-status.tsv": ("local_product-status-check",),
        }
        for path, outputs in examples.items():
            self.assertTrue((root / path).is_file(), path)
            result = ci_inputs.classify(root, [path], paths_only=True)
            self.assertFalse(result["input_all"], f"{path}: positive example must not hide behind fallback")
            for output in outputs:
                self.assertTrue(result[output], f"{path} must select {output}")
        unrelated = ci_inputs.classify(root, ["README.md"], paths_only=True)
        for gate in ("agent-context-test", "ci-router-test", "typescript", "doc", "hooks-test"):
            self.assertFalse(unrelated["local_" + gate], gate)


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--tracked-snapshot":
        tracked_snapshot(Path(__file__).resolve().parent.parent, Path(sys.argv[2]))
    else:
        unittest.main()
