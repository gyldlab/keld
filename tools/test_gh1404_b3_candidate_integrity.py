"""Mutation controls for a source-only B-3 archive/installed-lock receipt."""

import base64
import gzip
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock
import zlib


SOURCE = Path(__file__).resolve().parents[1]
VERIFIER = SOURCE / "tools/gh1404_b3_candidate_integrity.py"
SELECTED = "8884fb212b2c2835d63d00d6021a24dc5a956f1a"

VERIFIER_SPEC = importlib.util.spec_from_file_location("b3_candidate_verifier", VERIFIER)
verifier = importlib.util.module_from_spec(VERIFIER_SPEC)
VERIFIER_SPEC.loader.exec_module(verifier)


def packed(name, version, dependencies, extras=None):
    files = {
        "package.json": (json.dumps({
            "name": name, "version": version, "dependencies": dependencies,
        }) + "\n").encode(),
        "entry.js": b"export const provenance = 'candidate';\n",
    }
    files.update(extras or {})
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:gz") as archive:
        for path, data in sorted(files.items()):
            info = tarfile.TarInfo("package/" + path)
            info.size = len(data)
            info.mode = 0o644
            archive.addfile(info, io.BytesIO(data))
    return output.getvalue(), files


class ArchiveBudgetTest(unittest.TestCase):
    def test_gzip_decoder_error_becomes_labeled_refusal(self):
        data, _ = packed("@keld/api", "0.0.1", {})
        with mock.patch.object(gzip.GzipFile, "read", side_effect=zlib.error("decoder failure")):
            with self.assertRaisesRegex(ValueError, "^fixture: invalid gzip archive$"):
                verifier.archive_members(data, "fixture")

    def test_tar_recursion_at_open_and_iteration_becomes_labeled_refusal(self):
        data, _ = packed("@keld/api", "0.0.1", {})
        for after_first_header in (False, True):
            with self.subTest(after_first_header=after_first_header):
                parsed = 0
                next_member = tarfile.TarFile.next

                def fail_next(archive):
                    nonlocal parsed
                    if parsed >= int(after_first_header):
                        raise RecursionError("parser nesting limit")
                    parsed += 1
                    return next_member(archive)

                with mock.patch.object(tarfile.TarFile, "next", fail_next):
                    with self.assertRaisesRegex(ValueError, "^fixture: tar metadata nesting outside bound$"):
                        verifier.archive_members(data, "fixture")

    def test_inflated_tar_equality_passes_and_one_byte_over_refuses(self):
        data, expected = packed("@keld/api", "0.0.1", {})
        size = len(gzip.decompress(data))
        with mock.patch.object(verifier, "MAX_TAR_STREAM", size, create=True):
            files, directories, count = verifier.archive_members(data, "fixture")
        self.assertEqual({name: (body, 0o644) for name, body in expected.items()}, files)
        self.assertEqual({}, directories)
        self.assertEqual(2, count)
        with mock.patch.object(verifier, "MAX_TAR_STREAM", size - 1, create=True):
            with self.assertRaisesRegex(ValueError, "expanded tar stream outside bound"):
                verifier.archive_members(data, "fixture")

    def test_cumulative_budget_checked_before_extracting_next_member(self):
        data, expected = packed("@keld/api", "0.0.1", {}, {"z-note.txt": b"ordinary file content\n" * 20})
        extract = tarfile.TarFile.extractfile
        visited = []

        def record_extract(archive, member):
            visited.append(member.name)
            return extract(archive, member)

        limit = sum(map(len, expected.values()))
        with mock.patch.object(verifier, "MAX_ARCHIVE", limit):
            files, _, _ = verifier.archive_members(data, "fixture")
        self.assertEqual(set(expected), set(files))
        with mock.patch.object(verifier, "MAX_ARCHIVE", limit - 1):
            with mock.patch.object(tarfile.TarFile, "extractfile", record_extract):
                with self.assertRaisesRegex(ValueError, "^fixture: excessive expanded bytes$"):
                    verifier.archive_members(data, "fixture")
        self.assertEqual(["package/entry.js", "package/package.json"], visited)

    def test_archive_scan_stops_at_first_member_over_count(self):
        data, expected = packed("@keld/api", "0.0.1", {}, {"z-extra.js": b"export {};\n"})
        original = tarfile.TarFile.next
        visited = set()

        def observe_next(archive):
            member = original(archive)
            if member is not None:
                visited.add(member.name)
            return member

        with mock.patch.object(verifier, "MAX_MEMBERS", 1):
            with mock.patch.object(tarfile.TarFile, "next", observe_next):
                with self.assertRaisesRegex(ValueError, "archive member count outside bound"):
                    verifier.archive_members(data, "fixture")
        self.assertEqual({"package/" + name for name in sorted(expected)[:2]}, visited)


class CandidateIntegrityTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="gh1404-b3-integrity-")
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.project = root / "project"
        self.repro = root / "second"
        self.project.mkdir()
        self.repro.mkdir()
        self.source = SOURCE
        (self.project / "vendor").mkdir()
        source_manifest = SOURCE / "crates/keld-cli/tests/fixtures/gh1404-minimal-app/package.json"
        shutil.copyfile(source_manifest, self.project / "package.json")
        self.manifest = json.loads(source_manifest.read_text())
        version = {
            n: json.loads((SOURCE / "packages/@keld" / n / "package.json").read_text())["version"]
            for n in ("api", "kipc", "electron")
        }
        node = json.loads((SOURCE / "packages/@keld/electron/package.json").read_text())[
            "dependencies"
        ]["@types/node"]
        self.specs = [
            ("api", "node_modules/@keld/api", "@keld/api", version["api"],
             {"@keld/kipc": version["kipc"]}),
            ("kipc", "node_modules/@keld/kipc", "@keld/kipc", version["kipc"], {}),
            ("electron", "node_modules/electron", "@keld/electron", version["electron"],
             {"@keld/api": version["api"], "@keld/kipc": version["kipc"], "@types/node": node}),
            ("types-node", "node_modules/@types/node", "@types/node", node,
             {"undici-types": "~8.3.0"}),
            ("undici-types", "node_modules/undici-types", "undici-types", "8.3.0", {}),
        ]
        self.lock = {
            "name": self.manifest["name"],
            "version": self.manifest["version"],
            "lockfileVersion": 3,
            "requires": True,
            "packages": {"": {
                "name": self.manifest["name"], "version": self.manifest["version"],
                "dependencies": self.manifest["dependencies"],
            }},
        }
        for spec in self.specs:
            self._replace_package(spec)
        self._write_lock()

    def _replace_package(self, spec, extras=None):
        short, location, name, version, deps = spec
        prefix = "keld-" + short if short in {"api", "kipc", "electron"} else short
        filename = f"{prefix}-{version}.tgz"
        archive, entries = packed(name, version, deps, extras)
        (self.project / "vendor" / filename).write_bytes(archive)
        (self.repro / filename).write_bytes(archive)
        installed = self.project / location
        for relative, payload in entries.items():
            path = installed / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(payload)
            path.chmod(0o644)
        self.lock["packages"][location] = {
            "name": name,
            "version": version,
            "resolved": "file:vendor/" + filename,
            "integrity": "sha512-" + base64.b64encode(hashlib.sha512(archive).digest()).decode(),
            **({"dependencies": deps} if deps else {}),
        }

    def _write_lock(self):
        (self.project / "package-lock.json").write_text(json.dumps(self.lock, indent=2) + "\n")

    def _add_archive_directory(self, short, directory, mode=0o755, before_files=False):
        spec = next(row for row in self.specs if row[0] == short)
        filename = (
            ("keld-" + short if short in {"api", "kipc", "electron"} else short)
            + "-" + spec[3] + ".tgz"
        )
        before = (self.project / "vendor" / filename).read_bytes()
        output = io.BytesIO()
        with tarfile.open(fileobj=io.BytesIO(before), mode="r:gz") as source:
            with tarfile.open(fileobj=output, mode="w:gz") as archive:
                extra = tarfile.TarInfo("package/" + directory + "/")
                extra.type = tarfile.DIRTYPE
                extra.mode = mode
                if before_files:
                    archive.addfile(extra)
                for member in source.getmembers():
                    archive.addfile(member, source.extractfile(member) if member.isfile() else None)
                if not before_files:
                    archive.addfile(extra)
        packed_archive = output.getvalue()
        (self.project / "vendor" / filename).write_bytes(packed_archive)
        (self.repro / filename).write_bytes(packed_archive)
        self.lock["packages"][spec[1]]["integrity"] = (
            "sha512-" + base64.b64encode(hashlib.sha512(packed_archive).digest()).decode()
        )
        self._write_lock()

    def _mutate_source_after_first_check(self, relative, changed):
        self.isolated_selected_source()
        target = self.source / relative
        original_archive_members = verifier.archive_members
        mutated = False

        def change_during_archive_census(*args):
            nonlocal mutated
            if not mutated:
                target.write_bytes(changed(target.read_bytes()))
                mutated = True
            return original_archive_members(*args)

        with mock.patch.object(
            verifier, "archive_members", side_effect=change_during_archive_census
        ):
            with self.assertRaisesRegex(ValueError, "selected source"):
                verifier.validate(self.source, SELECTED, self.project, self.repro)
        self.assertTrue(mutated, "the test must change the real file after the first check")

    def check(self, expected_ok, message=""):
        result = subprocess.run(
            [sys.executable, str(VERIFIER), "--source-root", str(self.source),
             "--selected-revision", SELECTED, "--candidate-root", str(self.project),
             "--repro-dir", str(self.repro)],
            check=False, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        self.assertEqual(result.returncode == 0, expected_ok, result.stderr + result.stdout)
        if message:
            self.assertIn(message, result.stderr + result.stdout)
        return result

    def isolated_selected_source(self):
        # A separate Git repository with shared read-only object storage avoids
        # registering or mutating another worker's Git worktree/index.
        checkout = Path(self.temp.name) / "immutable-source-fixture"
        commands = [
            ["git", "clone", "--shared", "--no-checkout", str(SOURCE), str(checkout)],
            ["git", "-C", str(checkout), "config", "maintenance.auto", "false"],
            ["git", "-C", str(checkout), "sparse-checkout", "init", "--cone"],
            ["git", "-C", str(checkout), "config", "maintenance.auto", "false"],
            [
                "git", "-C", str(checkout), "sparse-checkout", "set",
                "packages/@keld", "crates/keld-cli/tests/fixtures/gh1404-minimal-app",
            ],
            ["git", "-C", str(checkout), "checkout", "--detach", SELECTED],
        ]
        for command in commands:
            result = subprocess.run(command, capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
        self.source = checkout

    def test_clone_disables_maintenance_before_first_sparse_git_operation(self):
        """Catch a removed first config even if the later census config stays."""
        checkout = Path(self.temp.name) / "immutable-source-fixture"
        original_run = subprocess.run
        inspected = []

        def verify_clone_order(command, *args, **kwargs):
            if (not inspected and command[:4] ==
                    ["git", "-C", str(checkout), "sparse-checkout"]):
                # This is the first Git operation after clone/config in the
                # fixture. A later maintenance setting cannot protect it.
                config = original_run(
                    ["git", "-C", str(checkout), "config", "--local", "--get",
                     "maintenance.auto"],
                    capture_output=True, text=True, check=False,
                )
                self.assertEqual(config.returncode, 0,
                                 "temporary clone must set maintenance.auto first")
                self.assertEqual(config.stdout.strip(), "false")
                inspected.append("sparse-first")
            return original_run(command, *args, **kwargs)

        with mock.patch.object(subprocess, "run", side_effect=verify_clone_order):
            self.isolated_selected_source()
        self.assertEqual(inspected, ["sparse-first"],
                         "did not inspect the first real sparse Git operation")
        selected = original_run(
            ["git", "-C", str(checkout), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        self.assertEqual(selected, SELECTED)

    def test_refuses_uncommitted_post_selected_build_fixture(self):
        self.isolated_selected_source()
        changed = self.source / "crates/keld-cli/tests/fixtures/gh1404-minimal-app/keld.build.ts"
        with changed.open("a", encoding="utf8") as writer:
            writer.write("\n// independently unapproved source modification\n")
        self.check(False, "selected source fixture")

    def test_refuses_matching_modified_source_and_candidate_package(self):
        self.isolated_selected_source()
        selected = self.source / "crates/keld-cli/tests/fixtures/gh1404-minimal-app/package.json"
        content = json.loads(selected.read_text("utf8"))
        content["reviewerUnapprovedField"] = "foreign"
        modified = json.dumps(content, indent=2) + "\n"
        selected.write_text(modified, encoding="utf8")
        (self.project / "package.json").write_text(modified, encoding="utf8")
        self.check(False, "selected source fixture")

    def test_refuses_mutation_of_selected_manifest_after_first_check(self):
        self._mutate_source_after_first_check(
            "crates/keld-cli/tests/fixtures/gh1404-minimal-app/package.json",
            lambda content: content + b"\n ",
        )

    def test_refuses_mutation_of_selected_build_after_first_check(self):
        self._mutate_source_after_first_check(
            "crates/keld-cli/tests/fixtures/gh1404-minimal-app/keld.build.ts",
            lambda content: content + b"\n// swapped after initial source check\n",
        )

    def test_refuses_selected_fixture_symlinked_parent(self):
        self.isolated_selected_source()
        fixture = self.source / "crates/keld-cli/tests/fixtures/gh1404-minimal-app"
        original = fixture.with_name(fixture.name + "-real")
        fixture.rename(original)
        fixture.symlink_to(original.name, target_is_directory=True)
        self.check(False, "directory")

    def test_refuses_candidate_vendor_symlinked_parent(self):
        vendor = self.project / "vendor"
        original = self.project / "vendor-real"
        vendor.rename(original)
        vendor.symlink_to(original.name, target_is_directory=True)
        self.check(False, "directory")

    def test_refuses_candidate_node_modules_symlinked_parent(self):
        installed = self.project / "node_modules"
        original = self.project / "node_modules-real"
        installed.rename(original)
        installed.symlink_to(original.name, target_is_directory=True)
        self.check(False, "directory")

    def test_refuses_installed_member_hardlink_to_uncustodied_path(self):
        installed = self.project / "node_modules/@keld/api/entry.js"
        os.link(installed, self.project / "unowned-hardlink.js")
        self.check(False, "hardlink")

    def test_refuses_archive_directory_missing_from_install(self):
        self._add_archive_directory("api", "orphan")
        self.check(False, "directories")

    def test_refuses_world_writable_archive_directory(self):
        self._add_archive_directory("api", "orphan", mode=0o777)
        self.check(False, "directory mode")

    def test_refuses_archive_file_directory_collision(self):
        self._add_archive_directory("api", "entry.js", before_files=True)
        self.check(False, "collision")

    def test_refuses_unlisted_installed_directory(self):
        (self.project / "node_modules/@keld/api/foreign-dir").mkdir()
        self.check(False, "directories")

    def test_refuses_foreign_top_level_installed_package(self):
        foreign = self.project / "node_modules/foreign-installed-package"
        foreign.mkdir()
        (foreign / "index.js").write_text("export const foreign = true;\n")
        self.check(False, "unexpected installed package")

    def test_refuses_foreign_scoped_keld_provider(self):
        foreign = self.project / "node_modules/@keld/foreign-provider"
        foreign.mkdir()
        (foreign / "index.js").write_text("export const foreign = true;\n")
        self.check(False, "unexpected installed package")

    def test_refuses_foreign_scoped_types_provider(self):
        foreign = self.project / "node_modules/@types/foreign-types"
        foreign.mkdir()
        (foreign / "index.d.ts").write_text("export type Foreign = string;\n")
        self.check(False, "unexpected installed package")

    def test_refuses_foreign_symlinked_installed_package(self):
        outside = self.project / "external-package"
        outside.mkdir()
        (outside / "index.js").write_text("export const foreign = true;\n")
        (self.project / "node_modules/external-package").symlink_to(
            outside, target_is_directory=True,
        )
        self.check(False, "unexpected installed package")

    def test_refuses_foreign_root_module_file(self):
        (self.project / "node_modules/foreign-module.js").write_text(
            "module.exports = 'unqualified';\n",
        )
        self.check(False, "unexpected installed package")

    def test_refuses_foreign_package_added_during_archive_check(self):
        original = verifier.archive_members
        injected = False

        def add_foreign_after_first_census(*args):
            nonlocal injected
            if not injected:
                injected = True
                foreign = self.project / "node_modules/foreign-installed-package"
                foreign.mkdir()
                (foreign / "index.js").write_text("export const foreign = true;\n")
            return original(*args)

        with mock.patch.object(verifier, "archive_members",
                               side_effect=add_foreign_after_first_census):
            with self.assertRaisesRegex(ValueError, "unexpected installed package"):
                verifier.validate(self.source, SELECTED, self.project, self.repro)
        self.assertTrue(injected, "the actual installed tree must change during validation")

    def test_allows_normal_npm_hidden_lock_file(self):
        (self.project / "node_modules/.package-lock.json").write_text("{}\n")
        self.check(True)

    def test_refuses_linked_hidden_npm_lock_file(self):
        outside = self.project / "external-package-lock.json"
        outside.write_text("{}\n")
        (self.project / "node_modules/.package-lock.json").symlink_to(outside)
        self.check(False, "file mode or metadata")

    def test_refuses_world_writable_node_modules_root(self):
        (self.project / "node_modules").chmod(0o777)
        self.check(False, "directory mode")

    def test_refuses_world_writable_installed_directory(self):
        (self.project / "node_modules/@keld/api").chmod(0o777)
        self.check(False, "directory mode")

    def test_accepts_matching_explicit_archive_directory_and_counts_entries(self):
        self._add_archive_directory("api", "empty")
        (self.project / "node_modules/@keld/api/empty").mkdir()
        receipt = json.loads(self.check(True).stdout)
        api = next(row for row in receipt["packages"] if row["name"] == "@keld/api")
        self.assertEqual(api["memberCount"], 3)

    def test_selected_source_receipts_equal_independent_git_blob_digests(self):
        receipt = json.loads(self.check(True).stdout)
        for path, field in (
            ("crates/keld-cli/tests/fixtures/gh1404-minimal-app/package.json",
             "selectedManifestSha256"),
            ("packages/@keld/kipc/scripts/package-artifacts.ts", "producerSourceSha256"),
        ):
            immutable = subprocess.run(
                ["git", "-C", str(SOURCE), "show", SELECTED + ":" + path],
                check=True, capture_output=True,
            ).stdout
            self.assertEqual(receipt[field], hashlib.sha256(immutable).hexdigest())

    def test_healthy_sealed_candidate(self):
        result = self.check(True)
        self.assertIn("UNQUALIFIED_SOURCE_ONLY", result.stdout)

    def test_core_provider_fingerprint_matches_pinned_two_file_vector(self):
        # Reference vector from the Core graph contract: SHA256 of each
        # sorted relative path + NUL + decimal size + NUL + raw SHA256 + NUL.
        # The two-file test KIPC fixture has no archive-local metadata noise.
        actual = json.loads(self.check(True).stdout)
        kipc = next(p for p in actual["packages"] if p["name"] == "@keld/kipc")
        self.assertEqual(
            kipc["candidateCoreUnpackedTreeSha256"],
            "3bff413379d590bd7a7267613a6a19d8d1ea3d87f4313bfe4c506e9a1fdad2b2",
        )
        self.assertEqual(actual["status"], "UNQUALIFIED_SOURCE_ONLY")

    def test_self_consistent_unapproved_archive_changes_candidate_core_fingerprint(self):
        baseline = json.loads(self.check(True).stdout)
        baseline_kipc = next(p for p in baseline["packages"] if p["name"] == "@keld/kipc")
        spec = next(row for row in self.specs if row[0] == "kipc")
        self._replace_package(
            spec,
            {"entry.js": b"export const provenance = 'unqualified variant';\n"},
        )
        self._write_lock()
        changed = json.loads(self.check(True).stdout)
        changed_kipc = next(p for p in changed["packages"] if p["name"] == "@keld/kipc")
        self.assertNotEqual(
            baseline_kipc["candidateCoreUnpackedTreeSha256"],
            changed_kipc["candidateCoreUnpackedTreeSha256"],
        )
        self.assertEqual(changed["status"], "UNQUALIFIED_SOURCE_ONLY")

    def test_rejects_changed_installed_file(self):
        (self.project / "node_modules/@keld/api/entry.js").write_text("modified\n")
        self.check(False, "installed")

    def test_rejects_changed_archive_despite_matching_repro(self):
        path = self.project / "vendor/keld-api-0.0.1.tgz"
        path.write_bytes(path.read_bytes() + b"X")
        shutil.copyfile(path, self.repro / path.name)
        self.check(False, "integrity")

    def test_rejects_changed_integrity_with_matching_files(self):
        row = self.lock["packages"]["node_modules/electron"]
        row["integrity"] = "sha512-" + base64.b64encode(b"\x00" * 64).decode()
        self._write_lock()
        self.check(False, "integrity")

    def test_rejects_lock_claim_of_install_script(self):
        self.lock["packages"]["node_modules/@keld/api"]["hasInstallScript"] = True
        self._write_lock()
        self.check(False, "install script")

    def test_rejects_source_version_substitution_despite_consistent_archive_and_lock(self):
        spec = next(row for row in self.specs if row[0] == "api")
        _, location, name, version, dependencies = spec
        self.assertEqual(version, self.manifest["dependencies"]["@keld/api"])
        other_version = next(row[3] for row in self.specs if row[0] == "types-node")
        archive, entries = packed(name, other_version, dependencies)
        filename = f"keld-api-{version}.tgz"
        (self.project / "vendor" / filename).write_bytes(archive)
        (self.repro / filename).write_bytes(archive)
        for relative, payload in entries.items():
            (self.project / location / relative).write_bytes(payload)
        self.lock["packages"][location]["version"] = other_version
        self.lock["packages"][location]["integrity"] = (
            "sha512-" + base64.b64encode(hashlib.sha512(archive).digest()).decode()
        )
        self._write_lock()
        self.check(False, "source version")

    def test_rejects_mode_substitution(self):
        (self.project / "node_modules/@keld/kipc/entry.js").chmod(0o600)
        self.check(False, "mode")

    def test_rejects_extra_installed_provider_member(self):
        (self.project / "node_modules/@keld/kipc/foreign.js").write_text("foreign\n")
        self.check(False, "unexpected")

    def test_rejects_test_hook_even_when_every_digest_agrees(self):
        spec = next(row for row in self.specs if row[0] == "kipc")
        self._replace_package(spec, {"src/test-hooks.ts": b"export const hook = 1;\n"})
        self._write_lock()
        self.check(False, "test")

    def test_rejects_backslash_archive_member_even_when_installed_matches(self):
        spec = next(row for row in self.specs if row[0] == "api")
        self._replace_package(spec, {"..\\escape.js": b"export const escape = 1;\n"})
        self._write_lock()
        self.check(False, "unsafe archive")

    def test_rejects_cross_cwd_archive_difference(self):
        path = self.repro / "keld-electron-0.0.1.tgz"
        path.write_bytes(path.read_bytes() + b"X")
        self.check(False, "reproduc")

    def test_rejects_alias_redirection(self):
        manifest = json.loads((self.project / "package.json").read_text())
        manifest["dependencies"]["electron"] = "electron@0.0.1"
        (self.project / "package.json").write_text(json.dumps(manifest))
        self.lock["packages"][""]["dependencies"] = manifest["dependencies"]
        self._write_lock()
        self.check(False, "source")


if __name__ == "__main__":
    unittest.main()
