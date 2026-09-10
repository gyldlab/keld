"""Hash-bound native-client adapters for Keld closeout evidence."""

import argparse
import base64
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

sys.dont_write_bytecode = True
import session_closeout

ID = r"[A-Za-z0-9_-]{1,128}"



def response(payload):
    """Return a hook response; invalid admission never silently permits completion."""
    repeated = isinstance(payload, dict) and payload.get("stop_hook_active") is True
    receipt_path = None
    try:
        session_closeout.require(isinstance(payload, dict), "hook input must be an object")
        event = payload.get("hook_event_name")
        session_closeout.require(event in {"UserPromptSubmit", "Stop"}, "unsupported hook event")
        ids = [identifier(payload.get(key), key) for key in ("session_id", "turn_id")]
        common, root = git_context(payload.get("cwd"))
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
                    "Use the task checkout as repo, in the same Git repository as " + str(root) +
                    ". Before cleanup, retain baseline git_common_dir and source_ref (refs/heads/branch). "
                    "This hook verifies local evidence only.")}}
        if not os.path.lexists(baseline_path):
            return {"systemMessage": "No activated Keld task; closeout enforcement not claimed."}
        receipt = session_closeout.read_json(receipt_path)
        session_closeout.require(isinstance(receipt, dict) and receipt.get("session_id") == binding and receipt.get("turn_id") == ids[1],
                                "receipt belongs to another session or turn")
        baseline_ref = receipt.get("baseline", {})
        session_closeout.require(isinstance(baseline_ref, dict) and
                                session_closeout.absolute(baseline_ref.get("path")).resolve() == baseline_path.resolve(),
                                "receipt does not consume the activated baseline")
        baseline = session_closeout.read_json(session_closeout.evidence(baseline_ref))
        task_common, _ = session_closeout.repository_context(receipt, baseline)
        session_closeout.require(task_common.resolve() == common.resolve(),
                                "receipt belongs to another Git repository")
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



def identifier(value, name):
    session_closeout.require(isinstance(value, str) and re.fullmatch(ID, value), "invalid " + name)
    return value


def namespaced(prefix, value, name):
    result = prefix + identifier(value, name)
    session_closeout.require(len(result) <= 128, "invalid " + name)
    return result


def claude_response(payload):
    try:
        session_closeout.require(isinstance(payload, dict), "hook input must be an object")
        session_closeout.require(payload.get("hook_event_name") in {"UserPromptSubmit", "Stop"},
                                 "unsupported Claude hook event")
        return response(dict(payload, session_id=namespaced("claude-", payload.get("session_id"), "session_id"),
                             turn_id=identifier(payload.get("prompt_id"), "prompt_id")))
    except (ValueError, OSError, subprocess.SubprocessError, TypeError, KeyError) as error:
        return {"continue": False, "stopReason": "Invalid Claude closeout hook input: " + str(error),
                "systemMessage": "HANDOFF REQUIRED; session completion is not established."}


def git_context(root):
    result = subprocess.run(["git", "-C", str(session_closeout.absolute(root)), "rev-parse", "--path-format=absolute",
                             "--git-common-dir", "--show-toplevel"], check=True, capture_output=True,
                            text=True, encoding="utf-8", timeout=30)
    lines = result.stdout.splitlines()
    session_closeout.require(len(lines) == 2, "cannot identify checkout")
    return tuple(Path(line).resolve() for line in lines)


def cursor_context(payload):
    roots = payload.get("workspace_roots")
    session_closeout.require(isinstance(roots, list) and roots, "workspace_roots must be a non-empty array")
    launch = git_context(os.getcwd())
    contexts = {git_context(root) for root in roots}
    session_closeout.require(len(contexts) == 1 and next(iter(contexts))[0] == launch[0],
                             "Cursor workspace_roots do not identify the launch Git repository unambiguously")
    return next(iter(contexts))


