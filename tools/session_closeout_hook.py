"""Read-only root-turn hook adapter for the local closeout evidence validator."""

import json
import base64
import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys

sys.dont_write_bytecode = True
import session_closeout


def response(payload):
    """Return a hook response; invalid admission never silently permits completion."""
    repeated = isinstance(payload, dict) and payload.get("stop_hook_active") is True
    receipt_path = None
    try:
        session_closeout.require(isinstance(payload, dict), "hook input must be an object")
        event = payload.get("hook_event_name")
        session_closeout.require(event in {"UserPromptSubmit", "Stop"}, "unsupported hook event")
        ids = []
        for key in ("session_id", "turn_id"):
            value = payload.get(key)
            session_closeout.require(isinstance(value, str) and
                                    re.fullmatch(r"[A-Za-z0-9_-]{1,128}", value),
                                    "invalid " + key)
            ids.append(value)
        cwd = session_closeout.absolute(payload.get("cwd"))
        result = subprocess.run(
            ["git", "-C", str(cwd), "rev-parse", "--path-format=absolute",
             "--git-common-dir", "--show-toplevel"],
            check=True, capture_output=True, text=True, timeout=30)
        lines = result.stdout.splitlines()
        session_closeout.require(len(lines) == 2, "cannot identify checkout")
        common, root = (Path(line) for line in lines)
        receipt_path = common / "keld-closeout" / ids[0] / (ids[1] + ".json")
        baseline_path = common / "keld-closeout" / ids[0] / "baseline.json"
        binding = ids[0]
        if event == "UserPromptSubmit":
            return {"hookSpecificOutput": {
                "hookEventName": event,
                "additionalContext": (
                    "Follow docs/agents/workflow.md session closeout requirements. "
                    "For non-trivial work, activate this task by saving its independently reconciled "
                    "keld.session-baseline/v1 inventory at " + str(baseline_path) + ". "
                    "Do not activate for a standalone factual question. While activated, save this turn's "
                    "keld.session-closeout/v1 receipt at "
                    + str(receipt_path) + "; session_id must be " + binding + "; turn_id must be " + ids[1] + ". "
                    "Receipt repo must be " + str(root) + ". This hook verifies local evidence only.")}}
        if not os.path.lexists(baseline_path):
            return {"systemMessage": "No activated Keld task; closeout enforcement not claimed."}
        receipt = session_closeout.read_json(receipt_path)
        session_closeout.require(isinstance(receipt, dict) and receipt.get("session_id") == binding and receipt.get("turn_id") == ids[1],
                                "receipt belongs to another session or turn")
        session_closeout.require(session_closeout.absolute(receipt.get("repo")).resolve() == root.resolve(),
                                "receipt belongs to another checkout")
        baseline_ref = receipt.get("baseline", {})
        session_closeout.require(isinstance(baseline_ref, dict) and
                                session_closeout.absolute(baseline_ref.get("path")).resolve() == baseline_path.resolve(),
                                "receipt does not consume the activated baseline")
        outcome = session_closeout.check(receipt_path)
        return {"systemMessage": "Local closeout evidence accepted: " + outcome +
                ". Remote state and inventory completeness are not authenticated."}
    except (ValueError, OSError, subprocess.SubprocessError, TypeError, KeyError) as error:
        location = str(receipt_path) if receipt_path else "the current turn's fixed closeout receipt"
        reason = ("Closeout admission failed: " + str(error) + ". Repair " + location +
                  " using docs/agents/workflow.md; record unresolved work as an explicit handoff.")
        if repeated:
            return {"continue": False, "stopReason": reason,
                    "systemMessage": "HANDOFF REQUIRED; session completion is not established. " + reason}
        return {"decision": "block", "reason": reason}


def configuration():
    """Generate the reviewed registration; hashes prevent executing changed repo code.

    Refresh with: python -B tools/session_closeout_hook.py --print-config
    Base64 only transports multiline Python through both native shells; it is not trust.
    """
    specs = [(name, hashlib.sha256(Path(__file__).with_name(name + '.py').read_bytes()).hexdigest())
             for name in ('session_closeout', 'session_closeout_hook')]
    bootstrap = """import hashlib,json,pathlib,subprocess,sys,types
try:
 root=pathlib.Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],text=True).strip())
 specs=SPECS
 blobs=[(name,(root/'tools'/(name+'.py')).read_bytes(),digest) for name,digest in specs]
 if any(hashlib.sha256(blob).hexdigest()!=digest for name,blob,digest in blobs):
  raise ValueError('Closeout hook source changed; review and refresh trusted registration')
 for name,blob,digest in blobs:
  module=types.ModuleType(name)
  module.__file__=str(root/'tools'/(name+'.py'))
  sys.modules[name]=module
  exec(compile(blob,module.__file__,'exec'),module.__dict__)
 sys.modules['session_closeout_hook'].main()
except Exception as error:
 print(json.dumps({'continue':False,'stopReason':str(error),'systemMessage':'HANDOFF REQUIRED: closeout hook could not validate evidence.'}))
""".replace('SPECS', repr(specs))
    encoded = base64.b64encode(bootstrap.encode()).decode()
    code = "import base64;exec(base64.b64decode('" + encoded + "'))"
    handler = {'type': 'command', 'command': 'python3 -I -B -c "' + code + '"',
               'commandWindows': 'python -I -B -c "' + code + '"', 'timeout': 60,
               'statusMessage': 'Checking session closeout evidence'}
    return {'description': 'Keld session evidence gate; read-only, hash-bound handlers. Review via /hooks before use.',
            'hooks': {event: [{'hooks': [handler]}] for event in ('UserPromptSubmit', 'Stop')}}


def main():
    if sys.argv[1:] == ["--print-config"]:
        print(json.dumps(configuration(), indent=2))
        return 0
    try:
        payload = json.load(sys.stdin, object_pairs_hook=session_closeout.unique_object)
    except (ValueError, OSError) as error:
        print(json.dumps({"continue": False, "stopReason": "Invalid closeout hook input: " + str(error),
                          "systemMessage": "HANDOFF REQUIRED; session completion is not established."}))
        return 0
    print(json.dumps(response(payload)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
