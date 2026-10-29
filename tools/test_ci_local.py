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
    for args in (("init", "-q"), ("config", "maintenance.auto", "false"), ("config", "core.autocrlf", "false"),
                 ("add", "-f", ".")):
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
        self.git("config", "maintenance.auto", "false")
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
        for args in (("init", "-q"), ("config", "maintenance.auto", "false"), ("config", "user.name", "t"),
                     ("config", "user.email", "t@example.invalid")):
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


class SelectionTests(unittest.TestCase):
    """Selection admission is pure Python, so these run in hosted CI (#650)."""

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

class FmtIncludedBashCompatibilityTests(unittest.TestCase):
    """Exercise the production Rust discovery helper without Bash's mapfile builtin."""

    def test_fmt_included_supports_bash_without_mapfile(self):
        source = Path(__file__).resolve().parent.parent
        bash = "/bin/bash" if sys.platform == "darwin" else shutil.which("bash")
        self.assertTrue(bash, "formatter compatibility coverage requires Bash")
        with tempfile.TemporaryDirectory(prefix="keld-fmt-included-bash-") as temporary:
            root = Path(temporary)
            helper = root / "tools/fmt_included.sh"
            helper.parent.mkdir(parents=True)
            shutil.copyfile(source / "tools/fmt_included.sh", helper)
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text(
                'include!("included.rs");\n#[path = "module.rs"]\nmod module;\n',
                encoding="utf-8", newline="\n")
            (root / "src/included.rs").write_text("pub fn included() {}\n", encoding="utf-8")
            (root / "src/module.rs").write_text("pub fn module() {}\n", encoding="utf-8")
            bin_dir = root / "bin"
            bin_dir.mkdir()
            rustfmt = bin_dir / "rustfmt"
            rustfmt.write_text(
                '#!/bin/sh\n'
                'printf "%s\\n" "$@" >> "$KELD_TEST_RUSTFMT_TRACE"\n',
                encoding="utf-8", newline="\n")
            rustfmt.chmod(0o755)
            subprocess.run(["git", "init", "--quiet"], cwd=root, check=True)
            subprocess.run(["git", "config", "maintenance.auto", "false"], cwd=root, check=True)
            subprocess.run(["git", "add", "tools/fmt_included.sh", "src"], cwd=root, check=True)
            trace = root / "rustfmt-args"
            environment = os.environ.copy()
            environment.update(PATH=str(bin_dir) + os.pathsep + environment["PATH"],
                               KELD_TEST_RUSTFMT_TRACE=trace.as_posix())
            command = (
                "enable -n mapfile 2>/dev/null || true; "
                "set -- --check; source tools/fmt_included.sh \"$@\""
            )
            result = subprocess.run([bash, "--noprofile", "--norc", "-c", command], cwd=root,
                                    env=environment, capture_output=True, text=True, encoding="utf-8")
            self.assertEqual(result.returncode, 0, result.stderr)
            observed = trace.read_text(encoding="utf-8").splitlines()
            self.assertIn("--check", observed)
            self.assertIn("src/included.rs", observed)
            self.assertIn("src/module.rs", observed)


