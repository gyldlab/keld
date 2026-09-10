"""Hook admission tests reuse real git/filesystem closeout fixtures."""

import json
from pathlib import Path
import subprocess
import sys
import unittest

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

    def publish(self):
        fixture = self.fixture
        binding = "session_214"
        baseline = json.loads(fixture.baseline.read_text(encoding="utf-8"))
        baseline["session_id"] = binding
        self.receipt_path.parent.mkdir(parents=True, exist_ok=True)
        fixture.baseline = self.receipt_path.parent / 'baseline.json'
        fixture.baseline.write_text(json.dumps(baseline), encoding="utf-8")
        fixture.receipt.update(session_id=binding, turn_id='turn-1', baseline=fixture.proof(fixture.baseline))
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
        self.assertIn("another checkout", hook.response(self.payload)["reason"])

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


if __name__ == "__main__":
    unittest.main()
