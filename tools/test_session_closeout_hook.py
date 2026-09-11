"""Hook admission tests reuse real git/filesystem closeout fixtures."""

from contextlib import redirect_stderr
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import unittest
from unittest import mock

import session_closeout_hook as hook
import test_session_closeout as fixtures


class HookTests(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.CloseoutTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.repo = self.fixture.repo
        self.payload = {"session_id": "session_214", "turn_id": "turn-1",
                        "cwd": str(self.repo), "hook_event_name": "Stop", "stop_hook_active": False}
        self.receipt_path = self.repo / ".git" / "keld-closeout" / "session_214" / "turn-1.json"

    def publish(self, binding="session_214", turn="turn-1"):
        fixture = self.fixture
        self.receipt_path = self.repo / '.git' / 'keld-closeout' / binding / (turn + '.json')
        baseline = json.loads(fixture.baseline.read_text(encoding="utf-8"))
        baseline["session_id"] = binding
        self.receipt_path.parent.mkdir(parents=True, exist_ok=True)
        fixture.baseline = self.receipt_path.parent / 'baseline.json'
        fixture.baseline.write_text(json.dumps(baseline), encoding="utf-8")
        fixture.receipt.update(session_id=binding, turn_id=turn, baseline=fixture.proof(fixture.baseline))
        self.receipt_path.write_text(json.dumps(fixture.receipt), encoding="utf-8")

    def cli(self, payload):
        result = subprocess.run([sys.executable, "-B", str(Path(hook.__file__))], input=payload,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_registered_commands_execute_and_reject_changed_sources(self):
        import hashlib
        import base64
        import re
        import os
        import shutil
        renamed = self.fixture.root / 'repo-\u00e9'
        self.repo.rename(renamed)
        self.repo = renamed
        self.fixture.repo = renamed
        self.payload['cwd'] = str(renamed)
        self.receipt_path = renamed / '.git/keld-closeout/session_214/turn-1.json'
        root = Path(__file__).resolve().parent.parent
        config = json.loads((root / '.codex/hooks.json').read_text(encoding='utf-8'))
        self.assertEqual(config, hook.configuration())
        self.assertEqual(set(config['hooks']), {'UserPromptSubmit', 'Stop'})
        handlers = [config['hooks'][event][0]['hooks'][0] for event in config['hooks']]
        self.assertEqual(handlers[0], handlers[1])
        handler = handlers[0]
        for name in ('session_closeout', 'session_closeout_hook'):
            source = root / 'tools' / (name + '.py')
            self.assertIn(hashlib.sha256(source.read_bytes()).hexdigest(),
                          base64.b64decode(re.search(r"b64decode\('([^']+)'\)", handler['command'])[1]).decode())
        (self.repo / 'tools').mkdir()
        for name in ('session_closeout', 'session_closeout_hook'):
            shutil.copyfile(root / 'tools' / (name + '.py'), self.repo / 'tools' / (name + '.py'))
        command = handler['commandWindows'] if os.name == 'nt' else handler['command']
        shell = ['powershell.exe', '-NoProfile', '-Command'] if os.name == 'nt' else ['sh', '-c']
        def invoke():
            result = subprocess.run(shell + [command], cwd=self.repo,
                                    input=json.dumps(self.payload), capture_output=True, text=True,
                                    timeout=60)
            self.assertEqual(result.returncode, 0, result.stderr)
            return json.loads(result.stdout)
        self.receipt_path.parent.mkdir(parents=True, exist_ok=True)
        (self.receipt_path.parent / 'baseline.json').write_text('{}', encoding='utf-8')
        self.assertEqual(invoke()['decision'], 'block')
        target = self.repo / 'tools/session_closeout.py'
        target.write_text("raise RuntimeError('UNTRUSTED_SOURCE_EXECUTED')", encoding='utf-8')
        result = invoke()
        self.assertIs(result['continue'], False)
        self.assertIn('source changed', result['stopReason'])
        self.assertNotIn('UNTRUSTED_SOURCE_EXECUTED', result['stopReason'])

    def test_main_session_accepts_sibling_task_and_removed_task_but_not_foreign_repo(self):
        fixture = self.fixture
        task = fixture.root / 'task-worktree'
        fixture.git('worktree', 'add', '-b', 'task-branch', str(task))
        self.publish()
        baseline_path = self.receipt_path.parent / 'baseline.json'
        baseline = json.loads(baseline_path.read_text(encoding='utf-8'))
        baseline.update(repo=str(task), git_common_dir=str(self.repo / '.git'),
                        source_ref='refs/heads/task-branch', resources=[{'id': 'task', 'path': str(task)}])
        baseline_path.write_text(json.dumps(baseline), encoding='utf-8')
        receipt = json.loads(self.receipt_path.read_text(encoding='utf-8'))
        receipt.update(repo=str(task), baseline=fixture.proof(baseline_path),
                       resources=[{'id': 'task', 'path': str(task), 'status': 'retained',
                                   'reason': 'Active owned task checkout'}])
        self.receipt_path.write_text(json.dumps(receipt), encoding='utf-8')
        self.assertIn('accepted: complete', hook.response(self.payload)['systemMessage'])
        fixture.git('worktree', 'remove', str(task))
        receipt['resources'][0].update(status='removed', reason='Clean task checkout removed by Git')
        self.receipt_path.write_text(json.dumps(receipt), encoding='utf-8')
        self.assertIn('accepted: complete', hook.response(self.payload)['systemMessage'])
        foreign = fixtures.CloseoutTests()
        foreign.setUp()
        self.addCleanup(foreign.doCleanups)
        baseline.update(repo=str(foreign.repo), git_common_dir=str(foreign.repo / '.git'),
                        source_ref=foreign.git('symbolic-ref', 'HEAD'), resources=[])
        baseline_path.write_text(json.dumps(baseline), encoding='utf-8')
        receipt.update(repo=str(foreign.repo), head=foreign.receipt['head'], resources=[],
                       baseline=fixture.proof(baseline_path))
        self.receipt_path.write_text(json.dumps(receipt), encoding='utf-8')
        self.assertIn('another Git repository', hook.response(self.payload)['reason'])

    def test_inactive_factual_turn_has_no_fabricated_inventory(self):
        result = self.cli(json.dumps(self.payload))
        self.assertNotIn('decision', result)
        self.assertIn('not claimed', result['systemMessage'])
        self.assertFalse(self.receipt_path.parent.exists())

    def test_missing_receipt_blocks_then_stops_with_explicit_handoff(self):
        self.receipt_path.parent.mkdir(parents=True, exist_ok=True)
        (self.receipt_path.parent / 'baseline.json').write_text('{}', encoding='utf-8')
        first = self.cli(json.dumps(self.payload))
        self.assertEqual(first["decision"], "block")
        self.assertIn(str(self.receipt_path), first["reason"])
        self.payload["stop_hook_active"] = True
        second = self.cli(json.dumps(self.payload))
        self.assertIs(second["continue"], False)
        self.assertIn("HANDOFF REQUIRED", second["systemMessage"])

    def test_valid_receipt_permits_stop(self):
        self.publish()
        answer = self.cli(json.dumps(self.payload))
        self.assertNotIn("decision", answer)
        self.assertIn("accepted: complete", answer["systemMessage"])

    def test_wrong_session_turn_and_repository_reject(self):
        self.publish()
        for field, wrong in (("turn_id", "other-turn"),):
            with self.subTest(field=field):
                payload = dict(self.payload, **{field: wrong})
                self.assertEqual(hook.response(payload)["decision"], "block")
        receipt = self.fixture.receipt
        receipt["session_id"] = "wrong/session"
        self.receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        self.assertIn("another session", hook.response(self.payload)["reason"])
        receipt["session_id"] = "session_214"
        receipt["repo"] = str(self.repo.parent)
        self.receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        self.assertEqual(hook.response(self.payload)["decision"], "block")

    def test_subdirectory_and_prompt_point_to_fixed_receipt_without_prompt_storage(self):
        nested = self.repo / "nested"
        nested.mkdir()
        self.payload.update(cwd=str(nested), hook_event_name="UserPromptSubmit", prompt="PRIVATE PROMPT")
        answer = hook.response(self.payload)["hookSpecificOutput"]
        self.assertEqual(answer["hookEventName"], "UserPromptSubmit")
        self.assertIn(str(self.receipt_path), answer["additionalContext"])
        self.assertNotIn("PRIVATE PROMPT", json.dumps(answer))
        self.assertFalse(self.receipt_path.parent.exists())
        self.publish()
        self.payload["hook_event_name"] = "Stop"
        self.assertNotIn("decision", hook.response(self.payload))

    def test_malicious_identifier_and_unsupported_event_reject(self):
        for bad in ("../escape", "a/b", "", "x" * 129, "a\\b"):
            with self.subTest(identifier=bad):
                self.assertEqual(hook.response(dict(self.payload, session_id=bad))["decision"], "block")
        self.assertEqual(hook.response(dict(self.payload, hook_event_name="Other"))["decision"], "block")

    def test_malformed_json_has_visible_failure(self):
        result = self.cli("not JSON")
        self.assertIs(result["continue"], False)
        self.assertIn("HANDOFF REQUIRED", result["systemMessage"])

    def test_claude_native_ids_share_core_and_reject_malformed_id(self):
        payload = {"session_id": "native-session", "prompt_id": "prompt-1", "cwd": str(self.repo),
                   "hook_event_name": "UserPromptSubmit", "prompt": "PRIVATE"}
        answer = hook.claude_response(payload)["hookSpecificOutput"]["additionalContext"]
        self.assertIn("claude-native-session", answer)
        self.assertIn("prompt-1.json", answer)
        self.assertNotIn("PRIVATE", answer)
        payload["hook_event_name"] = "Stop"
        directory = self.repo / ".git/keld-closeout/claude-native-session"
        directory.mkdir(parents=True)
        (directory / "baseline.json").write_text("{}", encoding="utf-8")
        self.assertEqual(hook.claude_response(payload)["decision"], "block")
        payload["prompt_id"] = "../stale"
        self.assertIs(hook.claude_response(payload)["continue"], False)

    def test_cursor_session_prompt_stop_flow_is_metadata_only_and_bounded(self):
        payload = {"conversation_id": "conversation-1", "workspace_roots": [str(self.repo)],
                   "hook_event_name": "sessionStart"}
        with mock.patch("os.getcwd", return_value=str(self.repo)):
            start = hook.cursor_response(payload)
            self.assertIn("current.json", start["additional_context"])
            payload.update(hook_event_name="beforeSubmitPrompt", generation_id="generation-1",
                           prompt="PRIVATE PROMPT", user_message="PRIVATE USER", transcript="PRIVATE TRANSCRIPT")
            self.assertEqual(hook.cursor_response(payload), {"continue": True})
            current = self.repo / ".git/keld-closeout/cursor-conversation-1/current.json"
            stored = current.read_text(encoding="utf-8")
            self.assertNotIn("PRIVATE", stored)
            self.assertFalse((current.parent / "baseline.json").exists())
            payload.update(hook_event_name="stop", loop_count=0, status="completed")
            self.assertEqual(hook.cursor_response(payload), {})
            (current.parent / "baseline.json").write_text("{}", encoding="utf-8")
            self.assertIn("repair", hook.cursor_response(payload)["followup_message"])
            payload["loop_count"] = 1
            with redirect_stderr(io.StringIO()) as stderr:
                self.assertEqual(hook.cursor_response(payload), {})
                self.assertIn('HANDOFF REQUIRED', stderr.getvalue())
            payload.update(loop_count=0, status="aborted")
            self.assertEqual(hook.cursor_response(payload), {})

    def test_cursor_rejects_foreign_or_malformed_workspace_without_codex_shape(self):
        foreign = fixtures.CloseoutTests()
        foreign.setUp()
        self.addCleanup(foreign.doCleanups)
        base = {"conversation_id": "c", "generation_id": "g", "hook_event_name": "beforeSubmitPrompt"}
        with mock.patch("os.getcwd", return_value=str(self.repo)):
            for roots in ([], [str(foreign.repo)], [str(self.repo), str(foreign.repo)]):
                answer = hook.cursor_response(dict(base, workspace_roots=roots))
                self.assertEqual(set(answer), {"continue", "user_message"})
            answer = hook.cursor_response(dict(base, workspace_roots=[str(self.repo)], generation_id="../bad"))
            self.assertEqual(set(answer), {"continue", "user_message"})

    def test_cursor_accepts_windows_posix_drive_workspace_roots(self):
        """Cursor on Windows emits `/d:/path`; that must not fail closed as relative."""
        drive_root = Path(self.repo).resolve()
        posix_drive = "/" + drive_root.as_posix()  # e.g. /D:/WORK/keld
        base = {"conversation_id": "c", "generation_id": "g", "hook_event_name": "beforeSubmitPrompt"}
        with mock.patch("os.getcwd", return_value=str(self.repo)):
            with mock.patch.object(hook.os, "name", "nt"):
                answer = hook.cursor_response(dict(base, workspace_roots=[posix_drive]))
        self.assertEqual(answer, {"continue": True})
        self.assertTrue(
            (self.repo / ".git/keld-closeout/cursor-c/current.json").is_file())

    def test_native_config_shapes_platforms_and_exec_argv(self):
        codex = hook.configuration()
        self.assertEqual(codex, json.loads((Path(__file__).parent.parent / ".codex/hooks.json").read_text(encoding='utf-8')))
        claude = hook.configuration("claude", "windows")
        handler = claude["hooks"]["Stop"][0]["hooks"][0]
        self.assertEqual(handler["command"], "python.exe")
        self.assertEqual(handler["args"][:3], ["-I", "-B", "-c"])
        self.assertNotIn("commandWindows", handler)
        cursor = hook.configuration("cursor", "posix")
        self.assertEqual(cursor["version"], 1)
        self.assertEqual(set(cursor["hooks"]), {"sessionStart", "beforeSubmitPrompt", "stop"})
        self.assertEqual(cursor["hooks"]["stop"][0]["loop_limit"], 1)
        self.assertNotIn("commandWindows", cursor["hooks"]["stop"][0])

    def test_native_valid_receipts_cannot_cross_fresh_turns(self):
        self.publish('claude-native-session', 'prompt-1')
        payload = {'session_id': 'native-session', 'prompt_id': 'prompt-1',
                   'cwd': str(self.repo), 'hook_event_name': 'Stop'}
        self.assertIn('accepted: complete', hook.claude_response(payload)['systemMessage'])
        payload['prompt_id'] = 'prompt-2'
        self.assertEqual(hook.claude_response(payload)['decision'], 'block')
        # Copying an otherwise valid old receipt to the fresh path must also fail.
        self.receipt_path.with_name('prompt-2.json').write_bytes(self.receipt_path.read_bytes())
        self.assertIn('another session or turn', hook.claude_response(payload)['reason'])
        self.publish('cursor-native-session', 'generation-1')
        payload = {'conversation_id': 'native-session', 'generation_id': 'generation-1',
                   'workspace_roots': [str(self.repo)], 'hook_event_name': 'stop',
                   'status': 'completed', 'loop_count': 0}
        previous = os.getcwd()
        try:
            os.chdir(self.repo)
            self.assertEqual(hook.cursor_response(payload), {})
            payload['generation_id'] = 'generation-2'
            self.assertIn('followup_message', hook.cursor_response(payload))
            self.receipt_path.with_name('generation-2.json').write_bytes(self.receipt_path.read_bytes())
            self.assertIn('another session or turn', hook.cursor_response(payload)['followup_message'])
        finally:
            os.chdir(previous)

    def test_cursor_malformed_fields_and_cancelled_turn_do_not_claim_success(self):
        previous = os.getcwd()
        try:
            os.chdir(self.repo)
            base = {'conversation_id': 'c', 'generation_id': 'g',
                    'workspace_roots': [str(self.repo)], 'hook_event_name': 'stop',
                    'status': 'completed', 'loop_count': 0}
            for invalid in (None, True, '1', -1, [], {}):
                answer = hook.cursor_response(dict(base, loop_count=invalid))
                self.assertIn('invalid Cursor loop_count', answer['followup_message'])
            for status in ('aborted', 'error'):
                self.assertEqual(hook.cursor_response({'hook_event_name': 'stop', 'status': status}), {})
            for size in (0, 122, 129):
                answer = hook.cursor_response(dict(base, hook_event_name='beforeSubmitPrompt',
                                                   conversation_id='x' * size))
                self.assertIs(answer['continue'], False)
            self.assertFalse((self.repo / '.git/keld-closeout').exists())
        finally:
            os.chdir(previous)

    def test_generated_native_commands_execute_and_fail_before_untrusted_source(self):
        renamed = self.fixture.root / 'repo space-\u00e9-\u6d4b'
        self.repo.rename(renamed)
        self.repo = renamed
        source_root = Path(__file__).resolve().parent.parent
        (self.repo / 'tools').mkdir()
        checker = self.repo / 'tools/session_closeout.py'
        for harness in ('codex', 'claude', 'cursor'):
            with self.subTest(harness=harness):
                for name in ('session_closeout', 'session_closeout_hook'):
                    shutil.copyfile(source_root / 'tools' / (name + '.py'),
                                    self.repo / 'tools' / (name + '.py'))
                config = hook.configuration(harness)
                if harness == 'claude':
                    handler = config['hooks']['Stop'][0]['hooks'][0]
                    command = [handler['command'], *handler['args']]
                    payload = {'session_id': 'native', 'prompt_id': 'p', 'cwd': str(self.repo),
                               'hook_event_name': 'Stop'}
                elif harness == 'codex':
                    handler = config['hooks']['Stop'][0]['hooks'][0]
                    shell = ['powershell.exe', '-NoProfile', '-NonInteractive', '-Command'] if os.name == 'nt' else ['sh', '-c']
                    command = shell + [handler['commandWindows'] if os.name == 'nt' else handler['command']]
                    payload = {'session_id': 'native', 'turn_id': 'p', 'cwd': str(self.repo),
                               'hook_event_name': 'Stop'}
                else:
                    handler = config['hooks']['stop'][0]
                    shell = ['powershell.exe', '-NoProfile', '-NonInteractive', '-Command'] if os.name == 'nt' else ['sh', '-c']
                    command = shell + [handler['command']]
                    payload = {'conversation_id': 'native', 'generation_id': 'p',
                               'workspace_roots': [str(self.repo)], 'hook_event_name': 'stop',
                               'status': 'completed', 'loop_count': 0}
                def invoke(raw):
                    return subprocess.run(command, cwd=self.repo, input=raw, capture_output=True,
                                          text=True, encoding='utf-8', timeout=60)
                answer = invoke(json.dumps(payload, ensure_ascii=False))
                self.assertEqual(answer.returncode, 0, answer.stderr)
                if harness in ('codex', 'claude'):
                    self.assertIn('No activated Keld task', json.loads(answer.stdout)['systemMessage'])
                else:
                    self.assertEqual(json.loads(answer.stdout), {})
                if harness == 'cursor':
                    malformed = invoke('not JSON')
                    self.assertNotEqual(malformed.returncode, 0)
                    self.assertIn('Invalid Cursor closeout hook input', malformed.stderr)
                checker.write_text("raise RuntimeError('UNTRUSTED_SOURCE_EXECUTED')", encoding='utf-8')
                rejected = invoke(json.dumps(payload))
                self.assertNotIn('UNTRUSTED_SOURCE_EXECUTED', rejected.stdout + rejected.stderr)
                if harness == 'cursor':
                    self.assertNotEqual(rejected.returncode, 0)
                    self.assertIn('source changed', rejected.stderr)
                else:
                    self.assertIs(json.loads(rejected.stdout)['continue'], False)
                    self.assertIn('source changed', json.loads(rejected.stdout)['stopReason'])

    def test_cursor_metadata_directory_symlink_cannot_redirect_write(self):
        outside = self.fixture.root / 'outside'
        outside.mkdir()
        link = self.repo / '.git/keld-closeout'
        try:
            link.symlink_to(outside, target_is_directory=True)
        except OSError as error:
            self.skipTest('OS did not grant directory symlink creation: ' + str(error))
        previous = os.getcwd()
        try:
            os.chdir(self.repo)
            answer = hook.cursor_response({'conversation_id': 'c', 'generation_id': 'g',
                                          'workspace_roots': [str(self.repo)],
                                          'hook_event_name': 'beforeSubmitPrompt'})
            self.assertIs(answer['continue'], False)
            self.assertEqual(list(outside.iterdir()), [])
        finally:
            os.chdir(previous)

    def test_invalid_utf8_is_rejected_before_native_event_dispatch(self):
        for harness in ('codex', 'claude', 'cursor'):
            with self.subTest(harness=harness):
                result = subprocess.run(
                    [sys.executable, '-B', str(Path(hook.__file__)), '--harness', harness],
                    input=b'{"unused":"\xff"}', capture_output=True, timeout=30)
                if harness == 'cursor':
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(b'utf-8', result.stderr)
                else:
                    self.assertEqual(result.returncode, 0)
                    answer = json.loads(result.stdout)
                    self.assertIs(answer['continue'], False)
                    self.assertIn('utf-8', answer['stopReason'])

    def test_native_utf8_bom_is_accepted_without_locale_decoding(self):
        cases = [(harness, count) for harness in ('codex', 'claude', 'cursor') for count in (1, 2)]
        for harness, bom_count in cases:
            with self.subTest(harness=harness, bom_count=bom_count):
                payload = {'hook_event_name': 'stop' if harness == 'cursor' else 'Stop',
                           'session_id': 'native', 'turn_id': 't', 'prompt_id': 't',
                           'conversation_id': 'native', 'generation_id': 't',
                           'cwd': str(self.repo), 'workspace_roots': [str(self.repo)],
                           'status': 'completed', 'loop_count': 0}
                result = subprocess.run(
                    [sys.executable, '-B', str(Path(hook.__file__)), '--harness', harness],
                    cwd=self.repo, input=b'\xef\xbb\xbf' * bom_count + json.dumps(payload).encode('utf-8'),
                    capture_output=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr)
                answer = json.loads(result.stdout)
                if harness == 'cursor':
                    self.assertEqual(answer, {})
                else:
                    self.assertIn('No activated Keld task', answer['systemMessage'])


if __name__ == "__main__":
    unittest.main()