class ExecutorTests(SelectionTests):
    """Inventory and execution drive real recipes through the `just` executable
    (tools/ci_local.py inventory/execute), so these stay local-only: hosted
    runners do not install `just` (#650). Inherits SelectionTests' helpers; the
    selection cases are skipped here because SelectionTests runs them.
    """
    test_unknown_gate_runs_and_malformed_gate_fails = None
    test_unrelated_rust_gates_need_explicit_false_not_missing_output = None

    def run_production_typescript(self, failure=""):
        source = Path(__file__).resolve().parent.parent
        bash = shutil.which("bash")
        self.assertIsNotNone(bash, "recipe tests require the same Bash used by CI")
        with tempfile.TemporaryDirectory(prefix="keld-ci-typescript-") as temporary:
            root = Path(temporary)
            shutil.copyfile(source / "justfile", root / "justfile")
            for package in ("kipc", "api", "electron"):
                (root / "packages/@keld" / package).mkdir(parents=True)
            (root / "bin").mkdir()
            bun = root / "bin/bun"
            bun.write_text(
                '#!/usr/bin/env bash\nset -euo pipefail\n'
                'package=${PWD##*/}\n'
                'printf "%s:%s\\n" "$package" "$*" >> "$KELD_TEST_BUN_TRACE"\n'
                'if [ "$package" = api ] && [ "$*" = "$KELD_TEST_BUN_FAIL" ]; then\n'
                '    exit 37\n'
                'fi\n', encoding="utf-8", newline="\n")
            bun.chmod(0o755)
            trace = root / "trace"
            environment = os.environ.copy()
            environment.update(PATH=str(root / "bin") + os.pathsep + environment["PATH"],
                               KELD_TEST_BUN_TRACE=trace.as_posix(), KELD_TEST_BUN_FAIL=failure)
            result = subprocess.run(["just", "--shell", bash, "typescript"], cwd=root,
                                    env=environment, capture_output=True, text=True, encoding="utf-8")
            return result, trace.read_text(encoding="utf-8").splitlines() if trace.exists() else []

    def test_typescript_recipe_checks_all_packages_with_frozen_installs(self):
        result, observed = self.run_production_typescript()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(observed, [
            "kipc:install --frozen-lockfile", "kipc:run typecheck", "kipc:test",
            "api:install --frozen-lockfile", "api:run typecheck", "api:test",
            "electron:install --frozen-lockfile", "electron:run typecheck", "electron:test",
        ])

    def test_typescript_recipe_stops_at_api_typecheck_or_test_failure(self):
        for failure in ("run typecheck", "test"):
            with self.subTest(failure=failure):
                result, observed = self.run_production_typescript(failure)
                self.assertEqual(result.returncode, 37, result.stderr)
                expected = ["kipc:install --frozen-lockfile", "kipc:run typecheck", "kipc:test",
                            "api:install --frozen-lockfile", "api:run typecheck"]
                if failure == "test":
                    expected.append("api:test")
                self.assertEqual(observed, expected)

    def test_fmt_repairs_rust_before_codegen_and_restamps_native_output(self):
        source = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory(prefix="keld-fmt-order-") as temporary:
            root = Path(temporary)
            outputs = ["crates/keld-cli/templates/hello/src/echo.generated.ts",
                       "packages/@keld/api/src/echo.generated.ts",
                       "packages/@keld/api/src/fs.generated.ts",
                       "packages/@keld/api/src/auth.generated.ts",
                       "packages/@keld/api/src/dialog.generated.ts",
                       "packages/@keld/kipc/src/transport.ts"]
            for relative in outputs + ["justfile", "bunfig.toml", "tools/fmt_included.sh",
                    "packages/@keld/kipc/scripts/echo-codegen.ts",
                    "crates/keld-ipc/src/echo.rs", "crates/keld-ipc/src/channel_table.rs",
                    "crates/keld-native/src/fs.rs", "crates/keld-native/src/auth/mod.rs",
                    "crates/keld-native/src/dialog.rs"]:
                destination = root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source / relative, destination)
            (root / "src").mkdir()
            (root / "Cargo.toml").write_bytes(
                b'[package]\nname = "format-order-control"\nversion = "0.0.0"\nedition = "2024"\n')
            (root / "src/lib.rs").write_bytes(
                b'#[path = "../crates/keld-ipc/src/echo.rs"]\nmod echo;\n'
                b'include!("included.rs");\nmacro_rules! fixture { ($($tokens:tt)*) => {} }\n'
                b'fixture! {\n    #[path = "path.rs"]\n    mod path;\n}\n')
            (root / "src/included.rs").write_bytes(b'pub fn included(  ) {}\n')
            (root / "src/path.rs").write_bytes(b'pub fn path(  ) {}\n')
            echo = root / "crates/keld-ipc/src/echo.rs"
            echo.write_bytes(echo.read_bytes().replace(b"pub struct EchoRequest {", b"pub struct EchoRequest{"))
            subprocess.run(["git", "init", "--quiet"], cwd=root, check=True)
            subprocess.run(["git", "config", "maintenance.auto", "false"], cwd=root, check=True)
            # The standalone Rust crate owns these tracked modules. Native Rust
            # files are untracked generator fixtures without their module trees.
            subprocess.run(["git", "add", "src", "Cargo.toml", "crates/keld-ipc/src/echo.rs"], cwd=root, check=True)
            # Negative control: moving JS generation back to a prerequisite
            # must reject the still-valid, unformatted Rust before repairing it.
            justfile = root / "justfile"
            correct_order = justfile.read_text(encoding="utf-8")
            justfile.write_bytes(correct_order.replace("fmt:\n    cargo fmt --all", "fmt: js-fmt\n    cargo fmt --all")
                                .replace("    just js-fmt\n", "").encode())
            wrong_order = subprocess.run(["just", "fmt"], cwd=root, capture_output=True)
            self.assertNotEqual(wrong_order.returncode, 0)
            self.assertIn(b"rustfmt-normalized", wrong_order.stderr)
            self.assertIn(b"pub struct EchoRequest{", echo.read_bytes())
            justfile.write_bytes(correct_order.encode())
            result = subprocess.run(["just", "fmt"], cwd=root, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", errors="replace"))
            self.assertIn(b"pub struct EchoRequest {", echo.read_bytes())
            self.assertEqual((root / "src/included.rs").read_bytes(), b"pub fn included() {}\n")
            self.assertEqual((root / "src/path.rs").read_bytes(), b"pub fn path() {}\n")
            for command in (["bun", "packages/@keld/kipc/scripts/echo-codegen.ts", "check"],
                            ["bun", "fmt", "--no-script", "--check", "**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}"]):
                checked = subprocess.run(command, cwd=root, capture_output=True)
                self.assertEqual(checked.returncode, 0, checked.stderr.decode("utf-8", errors="replace"))

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
                for args in (("init", "-q"), ("config", "maintenance.auto", "false"),
                             ("config", "user.name", "CI boundary"),
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
    def test_gh569_shipping_host_observer_inputs_select_compat_without_fallback(self):
        source = Path(__file__).resolve().parent.parent
        host_inputs = [
            "crates/keld-host/src/main.rs",
            "crates/keld-host/tests/no_flag_macos.rs",
            "crates/keld-host/tests/no_flag/macos/module_process.rs",
            "crates/keld-host/tests/fixtures/module_process_early.ts",
            "crates/keld-cli/src/boot.rs",
            "crates/keld-cli/src/config.rs",
            "crates/keld-cli/src/template.rs",
        ]
        with tempfile.TemporaryDirectory(prefix="keld-gh569-host-input-route-") as temporary:
            root = Path(temporary)
            tracked_snapshot(source, root)
            self.assertEqual(ci_inputs.stale(root), [], "fresh digest binding before path cases")
            bound_contract = ci_inputs.load(root)
            readers = set(bound_contract["reader_sets"]["rust-readers"]["patterns"])
            for reader in (
                "crates/keld-host/tests/no_flag_macos.rs",
                "crates/keld-host/tests/no_flag/macos/module_process.rs",
                "crates/keld-compat/tests/module_process_corpus.rs",
                "crates/keld-compat/tests/module_process_reference.rs",
            ):
                self.assertIn(reader, readers, "new GH569 reader must stay bound to its source bytes")

            def route(path):
                return subprocess.run(
                    ["bash", "tools/ci_changes.sh", "classify"],
                    cwd=root,
                    input=(path + "\0").encode(),
                    capture_output=True,
                )

            for path in host_inputs:
                with self.subTest(path=path):
                    selected = ci_inputs.classify(root, [path], paths_only=True)
                    self.assertFalse(selected["input_all"], selected)
                    self.assertTrue(selected["input_package_keld-compat"], selected)
                    result = route(path)
                    self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))
                    selection = dict(
                        line.split("=", 1)
                        for line in result.stdout.decode().splitlines()
                        if "=" in line
                    )
                    packages = set(selection["packages"].split())
                    self.assertIn("keld-compat", packages, selection)
                    self.assertFalse(selection["workspace"] == "true", selection)

            # Core/runtime and their Host dependency closure already reach the
            # observer through Cargo reverse dependencies; do not duplicate
            # those paths in rust_package_inputs.
            for path in (
                "crates/keld-core/src/app_session.rs",
                "crates/keld-runtime/src/macos_guardian.rs",
            ):
                with self.subTest(cargo_owned_path=path):
                    selected = ci_inputs.classify(root, [path], paths_only=True)
                    self.assertFalse(selected["input_all"], selected)
                    self.assertFalse(selected["input_package_keld-compat"], selected)
                    result = route(path)
                    self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))
                    selection = dict(
                        line.split("=", 1)
                        for line in result.stdout.decode().splitlines()
                        if "=" in line
                    )
                    self.assertIn("keld-compat", selection["packages"].split(), selection)

            # The Electron fixture is already a declared keld-compat external
            # input; changing it must continue to select the same observer.
            electron_fixture = "packages/@keld/electron/fixtures/module_process_probe.ts"
            electron_selection = ci_inputs.classify(root, [electron_fixture], paths_only=True)
            self.assertFalse(electron_selection["input_all"], electron_selection)
            self.assertTrue(electron_selection["input_package_keld-compat"], electron_selection)

            # Corpus bytes live inside the observing package, so Cargo's direct
            # package route owns them; no redundant external input edge is needed.
            corpus = "crates/keld-compat/fixtures/module-process-corpus/corpus.json"
            corpus_result = route(corpus)
            self.assertEqual(corpus_result.returncode, 0, corpus_result.stderr.decode(errors="replace"))
            corpus_selection = dict(
                line.split("=", 1)
                for line in corpus_result.stdout.decode().splitlines()
                if "=" in line
            )
            self.assertIn("keld-compat", corpus_selection["packages"].split())

            # An unrelated crate must not enter the compatibility observer's
            # package selection merely because Host inputs now have an edge.
            unrelated = "crates/keld-updater-helper/src/lib.rs"
            unrelated_result = route(unrelated)
            self.assertEqual(
                unrelated_result.returncode, 0, unrelated_result.stderr.decode(errors="replace")
            )
            unrelated_selection = dict(
                line.split("=", 1)
                for line in unrelated_result.stdout.decode().splitlines()
                if "=" in line
            )
            self.assertNotIn("keld-compat", unrelated_selection["packages"].split())

            # One-fault negatives: removing either exact external edge loses
            # only its observing package while the direct Host/package route stays.
            current_inputs = bound_contract["rust_package_inputs"]["keld-compat"]
            for edge, path in (
                ("crates/keld-host/**", "crates/keld-host/tests/no_flag_macos.rs"),
                ("crates/keld-cli/src/**", "crates/keld-cli/src/template.rs"),
            ):
                # On the intentionally-red baseline the edge is absent; the
                # first assertion above is the regression's failure-first proof.
                if edge not in current_inputs:
                    continue
                contract = json.loads(json.dumps(bound_contract))
                contract["rust_package_inputs"]["keld-compat"].remove(edge)
                (root / "tools/ci-inputs.json").write_text(
                    json.dumps(contract, indent=2) + "\n", encoding="utf-8"
                )
                with self.subTest(removed_edge=edge):
                    selected = ci_inputs.classify(root, [path], paths_only=True)
                    self.assertFalse(selected["input_all"], selected)
                    self.assertFalse(selected["input_package_keld-compat"], selected)
                    result = route(path)
                    self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))
                    selection = dict(
                        line.split("=", 1)
                        for line in result.stdout.decode().splitlines()
                        if "=" in line
                    )
                    self.assertNotIn("keld-compat", selection["packages"].split())
                (root / "tools/ci-inputs.json").write_text(
                    json.dumps(bound_contract, indent=2) + "\n", encoding="utf-8"
                )

    def test_native_format_configuration_changes_select_both_format_gates(self):
        source = Path(__file__).resolve().parent.parent
        # These are Bun 1.4.4's exact supported config basenames, not a copy of
        # the production input globs. A known path must not need all-gate fallback.
        names = [
            '.oxfmtrc.json',
            '.oxfmtrc.jsonc',
            'oxfmt.config.ts',
            'oxfmt.config.mts',
            'package.json',
            'package.yaml',
            '.prettierrc',
            '.prettierrc.json',
            '.prettierrc.yml',
            '.prettierrc.yaml',
            '.prettierrc.json5',
            '.prettierrc.js',
            'prettier.config.js',
            '.prettierrc.ts',
            'prettier.config.ts',
            '.prettierrc.mjs',
            'prettier.config.mjs',
            '.prettierrc.mts',
            'prettier.config.mts',
            '.prettierrc.cjs',
            'prettier.config.cjs',
            '.prettierrc.cts',
            'prettier.config.cts',
            '.prettierrc.toml',
            '.editorconfig',
        ]
        paths = ["crates/keld-cli/templates/hello/" + name for name in names]
        paths += ["packages/@keld/api/package.json",
                  "crates/keld-cli/templates/hello/.PRETTIERRC",
                  "crates/keld-ipc/fuzz/.prettierrc", ".gitignore",
                  "crates/keld-cli/templates/hello/.GITIGNORE",
                  "crates/keld-cli/templates/hello/.GitIgnore"]
        with tempfile.TemporaryDirectory(prefix="keld-native-config-route-") as temporary:
            root = Path(temporary)
            tracked_snapshot(source, root)
            for path in paths:
                with self.subTest(path=path):
                    selected = ci_inputs.classify(root, [path], paths_only=True)
                    self.assertFalse(selected["input_all"])
                    self.assertTrue(selected["local_js-fmt-check"])
                    self.assertTrue(selected.get("input_hygiene", False))
                    self.assertTrue(selected["local_typescript"])

    def test_root_manifests_keep_unknown_input_fallback(self):
        source = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory(prefix="keld-native-root-manifest-route-") as temporary:
            root = Path(temporary)
            tracked_snapshot(source, root)
            for path in ("package.json", "package.yaml"):
                with self.subTest(path=path):
                    selected = ci_inputs.classify(root, [path], paths_only=True)
                    self.assertTrue(selected["input_all"])
                    self.assertTrue(selected["local_js-fmt-check"])
                    self.assertTrue(selected["local_typescript"])
                    self.assertTrue(selected["local_clippy"])

    def test_nested_template_ignore_changes_select_native_formatter(self):
        source = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory(prefix="keld-native-nested-ignore-route-") as temporary:
            root = Path(temporary)
            tracked_snapshot(source, root)
            selected = ci_inputs.classify(
                root, ["crates/keld-cli/templates/hello/.gitignore"], paths_only=True)
            self.assertFalse(selected["input_all"])
            self.assertTrue(selected["local_js-fmt-check"])

    def test_root_ignore_changes_select_native_formatter(self):
        source = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory(prefix="keld-native-ignore-route-") as temporary:
            root = Path(temporary)
            # Bind fixture readers so unrelated in-progress source drift cannot
            # hide a missing input edge by conservatively selecting every gate.
            tracked_snapshot(source, root)
            selected = ci_inputs.classify(root, [".gitignore"], paths_only=True)
            self.assertFalse(selected["input_all"])
            self.assertTrue(selected["local_js-fmt-check"])

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

    def test_native_fs_inputs_select_bun_with_fresh_bindings_and_exact_edges(self):
        source = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory(prefix="keld-ci-native-fs-readers-") as temporary:
            root = Path(temporary)
            tracked_snapshot(source, root)
            fixture = "crates/keld-native/tests/fixtures/fs-payload-v0.tsv"
            if not (root / fixture).exists():
                # The CI edge can precede the feature artifact; contents are not a codec oracle here.
                (root / fixture).parent.mkdir(parents=True, exist_ok=True)
                (root / fixture).write_text("# native FS semantic fixture routing control\n", encoding="utf-8")
            subprocess.run(["git", "add", "-f", fixture], cwd=root, check=True, capture_output=True)
            # Bind only this disposable fixture's tracked membership/readers. No drift may mask an edge.
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(ci_inputs.rebind(root), 0)
            self.assertEqual(ci_inputs.stale(root), [])
            contract_path = root / "tools/ci-inputs.json"
            original_contract = contract_path.read_bytes()
            cases = (("crates/keld-native/src/fs.rs", True), (fixture, True),
                     ("crates/keld-native/src/lib.rs", False))
            for leaf, expected in cases:
                with self.subTest(input=leaf):
                    path = root / leaf
                    original_input = path.read_bytes()
                    path.write_bytes(original_input + b"\n// route content mutation\n")
                    try:
                        self.assertEqual(ci_inputs.stale(root), [], "input content is not reader drift")
                        selected = ci_inputs.classify(root, [leaf])
                        self.assertFalse(selected["input_all"], selected)
                        for output in ("input_ts", "local_typescript"):
                            self.assertEqual(selected[output], expected, (leaf, output, selected))
                        if expected:
                            contract = ci_inputs.load(root)
                            consumer = next(item for item in contract["consumers"]
                                            if item["owner"] == "Bun package readers")
                            consumer["inputs"].remove(leaf)
                            contract_path.write_text(json.dumps(contract), encoding="utf-8")
                            self.assertEqual(ci_inputs.stale(root), [], "the policy artifact cannot hash itself")
                            omitted = ci_inputs.classify(root, [leaf])
                            self.assertFalse(omitted["input_all"], omitted)
                            for output in ("input_ts", "local_typescript"):
                                self.assertFalse(omitted[output], (leaf, output, omitted))
                    finally:
                        path.write_bytes(original_input)
                        contract_path.write_bytes(original_contract)

    def test_future_macos_host_reader_byte_drift_requires_exact_registration(self):
        readers = (
            "crates/keld-host/tests/no_flag/macos/public_fs/project.rs",
            "crates/keld-host/tests/no_flag/macos/facade_ready.rs",
            "crates/keld-host/tests/no_flag/macos/facade_window.rs",
        )
        # Model main before the facade tests land, even after this test is
        # later run in a tree that already contains those product files.
        with tempfile.TemporaryDirectory(prefix="keld-ci-absent-facade-readers-") as temporary:
            root = Path(temporary)
            tracked_snapshot(source=Path(__file__).resolve().parent.parent, destination=root)
            patterns = ci_inputs.load(root)["reader_sets"]["rust-readers"]["patterns"]
            for reader in readers[1:]:
                self.assertIn(reader, patterns)
                subprocess.run(["git", "rm", "--cached", "-f", "--ignore-unmatch", "--", reader],
                               cwd=root, check=True, capture_output=True)
                (root / reader).unlink(missing_ok=True)
                self.assertFalse((root / reader).exists())
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(ci_inputs.rebind(root), 0)
            self.assertEqual(ci_inputs.stale(root), [])

        for reader in readers:
            with self.subTest(reader=reader):
                source = Path(__file__).resolve().parent.parent
                with tempfile.TemporaryDirectory(prefix="keld-ci-future-fs-reader-") as temporary:
                    root = Path(temporary)
                    tracked_snapshot(source, root)
                    self.assertEqual(ci_inputs.stale(root), [], "the bound snapshot starts fresh")
                    path = root / reader
                    path.parent.mkdir(parents=True, exist_ok=True)
                    original = b"// prospective public API composition reader\n"
                    changed = original + b"// reviewed reader content changed\n"
                    path.write_bytes(original)
                    subprocess.run(["git", "add", "-f", reader], cwd=root, check=True, capture_output=True)
                    with contextlib.redirect_stdout(io.StringIO()):
                        self.assertEqual(ci_inputs.rebind(root), 0)
                    self.assertEqual(ci_inputs.stale(root), [])
                    census_patterns = ci_inputs.load(root)["rust_source_census"]["patterns"]
                    membership = ci_inputs.membership_fingerprint(ci_inputs.files(root), census_patterns)

                    path.write_bytes(changed)
                    error = io.StringIO()
                    with contextlib.redirect_stderr(error):
                        self.assertEqual(ci_inputs.check(root), 1, "registered reader byte drift must reject freshness")
                    self.assertIn("CI-INPUTS-STALE", error.getvalue())
                    self.assertIn("rust-readers", error.getvalue())
                    self.assertEqual(ci_inputs.stale(root), ["rust-readers"])
                    self.assertEqual(ci_inputs.membership_fingerprint(ci_inputs.files(root), census_patterns), membership)

                    # Removing only the reader declaration falsifies the same direct freshness predicate.
                    # Bind the isolated control first so stale hashes or census cannot supply the rejection.
                    path.write_bytes(original)
                    contract = ci_inputs.load(root)
                    contract["reader_sets"]["rust-readers"]["patterns"].remove(reader)
                    (root / "tools/ci-inputs.json").write_text(json.dumps(contract), encoding="utf-8")
                    with contextlib.redirect_stdout(io.StringIO()):
                        self.assertEqual(ci_inputs.rebind(root), 0)
                    self.assertEqual(ci_inputs.stale(root), [])
                    path.write_bytes(changed)
                    with contextlib.redirect_stdout(io.StringIO()):
                        self.assertEqual(ci_inputs.check(root), 0, "omitting the exact reader hides its byte drift")
                    self.assertEqual(ci_inputs.stale(root), [])
                    self.assertEqual(ci_inputs.membership_fingerprint(ci_inputs.files(root), census_patterns), membership)

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
        # A bound tracked snapshot, so an unrelated dirty edit in the developer's checkout
        # cannot turn on a fallback and mask the controls below.
        source = Path(__file__).resolve().parent.parent
        fixture = tempfile.TemporaryDirectory(prefix="keld-ci-gitattributes-")
        self.addCleanup(fixture.cleanup)
        root = Path(fixture.name)
        tracked_snapshot(source, root)
        live = ci_inputs.load(root)
        rust = [c for c in live["consumers"] if c["owner"] == "Rust workspace external reads"]
        self.assertEqual(len(rust), 1)
        self.assertIn(".gitattributes", rust[0]["inputs"])
        # Today .gitattributes is an unknown input, so the fail-safe selects every lane.
        live_selection = ci_inputs.classify(root, [".gitattributes"], paths_only=True)
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
                selected = ci_inputs.classify(root, [".gitattributes"], paths_only=True)
            self.assertFalse(selected["input_all"], with_edge)
            for output in ("input_rust", "local_test"):
                self.assertIs(selected[output], with_edge, (output, with_edge))
        unrelated = ci_inputs.classify(root, ["README.md"], paths_only=True)
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