def write_atomic(path, value):
    probe = path.parent
    while probe != probe.parent:
        if os.path.lexists(probe):
            metadata = probe.lstat()
            session_closeout.require(not stat.S_ISLNK(metadata.st_mode), "Cursor metadata path contains a symlink")
            if os.name == "nt":
                session_closeout.require(not metadata.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                                         "Cursor metadata path contains a reparse point")
        probe = probe.parent
    path.parent.mkdir(parents=True, exist_ok=True)
    handle, temporary = tempfile.mkstemp(prefix="current-", suffix=".json", dir=path.parent)
    try:
        with os.fdopen(handle, "w", encoding="utf-8") as stream:
            json.dump(value, stream)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    except Exception:
        try:
            os.unlink(temporary)
        except OSError as cleanup_error:
            print("Cursor metadata temporary cleanup failed: " + str(cleanup_error), file=sys.stderr)
        raise


def cursor_response(payload):
    """Request at most one repair per conversation; Cursor has no terminal block."""
    try:
        session_closeout.require(isinstance(payload, dict), "hook input must be an object")
        event = payload.get("hook_event_name")
        session_closeout.require(event in {"sessionStart", "beforeSubmitPrompt", "stop"},
                                 "unsupported Cursor hook event")
        if event == "stop" and payload.get("status") in {"aborted", "error"}:
            return {}
        session_id = namespaced("cursor-", payload.get("conversation_id"), "conversation_id")
        common, root = cursor_context(payload)
        directory = common / "keld-closeout" / session_id
        current = directory / "current.json"
        if event == "sessionStart":
            return {"continue": True, "additional_context": "Before work and closeout, read " + str(current) +
                    ". beforeSubmitPrompt refreshes that fixed metadata file; it does not activate a baseline."}
        generation = identifier(payload.get("generation_id"), "generation_id")
        if event == "beforeSubmitPrompt":
            prompt = response({"session_id": session_id, "turn_id": generation, "cwd": str(root),
                               "hook_event_name": "UserPromptSubmit", "stop_hook_active": False})
            instruction = prompt["hookSpecificOutput"]["additionalContext"]
            write_atomic(current, {"session_id": session_id, "turn_id": generation, "repo": str(root),
                                   "receipt_path": str(directory / (generation + ".json")),
                                   "instruction": instruction})
            return {"continue": True}
        status = payload.get("status")
        session_closeout.require(status in {"completed", "aborted", "error"}, "invalid Cursor stop status")
        loop_count = payload.get("loop_count")
        session_closeout.require(type(loop_count) is int and loop_count >= 0, "invalid Cursor loop_count")
        result = response({"session_id": session_id, "turn_id": generation, "cwd": str(root),
                           "hook_event_name": "Stop", "stop_hook_active": False})
        if "decision" not in result:
            print(result.get("systemMessage", ""), file=sys.stderr)
            return {}
        if loop_count >= 1:
            print("HANDOFF REQUIRED; session completion is not established. " + result["reason"], file=sys.stderr)
            return {}
        return {"followup_message": "Read " + str(current) + "; repair its closeout receipt. " + result["reason"]}
    except (ValueError, OSError, subprocess.SubprocessError, TypeError, KeyError) as error:
        event = payload.get("hook_event_name") if isinstance(payload, dict) else None
        loop_count = payload.get("loop_count") if isinstance(payload, dict) else None
        if event == "stop" and type(loop_count) is int and loop_count >= 1:
            print("HANDOFF REQUIRED; " + str(error), file=sys.stderr)
            return {}
        if event == "stop":
            return {"followup_message": "Closeout hook failed: " + str(error)}
        if event == "sessionStart":
            return {"additional_context": "HANDOFF REQUIRED: " + str(error)}
        return {"continue": False, "user_message": "Closeout hook failed: " + str(error)}


