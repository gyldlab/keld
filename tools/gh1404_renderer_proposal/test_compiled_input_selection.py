#!/usr/bin/env python3
"""Fail-closed source-to-compiled-input tests; no installed-app acceptance."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


TOOL = Path(__file__).with_name("verify_compiled_input_selection.py")
spec = importlib.util.spec_from_file_location("g1_compiled_source_check", TOOL)
assert spec and spec.loader
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


class SourceToCompiledInputTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        def required(name):
            value = os.environ.get(name)
            if not value:
                raise RuntimeError(f"{name} is mandatory: provide independently pinned source fixtures")
            path = Path(value).resolve(strict=True)
            if not path.is_dir():
                raise RuntimeError(f"{name} must be a real directory")
            return path
        cls.inputs = required("GH1404_G1_OWNER_SOURCE_INPUTS")
        cls.staged = required("GH1404_G1_STAGED_OVERLAY")
        cls.original = required("GH1404_G1_PINNED_ORIGINAL")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="gh1404-compiled-order-")
        self.addCleanup(self.temp.cleanup)

    def audit(self, inputs=None, staged=None, compiled=None):
        return check.inspect(
            inputs or self.inputs, staged or self.staged,
            self.original, candidate_precompiled=compiled,
        )

    def copy_inputs(self):
        path = Path(self.temp.name) / "owner-inputs"
        shutil.copytree(self.inputs, path)
        return path

    def copy_stage(self):
        path = Path(self.temp.name) / "candidate-stage"
        shutil.copytree(self.staged, path)
        return path

    def copy_original(self):
        path = Path(self.temp.name) / "pristine-source"
        shutil.copytree(self.original, path)
        return path

    def test_real_131_source_order_positive_is_explicitly_not_compiled_qualification(self):
        result = self.audit()
        self.assertEqual(result["status"], "SOURCE_ONLY_INPUT_ORDER_VERIFIED_COMPILED_UNQUALIFIED")
        self.assertEqual(result["includeCount"], 131)
        self.assertEqual(result["patchPositions"], [34, 49])
        self.assertEqual(result["physicalSelectedSourceFiles"], 2)
        self.assertEqual(result["unverifiedPhysicalOriginalFiles"], 129)
        self.assertEqual(len(result["missingOriginalSourcePaths"]), 129)
        self.assertEqual(result["antChain"], "mxgraph/mxClient.js -> client.min.js -> js/app.min.js")
        self.assertEqual(
            result["originalPrecompiledMxClientGitBlob"],
            "d2611e23f620b19889201b44ef1d194fd3a69471",
        )
        self.assertEqual(
            result["originalAppMinGitBlob"],
            "ac88c96641c45fdffb3c64aca92d3329308b2ecf",
        )
        self.assertIsNone(result["physical131InputStreamSha256"])
        self.assertFalse(result["rebuiltCompiledMxClientQualified"])
        self.assertIsNone(result["candidateCompiledMxClientSha256"])
        self.assertIsNone(result["candidateAppMinSha256"])
        self.assertEqual(
            result["metadataSourceOrderDigestSha256"],
            "be1b5c1f50fa715f5eaa0d27194c8948f0db7983d9294738dd3f987b5ce55e35",
        )
        self.assertEqual(len(result["orderedSelectedSourceGitBlobs"]), 131)
        marked = [row for row in result["orderedSelectedSourceGitBlobs"] if row["G1SourcePatch"]]
        self.assertEqual([row["sequence"] for row in marked], [34, 49])
        self.assertTrue(all(row["originalGitBlob"] != row["selectedGitBlob"] for row in marked))

    def test_actual_pristine_precompiled_mxclient_is_refused_as_g1_candidate(self):
        pristine = self.inputs / "drawio-mxgraph-COMPILED-ORIGINAL-mxClient.js"
        with self.assertRaisesRegex(check.Refusal, "PRISTINE_PRECOMPILED_REUSED"):
            self.audit(compiled=pristine)

    def test_non_original_fake_compiled_file_is_not_a_positive(self):
        claimed = Path(self.temp.name) / "unreviewed-compiled.js"
        claimed.write_bytes(b"// fabricated new mxClient bytes: no pinned precompile\\n")
        with self.assertRaisesRegex(check.Refusal, "COMPILED_PROVENANCE_UNQUALIFIED"):
            self.audit(compiled=claimed)

    def test_compiled_qualification_without_a_compiler_input_is_rejected(self):
        with self.assertRaisesRegex(check.Refusal, "COMPILED_PROVENANCE_UNQUALIFIED"):
            check.inspect(self.inputs, self.staged, self.original, require_compiled=True)

    def test_one_byte_g1_leaf_tamper_is_refused_before_input_receipt(self):
        staged = self.copy_stage()
        leaf = staged / "src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js"
        leaf.write_bytes(leaf.read_bytes() + b"X")
        with self.assertRaisesRegex(check.Refusal, "stage|candidate|digest"):
            self.audit(staged=staged)

    def test_both_g1_leaf_and_manifest_tamper_does_not_self_authorize(self):
        staged = self.copy_stage()
        leaf = staged / "src/main/webapp/mxgraph/src/shape/mxText.js"
        leaf.write_bytes(leaf.read_bytes() + b"foreign")
        manifest_path = staged / "source-manifest.json"
        manifest = json.loads(manifest_path.read_text())
        for row in manifest["files"]:
            if row["path"].endswith("/mxText.js"):
                row["candidate_sha256"] = hashlib.sha256(leaf.read_bytes()).hexdigest()
        manifest_path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(check.Refusal, "stage|candidate|digest"):
            self.audit(staged=staged)

    def test_immutable_official_include_source_reorder_is_refused(self):
        owner = self.copy_inputs()
        file = owner / "drawio-mxgraph-src-mxClient.js"
        data = file.read_text()
        left = "mxClient.basePath + '/util/mxSvgCanvas2D.js'"
        right = "mxClient.basePath + '/shape/mxText.js'"
        self.assertIn(left, data)
        self.assertIn(right, data)
        file.write_text(data.replace(left, "@@G1LEFT@@").replace(right, left).replace("@@G1LEFT@@", right))
        with self.assertRaisesRegex(check.Refusal, "immutable|include|digest"):
            self.audit(inputs=owner)

    def test_ant_source_graph_cannot_silently_change_compiled_input(self):
        owner = self.copy_inputs()
        build = owner / "drawio-pinned-build.xml"
        data = build.read_text()
        self.assertIn('name="mxClient.js"', data)
        build.write_text(data.replace('name="mxClient.js"', 'name="src/mxClient.js"', 1))
        with self.assertRaisesRegex(check.Refusal, "immutable|Ant|digest"):
            self.audit(inputs=owner)

    def test_source_tree_one_entry_changed_cannot_reauthorize_itself(self):
        owner = self.copy_inputs()
        path = owner / "pinned-drawio-recursive-tree.json"
        tree = json.loads(path.read_text())
        for node in tree["tree"]:
            if node["path"] == "src/main/webapp/mxgraph/src/util/mxLog.js":
                node["sha"] = "0" * 40
                break
        else:
            self.fail("mutation did not target real first ordered upstream source")
        path.write_text(json.dumps(tree))
        with self.assertRaisesRegex(check.Refusal, "immutable|tree|digest"):
            self.audit(inputs=owner)

    def test_physically_changed_pristine_original_cannot_authorize_compiler_input(self):
        pristine = self.copy_original()
        source = pristine / "src/main/webapp/mxgraph/src/shape/mxText.js"
        source.write_bytes(source.read_bytes() + b"// unauthorized")
        with self.assertRaisesRegex(check.Refusal, "physical original|pristine"):
            check.inspect(self.inputs, self.staged, pristine)

    def test_symlinked_stage_leaf_even_with_identical_bytes_refuses(self):
        staged = self.copy_stage()
        leaf = staged / "src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js"
        outside = Path(self.temp.name) / "same-byte-foreign.js"
        outside.write_bytes(leaf.read_bytes())
        leaf.unlink()
        leaf.symlink_to(outside)
        with self.assertRaisesRegex(
            (OSError, check.Refusal), "symbolic links|symlink|unsafe|source"
        ):
            self.audit(staged=staged)

    def test_symlinked_stage_parent_with_identical_bytes_refuses(self):
        staged = self.copy_stage()
        parent = staged / "src"
        outside = Path(self.temp.name) / "foreign-stage-src"
        parent.rename(outside)
        parent.symlink_to(outside, target_is_directory=True)
        # The two JS files remain byte-identical, but their namespace is no
        # longer anchored under the selected source-stage root.
        with self.assertRaises((OSError, check.Refusal)):
            self.audit(staged=staged)

    def test_symlinked_original_parent_with_identical_bytes_refuses(self):
        original = self.copy_original()
        parent = original / "src"
        outside = Path(self.temp.name) / "foreign-original-src"
        parent.rename(outside)
        parent.symlink_to(outside, target_is_directory=True)
        # Do not claim either original leaf is physically selected merely
        # because a foreign directory contains the matching pinned bytes.
        with self.assertRaises((OSError, check.Refusal)):
            check.inspect(self.inputs, self.staged, original)

    def test_cli_refuses_symlinked_stage_parent_without_success_receipt(self):
        staged = self.copy_stage()
        parent = staged / "src"
        outside = Path(self.temp.name) / "redirected-stage-src"
        parent.rename(outside)
        parent.symlink_to(outside, target_is_directory=True)
        result = subprocess.run(
            [sys.executable, str(TOOL), "--owner-inputs", str(self.inputs),
             "--g1-stage", str(staged), "--pristine-two-source-root", str(self.original)],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertIn("GH1404_G1_COMPILED_INPUT_REFUSED", result.stderr)

    def test_cli_refuses_symlinked_original_parent_without_success_receipt(self):
        original = self.copy_original()
        parent = original / "src"
        outside = Path(self.temp.name) / "redirected-original-src"
        parent.rename(outside)
        parent.symlink_to(outside, target_is_directory=True)
        result = subprocess.run(
            [sys.executable, str(TOOL), "--owner-inputs", str(self.inputs),
             "--g1-stage", str(self.staged), "--pristine-two-source-root", str(original)],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertIn("GH1404_G1_COMPILED_INPUT_REFUSED", result.stderr)

    def test_late_source_parent_swap_after_last_stage_read_cannot_publish_receipt(self):
        staged = self.copy_stage().resolve(strict=True)
        stage_source = staged / "src"
        final_leaf = staged / "src/main/webapp/mxgraph/src/shape/mxText.js"
        old_reader = check.physical
        swapped = False

        def changed_after_successful_read(path, *args, **kwargs):
            nonlocal swapped
            result = old_reader(path, *args, **kwargs)
            if not swapped and Path(path) == final_leaf:
                outside = Path(self.temp.name) / "foreign-after-last-source-read"
                shutil.copytree(stage_source, outside)
                (outside / "main/webapp/mxgraph/src/shape/mxText.js").write_bytes(
                    b"FOREIGN EXECUTABLE SOURCE AFTER VERIFIED READ\n"
                )
                stage_source.rename(staged / "retired-original-src")
                stage_source.symlink_to(outside, target_is_directory=True)
                swapped = True
            return result

        with mock.patch.object(check, "physical", side_effect=changed_after_successful_read):
            with self.assertRaises((OSError, check.Refusal)):
                check.inspect(self.inputs, staged, self.original)
        self.assertTrue(swapped, "fault did not run after the last genuine G1 source read")

    def test_late_same_byte_new_inode_cannot_rebind_source_receipt(self):
        staged = self.copy_stage().resolve(strict=True)
        stage_source = staged / "src"
        final_leaf = staged / "src/main/webapp/mxgraph/src/shape/mxText.js"
        old_reader = check.physical
        replaced = False

        def replace_after_last_read(path, *args, **kwargs):
            nonlocal replaced
            data = old_reader(path, *args, **kwargs)
            if not replaced and Path(path) == final_leaf:
                prior = staged / "retired-original-src"
                stage_source.rename(prior)
                shutil.copytree(prior, stage_source)
                replaced = True
            return data

        with mock.patch.object(check, "physical", side_effect=replace_after_last_read):
            with self.assertRaisesRegex(check.Refusal, "selected physical input changed"):
                check.inspect(self.inputs, staged, self.original)
        self.assertTrue(replaced, "identical-byte new-inode replacement did not run")

    def test_missing_unapproved_source_toolchain_cannot_be_minted_by_two_raw_patches(self):
        result = self.audit()
        self.assertEqual(result["unverifiedPhysicalOriginalFiles"], 129)
        self.assertEqual(result["missingQualifiedToolchain"], ["JDK", "Ant", "Closure compiler.jar", "precompiler assembly"])
        self.assertFalse(result["rebuiltCompiledMxClientQualified"])


if __name__ == "__main__":
    unittest.main()
