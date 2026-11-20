"""Hook admission tests reuse real git/filesystem closeout fixtures."""

from contextlib import redirect_stderr
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import session_closeout_hook as hook
import test_session_closeout as fixtures


def trusted_git_for_tests():
    """Use a one-link Git executable; do not weaken the product trust gate.

    Git for Windows often hard-links cmd/git.exe to git-lfs.exe. Its installed
    bin/git.exe launcher is a distinct one-link executable in that same Git
    installation. An unsuitable installation must still fail the real guard.
    """
    found = shutil.which('git')
    if found is None:
        raise RuntimeError('Git is required for native closeout tests')
    candidate = Path(found).resolve()
    if os.name == 'nt' and candidate.stat().st_nlink != 1:
        alternative = candidate.parent.parent / 'bin' / 'git.exe'
        if alternative.is_file() and alternative.stat().st_nlink == 1:
            candidate = alternative.resolve()
    return str(candidate)


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

    def fixture_git_configuration(self, executable):
        """Exercise production registration using only this disposable Git checkout."""
        original_guard = hook.trusted_git_executable
        with mock.patch.object(hook, 'trusted_git_executable',
                               side_effect=lambda value: original_guard(value, checkout=self.repo)):
            return hook.configuration(git_executable=executable)

    def test_registration_rejects_checkout_git(self):
        local_git = self.repo / 'fake-git'
        local_git.write_bytes(b'not an executable')
        self.addCleanup(local_git.unlink)
        with self.assertRaisesRegex(ValueError, 'outside the checkout'):
            self.fixture_git_configuration(str(local_git.resolve()))
        with self.assertRaisesRegex(ValueError, 'absolute'):
            hook.configuration(git_executable='git')

    def test_registration_negative_probe_preserves_existing_source_file(self):
        """A developer's same-name file in another source tree stays intact."""
        with tempfile.TemporaryDirectory(prefix='keld-protected-source-', dir=self.fixture.root) as temp:
            source_tools = Path(temp) / 'tools'
            source_tools.mkdir()
            sentinel = source_tools / 'fake-git'
            sentinel.write_bytes(b'UNTRACKED_DEVELOPER_FILE_MUST_SURVIVE')
            with mock.patch.object(hook, '__file__', str(source_tools / 'session_closeout_hook.py')):
                self.test_registration_rejects_checkout_git()
            self.assertEqual(sentinel.read_bytes(), b'UNTRACKED_DEVELOPER_FILE_MUST_SURVIVE')

    def test_registration_rejects_case_alias_of_checkout_git(self):
        """Real filesystem identity must trump differing component spellings."""
        local_git = self.repo / 'case-alias-git'
        local_git.write_text('#!/bin/sh\nexit 0\n', encoding='utf-8')
        local_git.chmod(0o755)
        self.addCleanup(local_git.unlink)
        checkout = self.repo
        alias = checkout.with_name(checkout.name.swapcase()) / local_git.name
        if not alias.exists() or not alias.samefile(local_git):
            self.skipTest('filesystem does not support the case-alias attack')
        with self.assertRaisesRegex(ValueError, 'outside the checkout'):
            self.fixture_git_configuration(str(alias))

    def test_registration_rejects_outside_hardlink_to_checkout_executable(self):
        """An outside pathname cannot make a checkout inode trusted."""
        checkout = self.repo
        local_git = checkout / (
            'hardlink-git.exe' if os.name == 'nt' else 'hardlink-git'
        )
        if os.name == 'nt':
            shutil.copyfile(sys.executable, local_git)
        else:
            local_git.write_text('#!/bin/sh\nexit 0\n', encoding='utf-8')
            local_git.chmod(0o755)
        self.addCleanup(local_git.unlink)
        with tempfile.TemporaryDirectory(prefix='keld-hardlink-', dir=checkout.parent) as temp:
            alias = Path(temp) / local_git.name
            try:
                os.link(local_git, alias)
            except OSError as error:
                self.skipTest(f'filesystem lacks hard-link support: {error}')
            self.assertTrue(alias.samefile(local_git))
            self.assertGreater(alias.stat().st_nlink, 1)
            with self.assertRaisesRegex(ValueError, 'only one hard link'):
                self.fixture_git_configuration(str(alias))

    def test_registration_rejects_sibling_worktree_executable(self):
        """A different worktree is not a trusted Git installation."""
        local_git = self.repo / ('sibling-git.exe' if os.name == 'nt' else 'sibling-git')
        if os.name == 'nt':
            shutil.copyfile(sys.executable, local_git)
        else:
            local_git.write_text('#!/bin/sh\nexit 0\n', encoding='utf-8')
            local_git.chmod(0o755)
        linked = self.fixture.root / 'linked-sibling'
        self.fixture.git('worktree', 'add', '-b', 'sibling-trust', str(linked))
        self.addCleanup(lambda: self.fixture.git('worktree', 'remove', '--force', str(linked)))
        guard = hook.trusted_git_executable
        with mock.patch.object(hook, 'trusted_git_executable',
                               side_effect=lambda value: guard(value, checkout=linked)):
            with self.assertRaisesRegex(ValueError, 'outside all worktrees of this repository'):
                hook.configuration(git_executable=str(local_git))

    @unittest.skipUnless(os.name == 'nt', 'Git for Windows launcher acceptance')
    def test_windows_installed_git_launcher_preserves_hardlink_rejection(self):
        selected = Path(trusted_git_for_tests())
        self.assertEqual(selected.stat().st_nlink, 1)
        version = subprocess.run([str(selected), '--version'], check=True,
                                 capture_output=True, text=True, timeout=30)
        self.assertIn('git version', version.stdout)
        ambient = Path(shutil.which('git')).resolve()
        if ambient.stat().st_nlink != 1:
            with self.assertRaisesRegex(ValueError, 'only one hard link'):
                hook.configuration(git_executable=str(ambient))
        self.assertIn('UserPromptSubmit', hook.configuration(git_executable=str(selected))['hooks'])

    def test_unconfigured_registration_fails_closed(self):
        handler = hook.configuration('claude')['hooks']['Stop'][0]['hooks'][0]
        result = subprocess.run([sys.executable, *handler['args']], cwd=self.repo,
                                input='{}', capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        answer = json.loads(result.stdout)
        self.assertIs(answer['continue'], False)
        self.assertIn('--git-executable', answer['stopReason'])

    @unittest.skipIf(os.name == 'nt', 'POSIX executable replacement probe')
    def test_explicit_registration_rejects_replaced_git_before_execution(self):
        git = self.fixture.root / 'reviewed-git'
        marker = self.fixture.root / 'unreviewed-git-ran'
        git.write_text('#!/bin/sh\nexit 0\n', encoding='utf-8')
        git.chmod(0o755)
        handler = hook.configuration('claude', git_executable=str(git))['hooks']['Stop'][0]['hooks'][0]
        git.write_text(f'#!/bin/sh\nprintf executed > "{marker}"\nexit 0\n', encoding='utf-8')
        result = subprocess.run([sys.executable, *handler['args']], cwd=self.repo,
                                input='{}', capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('trusted Git binary changed', json.loads(result.stdout)['stopReason'])
        self.assertFalse(marker.exists(), 'a replaced Git executable was invoked')

    def test_registered_git_survives_hostile_path_for_all_helpers(self):
        source_root = Path(hook.__file__).resolve().parent.parent
        (self.repo / 'tools').mkdir()
        for name in ('session_closeout', 'workspace', 'session_closeout_hook'):
            shutil.copyfile(source_root / 'tools' / (name + '.py'),
                            self.repo / 'tools' / (name + '.py'))
        trusted_git = trusted_git_for_tests()
        config = hook.configuration('claude', git_executable=trusted_git)
        handler = config['hooks']['Stop'][0]['hooks'][0]
        # A real child process must validate an activated receipt, exercising
        # bootstrap, adapter, workspace resolver and closeout validator Git calls.
        self.publish('claude-native', 'p')
        hostile = self.repo / 'git'
        hostile.write_text('#!/bin/sh\necho SUBSTITUTED_GIT >&2\nexit 73\n')
        hostile.chmod(0o755)
        if os.name == 'nt':
            # A native executable is necessary to exercise Windows CreateProcess.
            shutil.copyfile(sys.executable, self.repo / 'git.exe')
        (self.repo / '.git/info/exclude').write_text('tools/\ngit\ngit.exe\n')
        payload = dict(self.payload, session_id='native', prompt_id='p')
        result = subprocess.run([sys.executable, *handler['args']], cwd=self.repo,
                                env=dict(os.environ, PATH=str(self.repo)),
                                input=json.dumps(payload), capture_output=True,
                                text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Local closeout evidence accepted', json.loads(result.stdout).get('systemMessage', result.stdout))
        self.assertNotIn('SUBSTITUTED_GIT', result.stdout + result.stderr)

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
        config = hook.configuration(git_executable=trusted_git_for_tests())
        self.assertEqual(set(config['hooks']), {'UserPromptSubmit', 'Stop'})
        handlers = [config['hooks'][event][0]['hooks'][0] for event in config['hooks']]
        self.assertEqual(handlers[0], handlers[1])
        handler = handlers[0]
        for name in ('session_closeout', 'workspace', 'session_closeout_hook'):
            source = root / 'tools' / (name + '.py')
            self.assertIn(hashlib.sha256(source.read_bytes()).hexdigest(),
                          base64.b64decode(re.search(r"b64decode\('([^']+)'\)", handler['command'])[1]).decode())
        (self.repo / 'tools').mkdir()
        for name in ('session_closeout', 'workspace', 'session_closeout_hook'):
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

    def test_local_codex_registration_keeps_completed_receipt_clean(self):
        source_root = Path(__file__).resolve().parent.parent
        # Mirror a fresh clone: hooks are tracked, only the Git binding is local.
        entrypoint = Path('.codex/hooks.json')
        files = [Path('.gitignore'), Path('.gitattributes'), entrypoint,
                 *(Path('tools') / (name + '.py')
                 for name in ('session_closeout', 'workspace', 'session_closeout_hook'))]
        for path in files:
            (self.repo / path).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source_root / path, self.repo / path)
        self.fixture.git('add', '--', *(str(path) for path in files))
        self.fixture.git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                         'commit', '-qm', 'track closeout sources')
        head = self.fixture.git('rev-parse', 'HEAD')
        self.fixture.receipt['head'] = head
        self.fixture.receipt['checks'][0]['source_head'] = head
        self.publish()

        handler = json.loads((self.repo / entrypoint).read_text(encoding='utf-8'))['hooks']['Stop'][0]['hooks'][0]
        if os.name == 'nt':
            command = ['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', handler['commandWindows']]
        else:
            command = ['sh', '-c', handler['command']]

        def stop():
            result = subprocess.run(command, cwd=self.repo, check=True, input=json.dumps(self.payload),
                                    capture_output=True, text=True, encoding='utf-8', timeout=60)
            return json.loads(result.stdout)

        # Before setup, both discovered Keld handlers visibly refuse; there
        # must be no fallback to PATH Git and no execution of checkout code.
        answer = stop()
        self.assertIs(answer['continue'], False)
        self.assertIn('--configure-codex', answer['stopReason'])
        prompt_payload = dict(self.payload, hook_event_name='UserPromptSubmit')
        prompt_result = subprocess.run(command, cwd=self.repo, input=json.dumps(prompt_payload),
                                       capture_output=True, text=True, encoding='utf-8', timeout=60)
        self.assertEqual(prompt_result.returncode, 0, prompt_result.stderr)
        self.assertEqual(json.loads(prompt_result.stdout)['decision'], 'block')
        self.assertIn('--configure-codex', json.loads(prompt_result.stdout)['reason'])
        self.assertEqual(self.fixture.git('status', '--porcelain'), '')
        configured = subprocess.run(
            [sys.executable, '-B', 'tools/session_closeout_hook.py', '--configure-codex',
             '--git-executable', trusted_git_for_tests()],
            cwd=self.repo, check=True, capture_output=True, text=True, encoding='utf-8', timeout=30)
        self.assertIn('trusted Git binding installed', configured.stdout)
        local = hook.codex_trust_path(self.repo)
        self.assertTrue(local.is_file())
        self.addCleanup(lambda: local.unlink(missing_ok=True))
        if os.name != 'nt':
            # Reusing identical JSON must not falsely report success for a
            # manifest that the native hook will reject as non-private.
            local.chmod(0o644)
            rerun = subprocess.run(configured.args, cwd=self.repo, capture_output=True,
                                   text=True, encoding='utf-8', timeout=30)
            self.assertNotEqual(rerun.returncode, 0)
            self.assertIn('manifest is unsafe', rerun.stderr)
            repair = subprocess.run([*configured.args, '--replace-local-binding'], cwd=self.repo,
                                    capture_output=True, text=True, encoding='utf-8', timeout=30)
            self.assertEqual(repair.returncode, 0, repair.stderr)
            self.assertFalse(local.stat().st_mode & 0o077)
        self.assertFalse((self.repo / '.codex/hooks.local.json').exists())
        answer = stop()
        self.assertIn('accepted: complete', answer.get('systemMessage', str(answer)))
        prompt_result = subprocess.run(command, cwd=self.repo, input=json.dumps(prompt_payload),
                                       capture_output=True, text=True, encoding='utf-8', timeout=60)
        self.assertEqual(prompt_result.returncode, 0, prompt_result.stderr)
        self.assertIn('session closeout requirements',
                      json.loads(prompt_result.stdout)['hookSpecificOutput']['additionalContext'])
        self.assertEqual(self.fixture.git('status', '--porcelain'), '')
        self.assertEqual(self.fixture.git('ls-files', '--', '.codex/hooks.json'), '.codex/hooks.json')
        # Neither an invalid replacement of the local trusted Git path nor a
        # hostile executable from the checkout may run before hash validation.
        fake = self.repo / 'fake-git'
        fake.write_text('#!/bin/sh\necho HOSTILE_GIT_EXECUTED >&2\nexit 77\n', encoding='utf-8')
        fake.chmod(0o755)
        (self.repo / '.git/info/exclude').write_text('fake-git\n', encoding='utf-8')
        original = json.loads(local.read_text(encoding='utf-8'))
        local.write_text(json.dumps(dict(original, git_executable=str(fake))), encoding='utf-8')
        rejected = stop()
        self.assertIs(rejected['continue'], False)
        self.assertIn('outside the checkout', rejected['stopReason'])
        self.assertNotIn('HOSTILE_GIT_EXECUTED', json.dumps(rejected))
        # An outside executable must also match the reviewed binary digest.
        outside = self.fixture.root / 'outside-git'
        outside.write_text('#!/bin/sh\necho OUTSIDE_HOSTILE_GIT >&2\nexit 77\n', encoding='utf-8')
        outside.chmod(0o755)
        local.write_text(json.dumps(dict(original, git_executable=str(outside))), encoding='utf-8')
        rejected = stop()
        self.assertIs(rejected['continue'], False)
        self.assertIn('trusted Git binary changed', rejected['stopReason'])
        self.assertNotIn('OUTSIDE_HOSTILE_GIT', json.dumps(rejected))
        local.write_text(json.dumps(original), encoding='utf-8')
        # Keep the same receipt and configuration; one unrelated tracked
        # edit must still fail closed.
        with (self.repo / '.gitignore').open('a', encoding='utf-8') as ignore:
            ignore.write('\n# Uncommitted tracked edit.\n')
        answer = stop()
        self.assertEqual(answer['decision'], 'block')
        self.assertIn('dirty tracked files', answer['reason'])

    def test_codex_linked_worktree_never_inherits_parent_binding(self):
        source_root = Path(__file__).resolve().parent.parent
        files = [Path('.gitignore'), Path('.gitattributes'), Path('.codex/hooks.json'),
                 *(Path('tools') / (name + '.py')
                   for name in ('session_closeout', 'workspace', 'session_closeout_hook'))]
        for path in files:
            (self.repo / path).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source_root / path, self.repo / path)
        self.fixture.git('add', '--', *(str(path) for path in files))
        self.fixture.git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                         'commit', '-qm', 'track native Codex entrypoint')
        trusted_git = trusted_git_for_tests()
        setup = [sys.executable, '-B', 'tools/session_closeout_hook.py',
                 '--configure-codex', '--git-executable', trusted_git]
        subprocess.run(setup, cwd=self.repo, check=True, capture_output=True, timeout=30)
        main_trust = hook.codex_trust_path(self.repo)
        self.addCleanup(lambda: main_trust.unlink(missing_ok=True))
        linked = self.fixture.root / 'linked-task'
        self.fixture.git('worktree', 'add', '-b', 'task-hooks', str(linked))
        registered = json.loads((linked / '.codex/hooks.json').read_text(encoding='utf-8'))
        handler = registered['hooks']['Stop'][0]['hooks'][0]
        command = (['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', handler['commandWindows']]
                   if os.name == 'nt' else ['sh', '-c', handler['command']])
        payload = dict(self.payload, cwd=str(linked))
        def stop():
            result = subprocess.run(command, cwd=linked, input=json.dumps(payload),
                                    capture_output=True, text=True, encoding='utf-8', timeout=60)
            self.assertEqual(result.returncode, 0, result.stderr)
            return json.loads(result.stdout)
        self.assertIn('--configure-codex', stop()['stopReason'])
        linked_setup = subprocess.run(setup, cwd=linked, capture_output=True,
                                      text=True, encoding='utf-8', timeout=30)
        self.assertEqual(linked_setup.returncode, 0, linked_setup.stderr)
        linked_trust = hook.codex_trust_path(linked)
        self.addCleanup(lambda: linked_trust.unlink(missing_ok=True))
        self.assertIn('No activated Keld task', stop()['systemMessage'])
        self.assertEqual(self.fixture.git('status', '--porcelain'), '')
        self.assertEqual(self.fixture.git('-C', str(linked), 'status', '--porcelain'), '')

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
        self.assertIn(str(self.repo / ".keld-work" / "sessions" / "session_214"), answer["additionalContext"])
        self.assertNotIn("PRIVATE PROMPT", json.dumps(answer))
        self.assertFalse(self.receipt_path.parent.exists())
        self.assertFalse((self.repo / ".keld-work").exists())
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
                for name in ('session_closeout', 'workspace', 'session_closeout_hook'):
                    shutil.copyfile(source_root / 'tools' / (name + '.py'),
                                    self.repo / 'tools' / (name + '.py'))
                config = hook.configuration(harness, git_executable=trusted_git_for_tests())
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
