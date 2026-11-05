"""One source-fixture mutation per process; expected child exit is 1.

Runs only first-party source-shape fixture tests. It does not mutate vendored
preload bytes, invoke a production classifier, or test native admission.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent
FAULTS = {
    "omit-rest-only": "F-B forwards every argument after the channel",
    "mislabel-omitted-arguments": "F-C omission and F-J truncation",
    "bypass-normalize-only": "F-D normalizes the first argument",
}


def main() -> int:
    if len(sys.argv) != 2 or sys.argv[1] not in FAULTS:
        print("Choose one: " + ", ".join(FAULTS))
        return 2
    fault = sys.argv[1]
    corpus = json.loads((ROOT / "classifier_cases.json").read_text(encoding="utf-8"))
    if fault == "omit-rest-only":
        case = next(item for item in corpus["cases"] if item["id"] == "F-B")
        case["source"] = "({ sendMessage: (c, x) => send(c, x) })"
    elif fault == "mislabel-omitted-arguments":
        case = next(item for item in corpus["cases"] if item["id"] == "F-C")
        case["expected_rules"] = []
    else:
        case = next(item for item in corpus["cases"] if item["id"] == "F-D")
        assert case["source"].count("path.normalize(p)") == 1
        case["source"] = case["source"].replace("path.normalize(p)", "p")
    with tempfile.TemporaryDirectory(prefix=".w1-test-", dir=ROOT) as temporary:
        mutated = Path(temporary) / "corpus.json"
        mutated.write_text(json.dumps(corpus), encoding="utf-8")
        environment = dict(os.environ, W1_FIXTURE_CORPUS=str(mutated))
        completed = subprocess.run(
            ["node", "--test", "--test-name-pattern", FAULTS[fault],
             str(ROOT / "source_behavior.test.mjs")],
            env=environment, check=False,
        )
    return completed.returncode


if __name__ == "__main__":
    raise SystemExit(main())