# Git 2.54 made `geometric` the default strategy of the `git maintenance run
# --auto --detach` child that commit, merge, fetch, am, rebase and receive-pack
# start. Past 256 loose objects (or with packs to merge) it repacks in a
# daemonized process that outlives the git command, so deleting a throwaway
# repository races its writes: #656's run 37986565916 failed `TemporaryDirectory`
# cleanup with ENOTEMPTY on `.git` (#670).
# `maintenance.auto=false` is the one switch every one of those callers reads
# (prepare_auto_maintenance, run-command.c), so each repository a tracked
# Python, shell, Rust or TypeScript file creates sets it in its own config
# straight after `git init`. A repository config, unlike a `-c` flag, also binds
# the git commands the code under test runs. Commits elsewhere run in one of
# these repositories or in the real checkout, so the `git init` site is the census.
# `gc.auto` stays unset because the switch also covers auto-gc. Checked against
# the git v2.55.0 source: the only `gc --auto` is the maintenance `gc` task
# (builtin/gc.c:1257-1263), and the only `maintenance run --auto` is spawned by
# prepare_auto_maintenance (run-command.c:1956-1984), which returns first when
# maintenance.auto is false. Its callers are am, commit, fetch, merge, rebase and
# receive-pack. Git before 2.29 has no maintenance.auto and ran `gc --auto`
# itself; ubuntu-latest has 2.55.0.
QUOTED_GIT_INIT = re.compile(r"""["'`]init["'`]""")
# An unquoted shell `init` after `git` or a `$GIT*` variable anywhere on the line,
# so `cd x && git init`, `(cd x; git init)`, `"$GIT" init` and `command git init`
# are sites too. The pattern is unanchored and case-blind, so it fails closed.
SHELL_GIT_INIT = re.compile(r"(?:\bgit\b|\$\{?git\w*\}?).*\sinit(?=[\s;&|)]|$)", re.IGNORECASE)
# Fail closed: in a file that mentions git anywhere, in any case (`git`, `GIT`,
# `GIT_EXE`, `gitPath`, `runGit`), every quoted "init" in code is a site,
# whichever wrapper, argv variable, builder chain, `+=` or loop carries it to git.
# No call-shape attribution decides that, so a new shape cannot escape the census.
# The only exemptions are shapes that cannot be a git argument: a comparison
# operand (`phase == "init"`) and a match arm or case label (`"init" =>`,
# `case "init":`). A file that never mentions git cannot run it. Any other non-git
# "init" is spelled in two parts ("in" "it"), as this file does.
GIT_MENTION = re.compile("git", re.IGNORECASE)
NOT_GIT_ARGUMENT_BEFORE = re.compile(r"(?:[=!]=|\bcase)\s*$")
NOT_GIT_ARGUMENT_AFTER = re.compile(r"^\s*(?:[=!]=|=>)")
# Only an executable `config` call counts: a comment that names the setting, or
# the key without `config`, configures nothing. That includes a whole-line `#` or
# `//` comment of a script embedded in a string, such as a Rust raw string.
MAINTENANCE_OFF = re.compile(r"\bconfig\W+maintenance\.auto\W+false\b")
EMBEDDED_COMMENT_LINE = re.compile(r"""^\s*["'`]?\s*(?:#|//)""")
MAINTENANCE_OFF_WINDOW = 4
TEMP_REPOSITORY_SOURCES = ("*.py", "*.sh", "*.rs", "*.ts")
# Per language: (string delimiters, line-comment marker, `/* */` block comments,
# triple-quoted strings, a `#` comment needs a word start, char literals that hold
# a string delimiter). Strings stay, since git argv lives in them; comments go.
RUST_QUOTE_CHARS = ("'\"'", "'\\\"'")
SOURCE_SYNTAX = {".py": ("'\"", "#", False, True, False, ()), ".sh": ("'\"", "#", False, False, True, ()),
                 ".rs": ('"', "//", True, False, False, RUST_QUOTE_CHARS),
                 ".ts": ("'\"`", "//", True, False, False, ())}


