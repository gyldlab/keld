"""Real Git and reader-drift controls for selective local CI."""

import contextlib
import io
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import unittest.mock

import ci_inputs
import ci_local


def tracked_snapshot(source: Path, destination: Path, *, bind: bool = True) -> None:
    """Make a real Git fixture from tracked working bytes, without touching source.

    Scopes/consumers are copied unchanged. Only the fixture's reader digests are
    bound to its controlled census, so clean-input controls do not inherit an
    unrelated unknown file from the developer's checkout. Live drift is tested
    separately and must still select all its affected consumers. bind=False keeps the
    committed digests untouched so a freshness check sees exactly what CI checks out.
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
    for args in (("init", "-q"), ("config", "core.autocrlf", "false"), ("add", "-f", ".")):
        subprocess.run(["git", *args], cwd=destination, check=True, capture_output=True)
    if not bind:
        return
    contract = ci_inputs.load(destination)
    census = ci_inputs.files(destination)
    for reader_set in contract["reader_sets"].values():
        reader_set["sha256"] = ci_inputs.fingerprint(destination, census, reader_set["patterns"])
    rust_census = contract.get("rust_source_census")
    if rust_census:
        rust_census["membership_sha256"] = ci_inputs.membership_fingerprint(census, rust_census["patterns"])
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
        for gate in ("agent-context", "audit-docs", "product-status-check", "deny"):
            self.assertTrue(self.route()["local_" + gate])

    def test_malformed_package_input_mapping_cannot_omit_its_reader(self):
        for invalid in ([], None, "docs/input.md", {"keld-reader": "docs/input.md"},
                        {"keld-reader": []}, {"keld-reader": [None]}, {"keld-reader": [""]}):
            with self.subTest(mapping=invalid):
                self.contract["rust_package_inputs"] = invalid
                self.save_contract()
                with self.assertRaises(ValueError):
                    ci_inputs.classify(self.root, ["docs/input.md"], paths_only=True)

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

    def documentation_contract(self):
        self.write("src/reader.rs", "include_str!(\"../docs/input.md\");\n")
        self.git("add", "src/reader.rs")
        self.git("commit", "-qm", "reader source")
        self.base = self.git("rev-parse", "HEAD").strip()
        self.contract["known_inputs"].append("src/*")
        self.contract["consumers"].append({
            "owner": "fixture registry reader",
            "outputs": ["input_registry"],
            "inputs": ["src/*"],
            "documentation_inputs": ["docs/input.md"],
            "reader_set": "fixture",
        })
        self.contract["rust_package_inputs"] = {"keld-reader": ["src/*"]}
        self.contract["rust_package_documentation_inputs"] = {"keld-reader": ["docs/input.md"]}
        self.save_contract()
        self.git("add", "tools/ci-inputs.json")
        self.git("commit", "-qm", "documentation edges")
        self.base = self.git("rev-parse", "HEAD").strip()

    def assert_narrowing(self, result, expected):
        for key in ("documentation_only_registry", "documentation_only_package_keld-reader"):
            self.assertIs(result[key], expected, key)

    def test_documentation_only_edge_narrows_and_any_other_input_keeps_expansion(self):
        self.documentation_contract()
        self.write("docs/input.md", "documentation edit\n")
        result = self.route()
        self.assertTrue(result["input_registry"])
        self.assertTrue(result["input_package_keld-reader"])
        self.assert_narrowing(result, True)
        # Negative control: one non-documentation input of the same edges in
        # the same diff keeps reverse-dependent expansion for both edges.
        self.write("src/reader.rs", "include_str!(\"../docs/input.md\"); // reader edit\n")
        result = self.route()
        self.assertTrue(result["input_registry"])
        self.assertTrue(result["input_package_keld-reader"])
        self.assert_narrowing(result, False)

    def test_unrelated_and_fallback_selections_never_narrow(self):
        self.documentation_contract()
        self.write("README.md", "unrelated edit\n")
        result = self.route()
        self.assertFalse(result["input_registry"])
        self.assert_narrowing(result, False)
        self.write("docs/input.md", "documentation edit\n")
        self.assert_narrowing(self.route(comparison_unknown=True), False)
        self.write("docs/untracked.md", "no established contract\n")
        self.assert_narrowing(self.route(), False)
        (self.root / "docs/untracked.md").unlink()
        self.contract["consumers"][1]["contract"] = "reviewed scope update"
        self.save_contract()
        result = self.route()
        self.assertTrue(result["input_router"])
        self.assert_narrowing(result, False)

    def test_reader_drift_never_narrows(self):
        self.documentation_contract()
        self.write("readers/check.py", "from pathlib import Path\nPath('docs/other.md').read_text()\n")
        self.git("add", "readers/check.py")
        self.git("commit", "-qm", "reader drift")
        self.base = self.git("rev-parse", "HEAD").strip()
        self.write("docs/input.md", "documentation edit\n")
        result = self.route()
        # The fixture edge has no input_rust output, so only the drift guard
        # (not the input_all fallback) can refuse this narrowing.
        self.assertFalse(result["input_all"])
        self.assertTrue(result["input_registry"])
        self.assertFalse(result["documentation_only_registry"])
        # The package edge has no reader set of its own; its documentation
        # read still narrows.
        self.assertTrue(result["documentation_only_package_keld-reader"])

    def test_malformed_documentation_inputs_fail_before_selection(self):
        self.documentation_contract()
        for field, invalid in (("consumer", "docs/input.md"), ("consumer", [""]), ("consumer", [None]),
                               ("package", {"keld-reader": "docs/input.md"}), ("package", {"keld-reader": []}),
                               ("package", {"keld-reader": [""]}), ("package", ["docs/input.md"])):
            with self.subTest(field=field, invalid=invalid):
                self.documentation_contract_reset()
                if field == "consumer":
                    self.contract["consumers"][1]["documentation_inputs"] = invalid
                else:
                    self.contract["rust_package_documentation_inputs"] = invalid
                self.save_contract()
                with self.assertRaises(ValueError):
                    ci_inputs.classify(self.root, ["docs/input.md"], paths_only=True)

    def documentation_contract_reset(self):
        self.contract["consumers"][1]["documentation_inputs"] = ["docs/input.md"]
        self.contract["rust_package_documentation_inputs"] = {"keld-reader": ["docs/input.md"]}

    def test_missing_or_malformed_contract_fails_before_selection(self):
        for text in ("invalid", '{}', '[]', 'null', '42', '{"schema":"wrong","consumers":[]}'):
            self.write("tools/ci-inputs.json", text)
            with self.assertRaises(ValueError):
                self.route()
        (self.root / "tools/ci-inputs.json").unlink()
        with self.assertRaises(ValueError):
            self.route()


class FreshnessGateTests(unittest.TestCase):
    """The live gate compares committed digests with live bytes and binds nothing first."""

    tool = Path(__file__).resolve().parent / "ci_inputs.py"

    def run_tool(self, mode, root):
        return subprocess.run([sys.executable, "-B", str(self.tool), mode, str(root)],
                              capture_output=True, text=True, encoding="utf-8")

    def fixture(self):
        temp = tempfile.TemporaryDirectory(prefix="keld-ci-fresh-")
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        for args in (("init", "-q"), ("config", "user.name", "t"), ("config", "user.email", "t@example.invalid")):
            subprocess.run(["git", *args], cwd=root, check=True, capture_output=True)
        (root / "readers").mkdir()
        (root / "readers/check.py").write_text("print(1)\n", encoding="utf-8")
        (root / "tools").mkdir()
        census = ["readers/check.py", "tools/ci-inputs.json"]
        contract = {"schema": ci_inputs.SCHEMA, "known_inputs": ["*"], "consumers": [],
                    "reader_sets": {"fixture": {"patterns": ["readers/*.py"],
                        "sha256": ci_inputs.fingerprint(root, census, ["readers/*.py"])}}}
        (root / "tools/ci-inputs.json").write_text(json.dumps(contract, indent=2) + "\n", encoding="utf-8")
        subprocess.run(["git", "add", "."], cwd=root, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-qm", "bind"], cwd=root, check=True, capture_output=True)
        return root

    def test_fresh_passes_and_stale_fails_with_typed_fix_message(self):
        root = self.fixture()
        self.assertEqual(self.run_tool("--check", root).returncode, 0)
        (root / "readers/check.py").write_text("print(2)\n", encoding="utf-8")
        result = self.run_tool("--check", root)
        self.assertEqual(result.returncode, 1)
        self.assertIn("CI-INPUTS-STALE", result.stderr)
        self.assertIn("fixture", result.stderr)
        self.assertIn("just ci-inputs-rebind", result.stderr)

    def test_rebind_rewrites_only_digest_values_and_names_changed_files(self):
        root = self.fixture()
        before = json.loads((root / "tools/ci-inputs.json").read_text(encoding="utf-8"))
        (root / "readers/check.py").write_text("print(2)\n", encoding="utf-8")
        result = self.run_tool("--rebind", root)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("fixture:", result.stdout)
        self.assertIn("M readers/check.py", result.stdout)
        after = json.loads((root / "tools/ci-inputs.json").read_text(encoding="utf-8"))
        self.assertNotEqual(before["reader_sets"]["fixture"]["sha256"], after["reader_sets"]["fixture"]["sha256"])
        before["reader_sets"]["fixture"]["sha256"] = after["reader_sets"]["fixture"]["sha256"]
        self.assertEqual(before, after, "rebind must not change any scope, input or pattern")
        self.assertEqual(self.run_tool("--check", root).returncode, 0)

    def test_rebind_refuses_untracked_bound_files_and_prints_review_criterion(self):
        root = self.fixture()
        (root / "readers/check.py").write_text("print(2)\n", encoding="utf-8")
        (root / "readers/scratch.py").write_text("print(3)\n", encoding="utf-8")
        before = (root / "tools/ci-inputs.json").read_bytes()
        refused = self.run_tool("--rebind", root)
        self.assertEqual(refused.returncode, 1)
        self.assertIn("readers/scratch.py", refused.stderr)
        self.assertEqual((root / "tools/ci-inputs.json").read_bytes(), before, "refusal must not write")
        # Negative control: removing the untracked file lets the same rebind proceed.
        (root / "readers/scratch.py").unlink()
        done = self.run_tool("--rebind", root)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn("consumers[].inputs", done.stdout)

    def test_unreadable_set_reason_is_reported(self):
        root = self.fixture()
        (root / "readers/check.py").unlink()
        result = self.run_tool("--check", root)
        self.assertEqual(result.returncode, 1)
        self.assertIn("unreadable", result.stderr)

    def test_rebind_survives_an_unreadable_recording_commit(self):
        root = self.fixture()
        (root / "readers/check.py").write_text("print(2)\n", encoding="utf-8")
        real = ci_inputs.subprocess.check_output

        def partial_clone(args, *a, **kw):
            if args[:2] == ["git", "ls-tree"]:
                raise subprocess.CalledProcessError(128, args)
            return real(args, *a, **kw)

        with unittest.mock.patch.object(ci_inputs.subprocess, "check_output", partial_clone), \
                contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(ci_inputs.rebind(root), 0)
        self.assertIn("recording commit unreadable", out.getvalue())
        self.assertIn("readers/check.py", out.getvalue())
        self.assertEqual(self.run_tool("--check", root).returncode, 0, "digests must still be written")

    def test_rebind_is_a_noop_when_fresh(self):
        root = self.fixture()
        before = (root / "tools/ci-inputs.json").read_bytes()
        self.assertEqual(self.run_tool("--rebind", root).returncode, 0)
        self.assertEqual((root / "tools/ci-inputs.json").read_bytes(), before)


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

    def test_unrelated_rust_gates_need_explicit_false_not_missing_output(self):
        for gate in ("fmt-check", "clippy", "test"):
            selected = ci_local.selection(self.route(**{"local_" + gate: "false"}), ["fmt-check", "clippy", "test"])
            self.assertFalse(selected[gate])
            missing = "\n".join(line for line in self.route().splitlines() if not line.startswith("local_" + gate + "="))
            self.assertTrue(ci_local.selection(missing, ["fmt-check", "clippy", "test"])[gate])

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

    def test_typescript_cannot_share_parallel_policy_load_directly_or_through_a_body(self):
        with tempfile.TemporaryDirectory(prefix="keld-ci-ts-schedule-") as temporary:
            root = Path(temporary)
            suffix = "fmt-check clippy test doc deny"
            bodies = "".join(f"{gate}:\n    true\n" for gate in (*suffix.split(), "typescript"))
            for prefix in (
                f"ci-inventory: policy {suffix}\n[parallel]\npolicy: typescript\n",
                f"ci-inventory: policy typescript {suffix}\n[parallel]\npolicy: helper\nhelper: typescript\n    true\n",
            ):
                (root / "justfile").write_text(prefix + bodies, encoding="utf-8")
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
    def test_actual_template_edit_selects_runtime_reader_in_hosted_packages(self):
        source = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory(prefix="keld-ci-package-reader-") as temporary:
            root = Path(temporary)
            tracked_snapshot(source, root)
            for args in (("config", "user.name", "ci-contract-test"),
                         ("config", "user.email", "ci-contract@example.invalid"),
                         ("add", "."), ("commit", "-qm", "bound source")):
                subprocess.run(["git", *args], cwd=root, check=True, capture_output=True)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
            path = root / "crates/keld-cli/templates/hello/src/kipc.ts"
            path.write_text(path.read_text(encoding="utf-8") + "\n// adapter content mutation\n", encoding="utf-8")
            subprocess.run(["git", "commit", "-qam", "adapter edit"], cwd=root, check=True, capture_output=True)
            head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
            environment = os.environ.copy()
            environment.pop("GITHUB_OUTPUT", None)
            environment.update(KELD_CI_EVENT_NAME="pull_request", KELD_CI_BASE_SHA=base,
                               KELD_CI_HEAD_SHA=head)
            result = subprocess.run([shutil.which("bash"), "tools/ci_changes.sh", "github"],
                                    cwd=root, env=environment, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            selection = dict(line.split("=", 1) for line in result.stdout.splitlines())
            packages = set(selection["packages"].split())
            self.assertTrue({"keld-cli", "keld-host", "keld-runtime"} <= packages, selection)
            self.assertNotIn("keld-pack", packages, "external-reader edges must retain selectivity")
            manifest = root / "Cargo.toml"
            manifest.write_text(manifest.read_text(encoding="utf-8") + "\n# shared graph mutation\n", encoding="utf-8")
            subprocess.run(["git", "commit", "-qam", "shared and owned edit"], cwd=root, check=True, capture_output=True)
            environment["KELD_CI_HEAD_SHA"] = subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
            result = subprocess.run([shutil.which("bash"), "tools/ci_changes.sh", "github"],
                                    cwd=root, env=environment, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            selection = dict(line.split("=", 1) for line in result.stdout.splitlines())
            metadata = json.loads(subprocess.check_output(
                ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=root))
            self.assertEqual(set(selection["packages"].split()),
                             {package["name"] for package in metadata["packages"]},
                             "owned inputs must not narrow a shared workspace obligation")

    def test_real_rust_content_edit_and_added_reader_have_distinct_scope(self):
        source = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory(prefix="keld-ci-rust-census-") as temporary:
            root = Path(temporary)
            tracked_snapshot(source, root)
            leaf = "crates/keld-ipc/src/codec.rs"
            path = root / leaf
            path.write_text(path.read_text(encoding="utf-8") + "\n// pure codec implementation edit\n", encoding="utf-8")
            subprocess.run(["git", "add", leaf], cwd=root, check=True, capture_output=True)
            result = ci_inputs.classify(root, [leaf])
            self.assertFalse(result["input_all"])
            for gate in ("fmt-check", "clippy", "test", "doc"):
                self.assertTrue(result["local_" + gate], gate)
            for gate in ("typescript", "llms-check", "llms-test", "doc-placeholders-check",
                         "agent-context-test", "audit-docs-test", "product-status-test", "ci-router-test"):
                self.assertFalse(result["local_" + gate], gate)
            # A new cross-tree read in an existing source also declares its
            # reader/input edge in that same change. Membership alone cannot
            # discover a new read inside an already known file.
            path.write_text(path.read_text(encoding="utf-8") +
                            '\n#[cfg(test)] fn external_reader_probe() { let _ = std::fs::read_to_string("docs/reader-added.md"); }\n',
                            encoding="utf-8")
            declaration = ci_inputs.load(root)
            declaration["reader_sets"]["rust-readers"]["patterns"].append(leaf)
            declaration["reader_sets"]["rust-readers"]["sha256"] = ci_inputs.fingerprint(
                root, ci_inputs.files(root), declaration["reader_sets"]["rust-readers"]["patterns"])
            declaration["consumers"][0]["inputs"].append("docs/reader-added.md")
            (root / "tools/ci-inputs.json").write_text(json.dumps(declaration), encoding="utf-8")
            (root / "docs/reader-added.md").write_text("new reader input\n", encoding="utf-8")
            subprocess.run(["git", "add", "docs/reader-added.md"], cwd=root, check=True, capture_output=True)
            later = ci_inputs.classify(root, ["docs/reader-added.md"])
            self.assertFalse(later["input_all"])
            self.assertTrue(later["local_test"])
            added = "crates/keld-ipc/src/new_external_reader.rs"
            (root / added).write_text('pub fn read() { let _ = std::fs::read("README.md"); }\n', encoding="utf-8")
            subprocess.run(["git", "add", added], cwd=root, check=True, capture_output=True)
            unknown = ci_inputs.classify(root, [added])
            self.assertTrue(unknown["input_all"])
            narrowing = {key: value for key, value in unknown.items() if key.startswith("documentation_only_")}
            self.assertTrue(narrowing, "the production contract declares documentation-only edges")
            self.assertTrue(all(value for key, value in unknown.items() if key not in narrowing),
                            "new unbound membership must never omit a consumer")
            self.assertFalse(any(narrowing.values()), "a fallback must never narrow a reverse-dependent closure")

    def test_committed_digests_match_live_bytes_and_stale_is_caught(self):
        # Unbound snapshot: the digests are exactly what is committed, so this can fail.
        source = Path(__file__).resolve().parent.parent
        fixture = tempfile.TemporaryDirectory(prefix="keld-ci-fresh-prod-")
        self.addCleanup(fixture.cleanup)
        root = Path(fixture.name)
        tracked_snapshot(source, root, bind=False)
        self.assertEqual((root / "tools/ci-inputs.json").read_bytes().replace(b"\r\n", b"\n"),
                         (source / "tools/ci-inputs.json").read_bytes().replace(b"\r\n", b"\n"),
                         "bind=False must keep the committed digests byte-for-byte")
        live = subprocess.run([sys.executable, "-B", str(source / "tools/ci_inputs.py"), "--check", str(root)],
                              capture_output=True, text=True, encoding="utf-8")
        self.assertEqual(live.returncode, 0, live.stderr)
        # Negative control: a deliberately stale digest must fail and keep the fallback.
        path = root / "tools/ci-inputs.json"
        text = path.read_text(encoding="utf-8")
        contract = ci_inputs.load(root)
        victim = "policy-readers"
        recorded = contract["reader_sets"][victim]["sha256"]
        path.write_text(text.replace(recorded, "0" * 64), encoding="utf-8", newline="\n")
        self.assertEqual(ci_inputs.stale(root), [victim])
        failed = subprocess.run([sys.executable, "-B", str(source / "tools/ci_inputs.py"), "--check", str(root)],
                                capture_output=True, text=True, encoding="utf-8")
        self.assertEqual(failed.returncode, 1)
        self.assertIn(victim, failed.stderr)
        selection = ci_inputs.classify(root, [], paths_only=True)
        for consumer in contract["consumers"]:
            if consumer["reader_set"] == victim:
                for output in consumer["outputs"]:
                    self.assertTrue(selection[output], f"stale {victim} must still enable {output}")

    def test_gitattributes_selects_keld_compat_through_its_declared_read(self):
        # gh566 T3: the keld-compat snapshot normalisation check runs `git hash-object`,
        # which reads .gitattributes, so the read is a declared Rust input edge.
        source = Path(__file__).resolve().parent.parent
        live = ci_inputs.load(source)
        rust = [c for c in live["consumers"] if c["owner"] == "Rust workspace external reads"]
        self.assertEqual(len(rust), 1)
        self.assertIn(".gitattributes", rust[0]["inputs"])
        # Today .gitattributes is an unknown input, so the fail-safe selects every lane.
        live_selection = ci_inputs.classify(source, [".gitattributes"], paths_only=True)
        for output in ("input_all", "input_rust", "input_package_keld-compat", "local_test"):
            self.assertTrue(live_selection[output], output)
        # The declared edge alone still selects the Rust test lanes if .gitattributes
        # ever becomes a known input; without the edge, nothing would select them.
        def known(with_edge):
            def load(root):
                contract = json.loads(json.dumps(live))
                contract["known_inputs"] = contract["known_inputs"] + [".gitattributes"]
                if not with_edge:
                    for consumer in contract["consumers"]:
                        consumer["inputs"] = [i for i in consumer["inputs"] if i != ".gitattributes"]
                return contract
            return load
        for with_edge in (True, False):
            with unittest.mock.patch.object(ci_inputs, "load", known(with_edge)):
                selected = ci_inputs.classify(source, [".gitattributes"], paths_only=True)
            self.assertFalse(selected["input_all"], with_edge)
            for output in ("input_rust", "local_test"):
                self.assertIs(selected[output], with_edge, (output, with_edge))
        unrelated = ci_inputs.classify(source, ["README.md"], paths_only=True)
        for output in ("input_rust", "input_package_keld-compat", "local_test"):
            self.assertFalse(unrelated[output], output)

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
        outputs = [output for consumer in contract["consumers"] for output in consumer["outputs"]]
        self.assertEqual(len(outputs), len(set(outputs)), "each consumer output has exactly one owner")
        self.assertTrue({"input_rust", "input_ts", "input_registry", "input_mermaid"} <= set(outputs),
                        "cross-tree consumer owners cannot be removed from the production contract")
        examples = {
            "crates/keld-ipc/src/lib.rs": ("input_ts", "local_fmt-check", "local_clippy", "local_test"),
            "crates/keld-ipc/src/frame.rs": ("input_ts",),
            "crates/keld-ipc/src/echo.rs": ("input_ts",),
            "crates/keld-ipc/src/lifecycle.rs": ("input_ts",),
            "crates/keld-cli/templates/hello/src/main-body.ts": ("input_ts", "input_rust"),
            "tools/atomic_protocol.rs": ("input_registry", "input_rust"),
            "docs/architecture/03-security.md": ("input_rust", "local_llms-check", "local_test"),
            "docs/engineering/keld-error-codes.md": ("input_rust", "local_llms-check"),
            "crates/keld-compat/fixtures/lifecycle-corpus/report.md": ("input_rust", "local_test"),
            "docs/specs/kel53-full-package-activation.md": (
                "input_rust", "input_package_keld-update", "local_test"),
            "crates/keld-host/tests/fixtures/t1b_harness.ts": ("input_ts", "local_typescript"),
            "llms-full.txt": ("input_rust", "local_llms-check"),
            "docs/engineering/product-status.tsv": ("local_product-status-check",),
        }
        for path, outputs in examples.items():
            self.assertTrue((root / path).is_file(), path)
            result = ci_inputs.classify(root, [path], paths_only=True)
            self.assertFalse(result["input_all"], f"{path}: positive example must not hide behind fallback")
            for output in outputs:
                self.assertTrue(result[output], f"{path} must select {output}")
        # #624: documentation-only reads narrow; a non-documentation input of
        # the same edge (the recursive tools scan) never does.
        narrowing = {
            "llms-full.txt": ("documentation_only_registry", True),
            "docs/architecture/02-ipc.md": ("documentation_only_registry", True),
            "docs/engineering/keld-error-codes.md": ("documentation_only_registry", True),
            "tools/atomic_protocol.rs": ("documentation_only_registry", False),
            "crates/keld-cli/src/lib.rs": ("documentation_only_registry", False),
            "docs/specs/kel53-full-package-activation.md": ("documentation_only_package_keld-update", True),
            "crates/keld-pack/tests/fixtures/windows-v0-content.tar": ("documentation_only_package_keld-update", False),
        }
        for path, (key, expected) in narrowing.items():
            self.assertTrue((root / path).is_file(), path)
            self.assertIs(ci_inputs.classify(root, [path], paths_only=True)[key], expected, (path, key))
        source_inventory = (root / "tools/llms_docs.rs").read_text(encoding="utf-8")
        source_inventory = source_inventory.split("const SOURCES: &[Source] = &[", 1)[1].split("\n];", 1)[0]
        corpus_sources = re.findall(r'path: "([^"]+)"', source_inventory)
        self.assertTrue(corpus_sources, "the actual generator source inventory must be discovered")
        for path in corpus_sources:
            result = ci_inputs.classify(root, [path], paths_only=True)
            self.assertFalse(result["input_all"], path)
            for gate in ("fmt-check", "clippy", "test", "llms-check"):
                self.assertTrue(result["local_" + gate], (path, gate))
        for path in ("README.md", "docs/audits/README.md"):
            unrelated = ci_inputs.classify(root, [path], paths_only=True)
            for gate in ("fmt-check", "clippy", "test", "typescript"):
                self.assertFalse(unrelated["local_" + gate], (path, gate))
        unrelated = ci_inputs.classify(root, ["README.md"], paths_only=True)
        for gate in ("agent-context-test", "ci-router-test", "doc", "hooks-test", "product-status-test"):
            self.assertFalse(unrelated["local_" + gate], gate)
        duplicate = "docs/example/receiver-semantics-v0.tsv"
        path = root / duplicate
        path.parent.mkdir(parents=True)
        path.write_bytes((root / "crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv").read_bytes())
        subprocess.run(["git", "add", duplicate], cwd=root, check=True, capture_output=True)
        duplicated_corpus = ci_inputs.classify(root, [duplicate], paths_only=True)
        self.assertFalse(duplicated_corpus["input_all"])
        self.assertTrue(duplicated_corpus["input_ts"])
        self.assertFalse(ci_inputs.classify(root, ["docs/engineering/product-status.tsv"], paths_only=True)["input_ts"])


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--tracked-snapshot":
        tracked_snapshot(Path(__file__).resolve().parent.parent, Path(sys.argv[2]))
    else:
        unittest.main()