def configuration(harness="codex", platform=None):
    """Print project registration for the selected client and OS, without installing.

    Both source modules are pinned before execution. Base64 transports Python through
    native command parsers; it is not a trust mechanism. Never install globally.
    """
    session_closeout.require(harness in {"codex", "claude", "cursor"}, "unsupported harness")
    session_closeout.require(platform in {None, "windows", "posix"}, "unsupported platform")
    selected = platform or ("windows" if os.name == "nt" else "posix")
    specs = [(name, hashlib.sha256(Path(__file__).with_name(name + ".py").read_bytes()).hexdigest())
             for name in ("session_closeout", "session_closeout_hook")]
    bootstrap = """import hashlib,json,pathlib,subprocess,sys,types
try:
 root=pathlib.Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],text=True,encoding='utf-8').strip())
 specs=SPECS
 blobs=[(name,(root/'tools'/(name+'.py')).read_bytes(),digest) for name,digest in specs]
 if any(hashlib.sha256(blob).hexdigest()!=digest for name,blob,digest in blobs):
  raise ValueError('Closeout hook source changed; review and refresh trusted registration')
 for name,blob,digest in blobs:
  module=types.ModuleType(name)
  module.__file__=str(root/'tools'/(name+'.py'))
  sys.modules[name]=module
  exec(compile(blob,module.__file__,'exec'),module.__dict__)
 sys.argv=['session_closeout_hook.py','--harness',HARNESS]
 sys.exit(sys.modules['session_closeout_hook'].main())
except Exception as error:
 failure={'continue':False,'stopReason':str(error),'systemMessage':'HANDOFF REQUIRED: closeout hook could not validate evidence.'}
 if HARNESS=='cursor':
  print('HANDOFF REQUIRED: repair trusted closeout registration. '+str(error),file=sys.stderr)
  sys.exit(1)
 print(json.dumps(failure))
""".replace("SPECS", repr(specs)).replace("HARNESS", repr(harness))
    code = "import base64;exec(base64.b64decode('" + base64.b64encode(bootstrap.encode()).decode() + "'))"
    posix, windows = "python3 -I -B -c \"" + code + "\"", "python -I -B -c \"" + code + "\""
    if harness == "claude":
        handler = {"type": "command", "command": "python.exe" if selected == "windows" else "python3",
                   "args": ["-I", "-B", "-c", code], "timeout": 60}
        return {"hooks": {event: [{"hooks": [handler]}] for event in ("UserPromptSubmit", "Stop")}}
    if harness == "cursor":
        handler = {"command": windows if selected == "windows" else posix, "timeout": 60, "failClosed": True}
        hooks = {event: [dict(handler)] for event in ("sessionStart", "beforeSubmitPrompt", "stop")}
        hooks["stop"][0]["loop_limit"] = 1
        return {"version": 1, "hooks": hooks}
    handler = {"type": "command", "command": posix, "commandWindows": windows, "timeout": 60,
               "statusMessage": "Checking session closeout evidence"}
    return {"description": "Keld session evidence gate; hash-bound handlers. Review via /hooks before use.",
            "hooks": {event: [{"hooks": [handler]}] for event in ("UserPromptSubmit", "Stop")}}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--print-config", action="store_true")
    parser.add_argument("--harness", choices=("codex", "claude", "cursor"), default="codex")
    parser.add_argument("--platform", choices=("windows", "posix"))
    args = parser.parse_args()
    if args.print_config:
        print(json.dumps(configuration(args.harness, args.platform), indent=2))
        return 0
    try:
        # Native clients send UTF-8 JSON; Windows pipe encoding may be cp1252.
        payload = json.loads(sys.stdin.buffer.read().decode("utf-8"),
                             object_pairs_hook=session_closeout.unique_object)
    except (ValueError, OSError) as error:
        failure = {"continue": False, "stopReason": "Invalid closeout hook input: " + str(error),
                   "systemMessage": "HANDOFF REQUIRED; session completion is not established."}
        if args.harness == "cursor":
            print("Invalid Cursor closeout hook input: " + str(error), file=sys.stderr)
            return 1
        print(json.dumps(failure))
        return 0
    handler = {"codex": response, "claude": claude_response, "cursor": cursor_response}[args.harness]
    print(json.dumps(handler(payload)))
    return 0

if __name__ == "__main__":
    sys.exit(main())
