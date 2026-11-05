"""Run the bounded native matrix serially after obtaining the Ramani GUI slot.

Use through just work-run. An outer timeout quarantines the case and stops this
driver. It does not kill a process, infer cancellation, or delete its profile.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

from matrix_assertions import (assert_digest_rejection, assert_natural_forcing,
                               assert_order, assert_stale_delivery)
from prepare_probe import prepare
from native_census import observe as observe_resources


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def launch(binary, case, config, expected_nonzero=False):
    out_dir = Path(config["outDir"])
    write_json(case / "config.json", config)
    command = [str(binary), "--config", str(case / "config.json")]
    confirmed_exit = False
    native_error = None
    try:
        with (case / "stdout.txt").open("w", encoding="utf-8") as stdout:
            with (case / "stderr.txt").open("w", encoding="utf-8") as stderr:
                process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
                try:
                    code = process.wait(timeout=90)
                    confirmed_exit = True
                except subprocess.TimeoutExpired:
                    write_json(case / "outer-timeout.json", {
                        "pid": process.pid, "command": command,
                        "state": "quarantined; process may still run",
                        "cleanup": "unconfirmed; no following case launched",
                    })
                    raise RuntimeError("Native process exceeded outer bound; no cancellation inferred")
        write_json(case / "process.json", {"pid": process.pid, "exitCode": code, "command": command})
        result_path = out_dir / "result.json"
        if not result_path.is_file():
            raise RuntimeError(f"Native result missing after exit {code}: {result_path}")
        result = json.loads(result_path.read_text(encoding="utf-8"))
        if (code != 0) is not expected_nonzero:
            raise RuntimeError(f"Unexpected native exit {code}; inspect {case}")
        return result
    except Exception as error:
        native_error = error
        raise
    finally:
        # A confirmed exit requires this independent observation even if native
        # output is absent, malformed, unsuccessful, or cannot be recorded.
        # An outer timeout deliberately leaves confirmed_exit false.
        if confirmed_exit:
            try:
                observe_resources(case, out_dir / "profile")
            except Exception as census_error:
                errors = {
                    "native": ({"type": type(native_error).__name__,
                                "message": str(native_error)}
                               if native_error is not None else None),
                    "census": {"type": type(census_error).__name__,
                               "message": str(census_error)},
                }
                message = f"Independent process census failed: {census_error}"
                if native_error is not None:
                    message = f"{type(native_error).__name__}: {native_error}; {message}"
                try:
                    write_json(case / "exit-failures.json", errors)
                except OSError as receipt_error:
                    message += f"; failure receipt could not be written: {receipt_error}"
                raise RuntimeError(message) from (native_error or census_error)


def run(binary, output, slot_receipt):
    output.mkdir(parents=True, exist_ok=False)
    summary = {
        "schema": "keld.w1-native-fixture-matrix/v1",
        "startedUtc": datetime.now(timezone.utc).isoformat(),
        "machine": os.environ.get("COMPUTERNAME", "unknown"),
        "slotReceipt": slot_receipt, "binary": str(binary),
        "binarySha256": sha256(binary), "cases": [],
        "scope": "native feasibility and synthetic controls; no production admission",
        "productionRouteD": "not implemented or approved",
    }
    write_json(output / "matrix.json", summary)
    try:
        # One changed expected digest; valid source, page and executable stay fixed.
        bad = output / "01-wrong-digest"
        bad_prepared = prepare(bad, "document-created")
        config = json.loads((bad / "config.json").read_text())
        observed = config["bundleSha256"]
        config["bundleSha256"] = ("1" if observed[0] != "1" else "2") + observed[1:]
        result = launch(binary, bad, config, expected_nonzero=True)
        assert_digest_rejection(result, config["bundleSha256"], observed)
        summary["cases"].append({"name": bad.name, "status": "expected-rejection",
                                 "observedBundleSha256": bad_prepared["bundleSha256"]})
        write_json(output / "matrix.json", summary)

        # Failure-first timing control, then healthy recovery and retained settings.
        for name, variant, existing in (
            ("02-late-injection", "dom-content-loaded", None),
            ("03-early-recovery", "document-created", None),
            ("04-preserve-existing", "document-created", "none"),
        ):
            case = output / name
            prepared = prepare(case, variant, existing)
            config = json.loads((case / "config.json").read_text())
            result = launch(binary, case, config)
            evidence = assert_order(result, config, prepared, variant == "document-created")
            summary["cases"].append({"name": name, "status": "expected-observation", **evidence})
            write_json(output / "matrix.json", summary)

        for name, variant in (("05-unfenced-delivery", "unfenced"), ("06-guarded-delivery", "guarded")):
            case = output / name
            case.mkdir()
            first, second = case / "first.html", case / "second.html"
            page = "<!doctype html><meta charset='utf-8'><title>W1_STALE_FIXTURE</title>\n"
            first.write_text(page, encoding="utf-8")
            second.write_text(page, encoding="utf-8")
            config = {
                "mode": "stale-delivery", "variant": variant,
                "url": first.as_uri(), "successorUrl": second.as_uri(),
                "outDir": str(case / "native"), "timeoutMs": 15000, "visible": False,
            }
            result = launch(binary, case, config)
            evidence = assert_stale_delivery(result, config, variant == "unfenced")
            summary["cases"].append({"name": name, "status": "expected-observation", **evidence})
            write_json(output / "matrix.json", summary)

        for index, (variant, busy_ms) in enumerate(
            [("no-force", 50), ("busy", 50), ("busy", 200), ("busy", 1000),
             ("pagehide", 50), ("unload", 50)], start=7
        ):
            case = output / f"{index:02d}-natural-{variant}-{busy_ms}"
            case.mkdir()
            first, second = case / "first.html", case / "second.html"
            page = "<!doctype html><meta charset='utf-8'><title>W1_NATURAL_FIXTURE</title>\n"
            first.write_text(page, encoding="utf-8")
            second.write_text(page, encoding="utf-8")
            config = {
                "mode": "contentloading-race", "variant": variant,
                "url": first.as_uri(), "successorUrl": second.as_uri(),
                "outDir": str(case / "native"), "timeoutMs": 15000, "visible": False,
                "busyMs": busy_ms, "burst": 256, "observeMs": 1500,
            }
            evidence = assert_natural_forcing(launch(binary, case, config), config)
            summary["cases"].append({"name": case.name, "status": "observed-subset", **evidence})
            write_json(output / "matrix.json", summary)

        case = output / "13-final-healthy-recovery"
        prepared = prepare(case, "document-created")
        config = json.loads((case / "config.json").read_text())
        evidence = assert_order(launch(binary, case, config), config, prepared, True)
        summary["cases"].append({"name": case.name, "status": "expected-observation", **evidence})
        summary["status"] = "passed-fixture-matrix"
        summary["finishedUtc"] = datetime.now(timezone.utc).isoformat()
        write_json(output / "matrix.json", summary)
        return summary
    except (AssertionError, OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        summary["status"] = "failed-or-quarantined"
        summary["failure"] = f"{type(error).__name__}: {error}"
        summary["finishedUtc"] = datetime.now(timezone.utc).isoformat()
        write_json(output / "matrix.json", summary)
        raise


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--slot-receipt", required=True)
    args = parser.parse_args()
    if not __debug__:
        parser.error("Assertions must be enabled for these fixture oracles")
    if sys.platform != "win32":
        parser.error("Native acceptance runs on the authorized Windows machine")
    if os.environ.get("COMPUTERNAME", "").casefold() != "ramani":
        parser.error("This execution receipt is restricted to Ramani")
    destination = args.output or Path(os.environ["TEMP"]) / "w1-native-matrix"
    try:
        print(json.dumps(run(args.binary.resolve(), destination.resolve(), args.slot_receipt), indent=2))
    except (AssertionError, OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Native matrix stopped: {error}", file=sys.stderr)
        sys.exit(1)
