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
import workspace

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
        managed = workspace.context(root)
        session_closeout.require(managed.common.resolve() == common.resolve(),
                                 "workspace resolver belongs to another Git repository")
        workspace_pointer = managed.root / "sessions" / binding
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
                    "Local workspace pointer: " + str(workspace_pointer) +
                    ". This pointer does not create or activate a session. "
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
    result = subprocess.run([session_closeout.GIT_EXECUTABLE, "-c", "core.fsmonitor=false", "-C", str(session_closeout.absolute(root)), "rev-parse", "--path-format=absolute",
                             "--git-common-dir", "--show-toplevel"], check=True, capture_output=True,
                            text=True, encoding="utf-8", timeout=30,
                            env=session_closeout.git_environment())
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
            session_closeout.require(not stat.S_ISLNK(metadata.st_mode), "Hook metadata path contains a symlink")
            if os.name == "nt":
                session_closeout.require(not metadata.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                                         "Hook metadata path contains a reparse point")
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
    """Request a bounded repair from Cursor's stop cycle; no terminal block."""
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


def git_common_directory(checkout):
    """Read the common Git directory without invoking an untrusted Git binary."""
    marker = checkout / ".git"
    if marker.is_dir():
        return marker.resolve()
    if not marker.is_file():
        return None
    value = marker.read_text(encoding="utf-8").strip()
    session_closeout.require(value.startswith("gitdir: "), "invalid worktree Git metadata")
    admin = (checkout / value[len("gitdir: "):].strip()).resolve()
    common = admin / "commondir"
    return (admin / common.read_text(encoding="utf-8").strip()).resolve() if common.is_file() else admin


def trusted_git_executable(value, checkout=None):
    """Validate the path before any invocation, including filesystem aliases."""
    candidate = session_closeout.absolute(value).resolve(strict=True)
    checkout = checkout or Path(__file__).resolve().parent.parent
    session_closeout.require(not any(parent.samefile(checkout)
                                    for parent in (candidate, *candidate.parents)),
                             "Git executable must be outside the checkout")
    # An unrelated Git-managed installation (e.g. Homebrew) is allowed, but
    # another worktree of this repository is never a trusted installation.
    common = git_common_directory(checkout)
    session_closeout.require(common is not None, "checkout Git metadata is unavailable")
    session_closeout.require(not any((other := git_common_directory(parent)) is not None and
                                    other.samefile(common) for parent in candidate.parents),
                             "Git executable must be outside all worktrees of this repository")
    session_closeout.require(candidate.is_file() and os.access(candidate, os.X_OK),
                             "Git executable must be an executable file")
    # A hard link outside the checkout can still execute checkout-owned bytes.
    session_closeout.require(candidate.stat().st_nlink == 1,
                             "Git executable must have only one hard link")
    return str(candidate)


def codex_trust_path(checkout):
    """Locate a per-checkout enrollment in the user's non-repository config."""
    if os.name == "nt":
        base = Path(os.environ["LOCALAPPDATA"]) / "Keld"
    else:
        import pwd
        base = Path(pwd.getpwuid(os.getuid()).pw_dir) / ".config" / "keld"
    directory = base / "codex-hook-trust"
    key = hashlib.sha256(os.fsencode(str(checkout.resolve()))).hexdigest()
    return directory / (key + ".json")


def configure_codex_local(git_executable, replace=False):
    """Bind a trusted Git path without editing Codex's tracked hook entrypoint."""
    checkout = Path(__file__).resolve().parent.parent
    directory = checkout / ".codex"
    session_closeout.require(directory.is_dir() and not directory.is_symlink(),
                             "Codex configuration directory must be a real directory")
    tracked = directory / "hooks.json"
    session_closeout.require(tracked.is_file() and not tracked.is_symlink(),
                             "tracked .codex/hooks.json is required for hook discovery")
    registered = json.loads(tracked.read_text(encoding="utf-8"))
    expected = configuration()
    session_closeout.require(isinstance(registered, dict) and
                             isinstance(registered.get("hooks"), dict),
                             "Codex hooks registration is malformed")
    for event in ("UserPromptSubmit", "Stop"):
        session_closeout.require(any(item in registered["hooks"].get(event, [])
                                    for item in expected["hooks"][event]),
                                 "Codex " + event + " entrypoint does not match reviewed source hashes")
    selected = trusted_git_executable(git_executable, checkout)
    common = subprocess.check_output([selected, "-c", "core.fsmonitor=false", "-C", str(checkout), "rev-parse",
                                      "--path-format=absolute", "--git-common-dir"],
                                     text=True, encoding="utf-8", timeout=30,
                                     env=session_closeout.git_environment()).strip()
    session_closeout.require(common, "cannot resolve Git common directory")
    data = {"version": 1, "checkout": str(checkout), "git_common_dir": str(Path(common).resolve()),
            "git_executable": selected, "git_sha256": hashlib.sha256(Path(selected).read_bytes()).hexdigest()}
    target = codex_trust_path(checkout)
    session_closeout.require(not any(parent.is_symlink() for parent in
                                    (target, target.parent, target.parent.parent)),
                             "Codex trust path cannot contain a symlink")
    target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    if os.name != "nt":
        metadata = target.parent.stat()
        session_closeout.require(metadata.st_uid == os.getuid() and not metadata.st_mode & 0o077,
                                 "Codex trust directory must be user-owned and private")
    if target.exists():
        metadata = target.lstat()
        safe = (stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1 and
                not getattr(metadata, "st_file_attributes", 0) &
                getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0))
        if os.name != "nt":
            safe = safe and metadata.st_uid == os.getuid() and not metadata.st_mode & 0o077
        session_closeout.require(safe or replace,
                                 "Existing Codex trust manifest is unsafe; review and use --replace-local-binding")
        current = json.loads(target.read_text(encoding="utf-8"))
        session_closeout.require(replace or current == data,
                                 "Codex Git binding already differs; review it and use --replace-local-binding")
        if current == data and safe:
            return target
    write_atomic(target, data)
    if os.name != "nt":
        target.chmod(0o600)
    return target