def source_code_lines(relative: str, text: str) -> list[str]:
    """Return each line of `text` with its comments blanked and its strings kept.

    A trailing `#` or `//` comment and every `/* */` block comment configure
    nothing, so the census reads only code. A shell `#` starts a comment only at a
    word start. Strings are tracked per line. Python triple-quoted strings are
    kept verbatim: a test may write and run the script one holds, so the census
    cannot tell a docstring from code. A Rust `'"'` char literal opens no string.
    """
    quotes, marker, block, triples, word_start, quote_chars = SOURCE_SYNTAX[Path(relative).suffix]
    code, in_block, triple = [], False, None
    for line in text.splitlines():
        out, index, quote = [], 0, None
        while index < len(line):
            if triple:
                end = line.find(triple, index)
                stop = len(line) if end < 0 else end + 3
                out.append(line[index:stop])
                triple, index = (triple if end < 0 else None), stop
            elif in_block:
                end = line.find("*/", index)
                in_block, index = end < 0, len(line) if end < 0 else end + 2
            elif quote:
                out.append(line[index:index + 2] if line[index] == "\\" else line[index])
                quote = None if line[index] == quote else quote
                index += 2 if line[index] == "\\" else 1
            elif triples and line.startswith(('"""', "'''"), index):
                triple = line[index:index + 3]
                out.append(triple)
                index += 3
            elif quote_chars and line.startswith(quote_chars, index):
                literal = next(char for char in quote_chars if line.startswith(char, index))
                out.append(literal)
                index += len(literal)
            elif line[index] in quotes:
                quote = line[index]
                out.append(quote)
                index += 1
            elif block and line.startswith("/*", index):
                in_block, index = True, index + 2
            elif line.startswith(marker, index) and not (word_start and index and not line[index - 1].isspace()):
                break
            else:
                out.append(line[index])
                index += 1
        code.append("".join(out))
    return code


