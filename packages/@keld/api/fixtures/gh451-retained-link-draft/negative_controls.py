"""Review-only mutation controls; never edits the original fixture or other trees."""
from pathlib import Path
import re
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parent
bun = sys.argv[1]
source = (root / "candidate.ts").read_text()
tests = (root / "candidate.test.ts").read_text()
controls = [
    (
        "skip peer negotiation",
        "this.#eligible = port.negotiated && port.selected;",
        "this.#eligible = true;",
        "old and unselected peers",
    ),
    (
        "close after Retained",
        'case "retained":\n      case "cancelled":',
        'case "retained":\n        this.#port.fenceAccepted();\n        // fall through\n      case "cancelled":',
        "Prepare Retained keeps same port",
    ),
    (
        "permit new Prepare while original effect unresolved",
        'if (this.#phase.kind === "unknown")\n      return { kind: "unknown", original: this.#phase.original };',
        'if (this.#phase.kind === "unknown") this.#phase = { kind: "idle" };',
        "prepare lost preserves original identity",
    ),
    (
        "ignore Status original identity",
        '|| !sameOriginal(reply.original, original)',
        '',
        "Status mismatch",
    ),
    (
        "accept late direct Prepared from retired role",
        'if (!this.#port.isCurrent() ||\n        (outcome?.kind !== "accepted" && !this.#port.isLive()))\n      return this.#unknown(original);',
        'if (outcome?.kind !== "accepted" && !this.#port.isLive())\n      return this.#unknown(original);',
        "late Prepared after role retirement",
    ),
    (
        "accept Status correlation alias",
        ' || receipt.correlation === original.correlation',
        '',
        "Status may not reuse unresolved effect correlation",
    ),
    (
        "convert generic failure to Retained",
        '// are not no-effect. Other cancellation dispositions need owner approval.\n  return undefined;',
        '// MUTANT: infer no effect.\n  return { kind: "retained" };',
        "prepare generic-error preserves original identity",
    ),
]
for name, old, new, expected in controls:
    if old not in source:
        raise SystemExit(f"missing mutation preimage: {name}")
    with tempfile.TemporaryDirectory(prefix=".negative-", dir=root) as tmp:
        folder = Path(tmp)
        (folder / "candidate.ts").write_text(source.replace(old, new, 1))
        (folder / "candidate.test.ts").write_text(tests)
        run = subprocess.run(
            [bun, "test", str(folder / "candidate.test.ts"), "-t", expected],
            capture_output=True, text=True, timeout=20, check=False,
        )
        output = run.stdout + run.stderr
        # A compile error or unrelated failure is not a successful control.
        if run.returncode == 0 or not re.search(r"\(fail\) " + re.escape(expected), output):
            print(output)
            raise SystemExit(f"control did not falsify expected test: {name}")
        print(f"RED {name}: {expected} (exit {run.returncode})")
print(f"{len(controls)}/{len(controls)} negative controls detected")
