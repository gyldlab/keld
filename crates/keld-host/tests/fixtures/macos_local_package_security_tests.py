#!/usr/bin/env python3
"""Portable actual-census oracle controls, not native installed acceptance."""
import errno
from pathlib import Path
import unittest
from unittest.mock import patch

import macos_local_package_security as security


class ExactCensusTests(unittest.TestCase):
    root = Path('/owned/Example.app')
    row = (42, 1, 'original-start', '/owned/Example.app/Contents/Helpers/bun')

    def census(self, executable, probe=None):
        with patch.object(security.smoke, 'process_rows', return_value=[self.row]), \
                patch.object(security.smoke, 'executable_of', return_value=executable), \
                patch.object(security.smoke.os, 'kill', side_effect=probe) as kill:
            result = security.exact_census(self.root)
        return result, kill

    def test_owned_exact_executable_is_retained_without_signal_probe(self):
        result, kill = self.census(str(self.root / 'Contents/Helpers/bun'))
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0]['pid'], 42)
        self.assertEqual(result[0]['executable'], str(self.root / 'Contents/Helpers/bun'))
        kill.assert_not_called()

    def test_known_outside_root_is_not_claimed_owned(self):
        result, kill = self.census('/owned/Example.app-other/Contents/Helpers/bun')
        self.assertEqual(result, [])
        kill.assert_not_called()

    def test_unknown_live_identity_cannot_pass_empty_cleanup(self):
        result, kill = self.census(None)
        self.assertEqual(result[0]['identity'], 'unknown')
        self.assertIsNone(result[0]['executable'])
        with self.assertRaises(RuntimeError):
            security.smoke.require(not result, 'cleanup is unproved')
        kill.assert_called_once_with(42, 0)

    def test_only_proved_esrch_exit_can_remove_unknown_candidate(self):
        result, kill = self.census(None, ProcessLookupError(errno.ESRCH, 'exited'))
        self.assertEqual(result, [])
        kill.assert_called_once_with(42, 0)

    def test_denied_and_failed_probes_preserve_inconclusive_candidate(self):
        for error in (PermissionError(errno.EPERM, 'denied'), OSError(errno.EIO, 'failed')):
            with self.subTest(error=type(error).__name__):
                result, kill = self.census(None, error)
                self.assertEqual(result[0]['identity'], 'unknown')
                kill.assert_called_once_with(42, 0)

    def test_executable_observation_error_is_not_absence(self):
        with patch.object(security.smoke, 'process_rows', return_value=[self.row]), \
                patch.object(security.smoke, 'executable_of', side_effect=OSError('unreadable')), \
                patch.object(security.smoke.os, 'kill'):
            result = security.exact_census(self.root)
        self.assertEqual(result[0]['identity'], 'unknown')

    def test_non_candidate_is_ignored_without_identity_or_signal_probe(self):
        with patch.object(security.smoke, 'process_rows', return_value=[(43, 1, 'start', '/bin/sleep')]), \
                patch.object(security.smoke, 'executable_of') as executable, \
                patch.object(security.smoke.os, 'kill') as kill:
            self.assertEqual(security.exact_census(self.root), [])
        executable.assert_not_called()
        kill.assert_not_called()


if __name__ == '__main__':
    unittest.main()