def quoted_init_cannot_be_git_argument(line: str, match: re.Match) -> bool:
    """Whether the quoted init literal `match` in code `line` cannot be a git argument."""
    return bool(NOT_GIT_ARGUMENT_BEFORE.search(line[:match.start()])
                or NOT_GIT_ARGUMENT_AFTER.match(line[match.end():]))


def temp_repository_init_sites(root: Path) -> dict[str, bool]:
    """Map each `git init` site in tracked source to whether it disables maintenance.

    Source is every tracked .py, .sh, .rs and .ts file present in the working
    tree, read with comments blanked. A `config maintenance.auto false` call
    outside an embedded whole-line comment must appear on the init line or within
    the next MAINTENANCE_OFF_WINDOW lines, so it is the repository's next
    configuration step.
    """
    tracked = subprocess.check_output(["git", "ls-files", "-z", "--", *TEMP_REPOSITORY_SOURCES], cwd=root)
    sites = {}
    for raw in tracked.split(b"\0"):
        if not raw:
            continue
        relative = raw.decode("utf-8")
        path = root / relative
        if not path.is_file():
            continue  # Deleted in the working tree, or a directory symlink: nothing here runs.
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError as error:
            raise AssertionError(f"#670 census cannot read tracked {relative} as UTF-8 ({error}); "
                                 "a tracked source file must be text") from error
        code, mentions_git = source_code_lines(relative, text), bool(GIT_MENTION.search(text))
        for index, line in enumerate(code):
            if SHELL_GIT_INIT.search(line) or mentions_git and any(
                    not quoted_init_cannot_be_git_argument(line, match) for match in QUOTED_GIT_INIT.finditer(line)):
                window = code[index:index + 1 + MAINTENANCE_OFF_WINDOW]
                sites[f"{relative}:{index + 1}"] = any(
                    MAINTENANCE_OFF.search(item) and not EMBEDDED_COMMENT_LINE.match(item) for item in window)
    return sites