def configuration(harness="codex", platform=None, git_executable=None):
    """Print project registration for the selected client and OS, without installing.

    Both source modules are pinned before execution. Base64 transports Python through
    native command parsers; it is not a trust mechanism. Never install globally.
    """
    session_closeout.require(harness in {"codex", "claude", "cursor"}, "unsupported harness")
    session_closeout.require(platform in {None, "windows", "posix"}, "unsupported platform")
    selected = platform or ("windows" if os.name == "nt" else "posix")
    if git_executable is not None:
        git_executable = trusted_git_executable(git_executable)
    git_sha256 = (hashlib.sha256(Path(git_executable).read_bytes()).hexdigest()
                  if git_executable is not None else None)
    specs = [(name, hashlib.sha256(Path(__file__).with_name(name + ".py").read_bytes()).hexdigest())
             for name in ("session_closeout", "workspace", "session_closeout_hook")]
    bootstrap = """import hashlib,json,os,pathlib,subprocess,sys,types
def common_git_directory(root):
 marker=root/'.git'
 if marker.is_dir(): return marker.resolve()
 if not marker.is_file(): return None
 value=marker.read_text(encoding='utf-8').strip()
 if not value.startswith('gitdir: '): raise ValueError('Invalid worktree Git metadata')
 admin=(root/value[len('gitdir: '):].strip()).resolve()
 common=admin/'commondir'
 return (admin/common.read_text(encoding='utf-8').strip()).resolve() if common.is_file() else admin
try:
 git=GIT_PATH
 expected_git_sha256=GIT_SHA256
 launch=pathlib.Path.cwd().resolve()
 safe_env={k:v for k,v in os.environ.items() if not k.upper().startswith('GIT_')}
 expected_root=None
 expected_common=None
 if git is None and HARNESS=='codex':
  for location in (launch,*launch.parents):
   if (location/'.codex'/'hooks.json').is_file():
    expected_root=location
    break
  if expected_root is None or (expected_root/'.codex').is_symlink():
   raise ValueError('Keld Codex project hook entrypoint is missing or unsafe')
  if os.name=='nt':
   store=pathlib.Path(os.environ['LOCALAPPDATA'])/'Keld'/'codex-hook-trust'
  else:
   store=pathlib.Path(__import__('pwd').getpwuid(os.getuid()).pw_dir)/'.config'/'keld'/'codex-hook-trust'
  local=store/(hashlib.sha256(os.fsencode(str(expected_root.resolve()))).hexdigest()+'.json')
  if any(parent.is_symlink() for parent in (local,store,store.parent)) or not local.is_file():
   raise ValueError('Keld Codex setup required: --configure-codex --git-executable <trusted absolute Git path>')
  if os.name!='nt':
   mode=store.stat()
   file_mode=local.stat()
   if mode.st_uid!=os.getuid() or mode.st_mode & 0o077 or file_mode.st_uid!=os.getuid() or file_mode.st_mode & 0o077 or file_mode.st_nlink!=1:
    raise ValueError('Keld Codex trust manifest must be private and owned by the current user')
  setting=json.loads(local.read_text(encoding='utf-8'))
  if not isinstance(setting,dict) or set(setting)!={'version','checkout','git_common_dir','git_executable','git_sha256'} or setting['version']!=1 or setting['checkout']!=str(expected_root):
   raise ValueError('Keld Codex trust manifest does not belong to this checkout')
  expected_common=pathlib.Path(setting['git_common_dir'])
  git=setting['git_executable']
  expected_git_sha256=setting['git_sha256']
  if not isinstance(git,str) or not pathlib.Path(git).is_absolute():
   raise ValueError('Keld Codex Git binding must be an absolute path')
 if git is None:
  raise ValueError('Generate local registration with --git-executable and a trusted absolute Git path outside the checkout')
 candidate=pathlib.Path(git).resolve(strict=True)
 if expected_root is not None and any(parent.samefile(expected_root) for parent in (candidate,*candidate.parents)):
  raise ValueError('Keld Codex Git executable must be outside the checkout')
 selected_common=expected_common
 if selected_common is None:
  for parent in (launch,*launch.parents):
   selected_common=common_git_directory(parent)
   if selected_common is not None: break
 if selected_common is not None and any((other:=common_git_directory(parent)) is not None and other.samefile(selected_common) for parent in candidate.parents):
  raise ValueError('Keld Git executable must be outside all worktrees of this repository')
 if not candidate.is_file() or not os.access(candidate,os.X_OK):
  raise ValueError('Keld Git executable must be an executable file')
 if candidate.stat().st_nlink!=1:
  raise ValueError('Keld Git executable must have only one hard link')
 if not isinstance(expected_git_sha256,str) or hashlib.sha256(candidate.read_bytes()).hexdigest()!=expected_git_sha256:
  raise ValueError('Keld trusted Git binary changed; review and reconfigure')
 git=str(candidate)
 root=pathlib.Path(subprocess.check_output([git,'-c','core.fsmonitor=false','-C',str(launch),'rev-parse','--show-toplevel'],text=True,encoding='utf-8',timeout=30,env=safe_env).strip()).resolve()
 if expected_root is not None and not root.samefile(expected_root):
  raise ValueError('Keld Codex hook registration belongs to another checkout')
 if expected_common is not None:
  actual=pathlib.Path(subprocess.check_output([git,'-c','core.fsmonitor=false','-C',str(root),'rev-parse','--path-format=absolute','--git-common-dir'],text=True,encoding='utf-8',timeout=30,env=safe_env).strip()).resolve()
  if not actual.samefile(expected_common):
   raise ValueError('Keld Codex Git common directory changed; review and reconfigure')
 specs=SPECS
 blobs=[(name,(root/'tools'/(name+'.py')).read_bytes(),digest) for name,digest in specs]
 if any(hashlib.sha256(blob).hexdigest()!=digest for name,blob,digest in blobs):
  raise ValueError('Closeout hook source changed; review and refresh trusted registration')
 for name,blob,digest in blobs:
  module=types.ModuleType(name)
  module.__file__=str(root/'tools'/(name+'.py'))
  sys.modules[name]=module
  exec(compile(blob,module.__file__,'exec'),module.__dict__)
  if name=='session_closeout':
   module.GIT_EXECUTABLE=git
 sys.argv=['session_closeout_hook.py','--harness',HARNESS]
 sys.exit(sys.modules['session_closeout_hook'].main())
except Exception as error:
 failure={'continue':False,'stopReason':str(error),'systemMessage':'HANDOFF REQUIRED: closeout hook could not validate evidence.'}
 if HARNESS=='cursor':
  print('HANDOFF REQUIRED: repair trusted closeout registration. '+str(error),file=sys.stderr)
  sys.exit(1)
 if HARNESS=='codex':
  try:
   event=json.loads(sys.stdin.read()).get('hook_event_name')
  except Exception:
   event=None
  if event=='UserPromptSubmit':
   print(json.dumps({'decision':'block','reason':str(error),'systemMessage':failure['systemMessage']}))
   sys.exit(0)
 print(json.dumps(failure))
""".replace("SPECS", repr(specs)).replace("HARNESS", repr(harness)).replace("GIT_PATH", repr(git_executable)).replace("GIT_SHA256", repr(git_sha256))
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
    commands = parser.add_mutually_exclusive_group()
    commands.add_argument("--print-config", action="store_true")
    commands.add_argument("--configure-codex", action="store_true",
                          help="Write a protected per-user trust manifest; keep tracked hooks unchanged")
    parser.add_argument("--git-executable", help="Trusted absolute Git path outside this checkout; omitted emits a disabled template")
    parser.add_argument("--replace-local-binding", action="store_true",
                        help="Explicitly replace an existing Codex Git path after review")
    parser.add_argument("--harness", choices=("codex", "claude", "cursor"), default="codex")
    parser.add_argument("--platform", choices=("windows", "posix"))
    args = parser.parse_args()
    if args.configure_codex:
        if not args.git_executable:
            parser.error("--configure-codex requires --git-executable")
        location = configure_codex_local(args.git_executable, args.replace_local_binding)
        print("Codex trusted Git binding installed at " + str(location) +
              "; review and trust tracked .codex/hooks.json via Codex /hooks.")
        return 0
    if args.replace_local_binding:
        parser.error("--replace-local-binding requires --configure-codex")
    if args.print_config:
        print(json.dumps(configuration(args.harness, args.platform, args.git_executable), indent=2))
        return 0
    try:
        # Cursor's Windows CLI can prefix two UTF-8 BOMs. Strip only leading
        # transport markers, never content inside the JSON or use the pipe locale.
        payload = json.loads(sys.stdin.buffer.read().decode("utf-8").lstrip("\ufeff"),
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
