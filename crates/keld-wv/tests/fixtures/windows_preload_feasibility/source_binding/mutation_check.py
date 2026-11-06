"""One temporary Python-level fault per process; expected exit is nonzero.

These controls test fixture-audit sensitivity. They are not native admission,
production classifier, Guard, or KIPC negative controls.
"""
from __future__ import annotations

import copy
import sys
import unittest
from unittest import mock

import fixture_census
import source_audit
from test_source_audit import SourceAuditTests

ORIGINAL_CENSUS = fixture_census.census
ORIGINAL_VERIFY = source_audit.verify_sources


def omit_preloads(root, inputs, preloads):
    return ORIGINAL_CENSUS(root, inputs, [])


def omit_helper(root, inputs, preloads):
    return ORIGINAL_CENSUS(root, [name for name in inputs if name != "helper.cjs"], preloads)


def skip_preload_identity(root, manifest):
    modified = copy.deepcopy(manifest)
    modified["files"] = [
        item for item in manifest["files"] if item["path"] != "src/main/electron-preload.js"
    ]
    return ORIGINAL_VERIFY(root, modified)


def skip_main_identity(root, manifest):
    modified = copy.deepcopy(manifest)
    modified["files"] = [
        item for item in manifest["files"] if item["path"] != "src/main/electron.js"
    ]
    return ORIGINAL_VERIFY(root, modified)


MUTANTS = {
    "omit-other-preload": (
        "fixture_census.census", omit_preloads,
        "test_other_preload_changes_paired_input_binding",
    ),
    "omit-helper": (
        "fixture_census.census", omit_helper,
        "test_helper_changes_paired_input_binding",
    ),
    "ignore-preload-digest": (
        "source_audit.verify_sources", skip_preload_identity,
        "test_preload_tamper_refused_by_input_auditor",
    ),
    "ignore-main-digest": (
        "source_audit.verify_sources", skip_main_identity,
        "test_main_tamper_refused_by_input_auditor",
    ),
    "include-permissions-self-reference": (
        "fixture_census.EXCLUDED", "__no_exclusion__",
        "test_permissions_metadata_is_excluded",
    ),
}


def main() -> int:
    if len(sys.argv) != 2 or sys.argv[1] not in MUTANTS:
        print("Choose one: " + ", ".join(MUTANTS))
        return 2
    target, replacement, test_name = MUTANTS[sys.argv[1]]
    with mock.patch(target, replacement):
        result = unittest.TextTestRunner(verbosity=2).run(
            unittest.TestSuite([SourceAuditTests(test_name)])
        )
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