class TempRepositoryMaintenanceTests(unittest.TestCase):
    """#670: no throwaway repository may start detached background maintenance.

    The hosted router job runs this on every event. Locally, a source edit outside
    the router self-test inputs does not select it (tools/ci-inputs.json contract).
    """

    # Spelled in two parts because this file is itself in the census.
    VERB = "in" "it"

    def census(self, files, deleted=()):
        with tempfile.TemporaryDirectory(prefix="keld-ci-maintenance-census-") as temporary:
            root = Path(temporary)
            for args in ((self.VERB, "-q"), ("config", "maintenance.auto", "false")):
                subprocess.run(["git", *args], cwd=root, check=True, capture_output=True)
            for relative, text in files.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text.format(verb=self.VERB), encoding="utf-8", newline="\n")
            subprocess.run(["git", "add", "."], cwd=root, check=True, capture_output=True)
            for relative in deleted:
                (root / relative).unlink()
            return temp_repository_init_sites(root)

    def test_every_temp_repository_disables_background_maintenance(self):
        sites = temp_repository_init_sites(Path(__file__).resolve().parent.parent)
        self.assertEqual([site for site, disabled in sites.items() if not disabled], [],
                         "set `config maintenance.auto false` right after each `git init` (#670)")
        # Non-vacuous: every helper language and call shape is recognized.
        helpers = {site.rsplit(":", 1)[0] for site in sites}
        for helper in ("tools/test_ci_local.py", "tools/ci_changes_test.sh", "tools/hooks_test.sh",
                       "tools/ci_hygiene_tests.rs", "tools/mermaid_docs.rs", "tools/product_status.rs",
                       "tools/ci_workflow_security.test.ts", "tools/retained_fs_artifact_test.py",
                       "tools/test_session_closeout.py", "tools/test_workspace.py",
                       "docs/audits/test_verify.py", "crates/keld-wv/tests/linux_media_checkout_test.sh",
                       "crates/keld-compat/tests/corpus_registry/rules/v1_snapshots.rs"):
            self.assertIn(helper, helpers)

    def test_census_rejects_an_init_without_the_setting(self):
        sites = self.census({
            "tools/bare.py": 'self.git("{verb}", "--quiet")\nself.git("add", ".")\n',
            "tools/bare.sh": 'git -C "$d" {verb} -q\ngit -C "$d" commit -qm base\n',
            "tools/bare.rs": 'git_fixture(&root, &["{verb}", "--quiet"]);\n',
            "tools/enabled.ts": 'git("{verb}");\ngit("config", "maintenance.auto", "true");\n',
            "tools/late.py": 'git("{verb}")\n' + "pass\n" * 4 + 'git("config", "maintenance.auto", "false")\n',
            "tools/wrong_key.sh": 'git {verb} -q\ngit config gc.auto 0\n',
            # A comment, or the key without a `config` call, configures nothing.
            "tools/commented.sh": 'git {verb} -q\n# git config maintenance.auto false\n',
            "tools/commented.rs": 'git_fixture(root, &["{verb}"]);\n    // config maintenance.auto false\n',
            "tools/no_call.py": 'git("{verb}")  # maintenance.auto false\n',
            # A trailing or block comment that holds the whole call configures nothing.
            "tools/trailing.py": 'git("{verb}")  # git("config", "maintenance.auto", "false")\n',
            "tools/trailing.sh": 'git {verb} -q  # git config maintenance.auto false\n',
            "tools/trailing.rs": 'git_fixture(root, &["{verb}"]); // git config maintenance.auto false\n',
            "tools/block.rs": 'git_fixture(root, &["{verb}"]);\n/*\n'
                              'git_fixture(root, &["config", "maintenance.auto", "false"]);\n*/\n',
            "tools/inline_block.ts": 'git("{verb}"); /* git("config", "maintenance.auto", "false"); */\n',
            # Outside tools/: a Python audit test and a Rust crate test are in the census.
            "docs/audits/bare.py": 'subprocess.run(["git", "{verb}", "-q"], cwd=repo, check=True)\n',
            "crates/x/tests/bare.rs": 'Command::new("git")\n    .args(["{verb}", "-q"])\n',
            # A whole-line comment of a script embedded in a Rust raw string configures
            # nothing, and a `'"'` char literal opens no string that hides a comment.
            "tools/raw_script.rs": 'let script = r#"\ngit {verb} -q\n# git config maintenance.auto false\n"#;\n',
            "tools/char_quote.rs": 'git_fixture(root, &["{verb}"]); let q = \'"\'; // git config maintenance.auto false\n',
            # #672 re-review: a shell init that does not start the line, and a template literal.
            "tools/after_cd.sh": 'cd "$d" && git {verb} -q\n',
            "tools/subshell.sh": '(cd x; git {verb})\n',
            "tools/git_var.sh": '"$GIT" {verb} -q\n',
            "tools/braced_var.sh": '${{GIT_BIN}} {verb}\n',
            "tools/command.sh": 'command git {verb} -q\n',
            "tools/template.ts": 'execFileSync("git", [`{verb}`], {{ cwd: dir }});\n',
            # Not a source extension: prose is not scanned.
            "docs/notes.md": 'git {verb} -q\nrun `git("{verb}")`\n',
        })
        self.assertEqual(sites, {"tools/bare.py:1": False, "tools/bare.sh:1": False,
                                 "tools/bare.rs:1": False, "tools/enabled.ts:1": False,
                                 "tools/late.py:1": False, "tools/wrong_key.sh:1": False,
                                 "tools/commented.sh:1": False, "tools/commented.rs:1": False,
                                 "tools/no_call.py:1": False, "tools/trailing.py:1": False,
                                 "tools/trailing.sh:1": False, "tools/trailing.rs:1": False,
                                 "tools/block.rs:1": False, "tools/inline_block.ts:1": False,
                                 "docs/audits/bare.py:1": False, "crates/x/tests/bare.rs:2": False,
                                 "tools/raw_script.rs:2": False, "tools/char_quote.rs:1": False,
                                 "tools/after_cd.sh:1": False, "tools/subshell.sh:1": False,
                                 "tools/git_var.sh:1": False, "tools/braced_var.sh:1": False,
                                 "tools/command.sh:1": False, "tools/template.ts:1": False})

    def test_census_accepts_each_supported_form(self):
        sites = self.census({
            "tools/a.py": 'for args in (("{verb}", "-q"), ("config", "maintenance.auto", "false")):\n'
                          '    subprocess.run(["git", *args], cwd=root, check=True)\n',
            "tools/b.sh": 'git -C "$d" {verb} -q\ngit -C "$d" config maintenance.auto false\n',
            "tools/c.rs": 'git_fixture(root, &["{verb}", "--quiet"]);\n'
                          'git_fixture(root, &["config", "maintenance.auto", "false"]);\n',
            "tools/d.ts": 'git("{verb}");\n' + "\n" * 3 + 'git("config", "maintenance.auto", "false");\n',
            # A trailing comment after the call, a `#` inside a string, or a shell `#`
            # inside a word keeps the call.
            "tools/e.py": 'self.git("{verb}")\nself.git("config", "maintenance.auto", "false")  # #670\n',
            "tools/f.sh": 'git -C "$d#1" {verb} -q\ngit -C $d#1 config maintenance.auto false\n',
            # An executable line of a script embedded in a Rust raw string configures.
            "tools/g.rs": 'let script = r#"\ngit {verb} -q\ngit config maintenance.auto false\n"#;\n',
        })
        self.assertEqual(sites, {"tools/a.py:1": True, "tools/b.sh:1": True, "tools/c.rs:1": True,
                                 "tools/d.ts:1": True, "tools/e.py:1": True, "tools/f.sh:1": True,
                                 "tools/g.rs:2": True})

    def test_census_counts_every_quoted_init_in_a_file_that_mentions_git(self):
        # Fail closed: no call-shape attribution decides a site, so every shape that can
        # carry "init" to git is a site, including the ones a git-argument heuristic misses.
        sites = self.census({
            "tools/callee.py": 'census.git(repo, "{verb}", "-q")\n',
            "tools/argv.py": 'subprocess.run(["git", "{verb}", "-q"], cwd=repo, check=True)\n',
            "tools/method.rs": 'temp.git(&["{verb}", "--quiet"]);\n',
            "tools/wrapped.rs": 'git_fixture(\n    root,\n    &["{verb}", "--quiet"],\n);\n',
            "tools/tuple_loop.py": 'for args in (("{verb}", "-q"),\n             ("add", ".")):\n'
                                   '    subprocess.run(["git", *args], cwd=repo, check=True)\n',
            "tools/command_loop.rs": 'for args in [\n    &["-c", "x=y", "{verb}", "-q"][..],\n] {{\n'
                                     '    let status = std::process::Command::new("git")\n'
                                     '        .args(args)\n        .status();\n}}\n',
            "tools/of_loop.ts": 'for (const args of [["{verb}"], ["add", "."]]) {{\n  git(...args);\n}}\n',
            # #672 re-review: shapes a git-argument heuristic silently missed.
            "tools/exec_path.ts": 'execFileSync(gitPath, ["{verb}"], {{ cwd: dir }});\n',
            "tools/run_git.ts": 'runGit(dir, ["{verb}"]);\n',
            "tools/git_const.py": 'subprocess.run([GIT, "{verb}"], cwd=repo, check=True)\n',
            "tools/git_exe.py": 'subprocess.run(\n    [\n        GIT_EXE,\n        "{verb}",\n    ],\n    cwd=repo,\n)\n',
            "tools/wrapper.py": 'def run(*args):\n    subprocess.run(["git", *args], cwd=repo, check=True)\n\n\n'
                                'run("{verb}", "-q")\n',
            "tools/arg.rs": 'let mut c = Command::new("git");\nc.arg("{verb}");\n',
            "tools/chain.rs": 'let status = Command::new("git")\n    .current_dir(&root)\n    .env("A", "1")\n'
                              '    .env("B", "2")\n    .env("C", "3")\n    .env("D", "4")\n'
                              '    .arg("{verb}")\n    .status();\n',
            "tools/append.py": 'cmd = ["git"]\ncmd += ["{verb}"]\nsubprocess.run(cmd, check=True)\n',
            "tools/bound_loop.py": 'steps = [("{verb}", "-q"), ("add", ".")]\nfor step in steps:\n'
                                   '    subprocess.run(["git", *step], check=True)\n',
            # Shapes that look unrelated still count in a file that runs git.
            "tools/label.ts": 'log("git:", "{verb}");\n',
            "tools/separate.ts": 'git("status"); phase("{verb}");\n',
            "tools/loop_other_name.py": 'for step in (("{verb}",),):\n    subprocess.run(["git", *args])\n',
            # Exempt: a comparison operand, a match arm and a case label cannot be git
            # arguments; nor can a comment, a split literal, or a file that never mentions git.
            "tools/compare.py": 'subprocess.run(["git", "status"])\nif phase == "{verb}":\n    pass\n'
                                'if "{verb}" != phase:\n    pass\n',
            "tools/arm.rs": 'Command::new("git");\nmatch phase {{\n    "{verb}" => Phase::Start,\n}}\n',
            "tools/case.py": 'subprocess.run(["git", "status"])\nmatch phase:\n    case "{verb}":\n        pass\n',
            "tools/case.ts": 'git("status");\nswitch (phase) {{\n  case "{verb}":\n    break;\n}}\n',
            "tools/commented.rs": '// git_fixture(root, &["{verb}"]);\n/* git("{verb}") */\n',
            "tools/commented.py": 'x = 1  # self.git("{verb}")\n',
            "tools/split.py": 'subprocess.run(["git", "status"])\nVERB = "in" "it"\n',
            "tools/state.py": 'state = "{verb}"\nif phase == "{verb}":\n    log("{verb}")\n',
            "tools/other_loop.py": 'for step in (("{verb}", "-q"),):\n    print(step)\n',
        })
        self.assertEqual(sites, {"tools/callee.py:1": False, "tools/argv.py:1": False,
                                 "tools/method.rs:1": False, "tools/wrapped.rs:3": False,
                                 "tools/tuple_loop.py:1": False,
                                 "tools/command_loop.rs:2": False, "tools/of_loop.ts:1": False,
                                 "tools/exec_path.ts:1": False, "tools/run_git.ts:1": False,
                                 "tools/git_const.py:1": False, "tools/git_exe.py:4": False,
                                 "tools/wrapper.py:5": False, "tools/arg.rs:2": False,
                                 "tools/chain.rs:7": False, "tools/append.py:2": False,
                                 "tools/bound_loop.py:1": False, "tools/label.ts:1": False,
                                 "tools/separate.ts:1": False, "tools/loop_other_name.py:1": False})

    def test_census_skips_a_tracked_source_deleted_from_the_working_tree(self):
        # A tracked path missing from the working tree runs nothing; reading it must not crash.
        sites = self.census({"tools/gone.py": 'git("{verb}")\n', "tools/kept.py": 'git("{verb}")\n'},
                            deleted=("tools/gone.py",))
        self.assertEqual(sites, {"tools/kept.py:1": False})

    def test_tracked_snapshot_commit_starts_no_background_maintenance(self):
        with tempfile.TemporaryDirectory(prefix="keld-ci-maintenance-") as temporary:
            source, snapshot = Path(temporary) / "source", Path(temporary) / "snapshot"
            source.mkdir()
            for args in (("init", "-q"), ("config", "maintenance.auto", "false")):
                subprocess.run(["git", *args], cwd=source, check=True, capture_output=True)
            (source / "file.txt").write_text("tracked\n", encoding="utf-8")
            subprocess.run(["git", "add", "file.txt"], cwd=source, check=True, capture_output=True)
            tracked_snapshot(source, snapshot, bind=False)
            environment = {**os.environ, "GIT_TRACE": "1"}

            def commit(message):
                # No detach: the control's maintenance child must finish before cleanup.
                return subprocess.run(
                    ["git", "-c", "user.name=t", "-c", "user.email=t@example.invalid",
                     "-c", "maintenance.autoDetach=false", "commit", "-q", "--allow-empty", "-m", message],
                    cwd=snapshot, env=environment, check=True, capture_output=True, text=True).stderr

            self.assertNotIn("maintenance run", commit("snapshot"))
            # Negative control: the same commit with the switch on starts maintenance.
            subprocess.run(["git", "config", "maintenance.auto", "true"], cwd=snapshot, check=True)
            self.assertIn("maintenance run --auto", commit("control"))


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--tracked-snapshot":
        tracked_snapshot(Path(__file__).resolve().parent.parent, Path(sys.argv[2]))
    else:
        unittest.main()
