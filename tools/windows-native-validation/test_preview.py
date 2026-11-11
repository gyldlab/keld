import hashlib,json,unittest
from pathlib import Path,PureWindowsPath
from preview_guard import transform,TARGETS
from preview_safety import reject_alias_components,outside_repo
class PreviewTests(unittest.TestCase):
    def test_fixed_targets(self):
        plan=json.loads((Path(__file__).parent/'guard-source-plan.json').read_text())
        self.assertEqual(set(plan),TARGETS)
    def test_new_files_have_reviewed_hash(self):
        plan=json.loads((Path(__file__).parent/'guard-source-plan.json').read_text())
        for entry in plan.values():
            if entry['before_sha256'] is None:
                self.assertEqual(hashlib.sha256(transform(None,entry)).hexdigest(),entry['after_sha256'])
    def test_preserves_unrelated_native_body(self):
        entry={'after_sha256':None,'operations':[{'before':'target\n','after':'replacement\n'}]}
        self.assertEqual(transform(b'unknown\ntarget\nmore unknown\n',entry),b'unknown\nreplacement\nmore unknown\n')
    def test_missing_and_duplicate_context_refuse(self):
        entry={'after_sha256':None,'operations':[{'before':'target\n','after':'replacement\n'}]}
        for raw in [b'unknown\n',b'target\ntarget\n',b'target\r\n']:
            with self.assertRaises(ValueError):transform(raw,entry)
    def test_new_file_cannot_replace_existing(self):
        entry={'after_sha256':None,'operations':[{'before':'','after':'new'}]}
        with self.assertRaises(ValueError):transform(b'existing',entry)
    def test_parent_alias_and_containment_refuse(self):
        with self.assertRaises(ValueError):reject_alias_components(PureWindowsPath('D:/KeldBuild/x/..'))
        self.assertFalse(outside_repo(PureWindowsPath('D:/KeldBuild'),PureWindowsPath('D:/KeldBuild/preview')))
        self.assertTrue(outside_repo(PureWindowsPath('D:/KeldBuild/keld'),PureWindowsPath('D:/KeldBuild/preview')))
if __name__=='__main__':unittest.main()
