import importlib.util,json,unittest
from pathlib import Path
import preview_update as driver
ROOT=Path(__file__).parent
PLAN=json.loads((ROOT/'update-source-plan.json').read_text())
class PreviewTests(unittest.TestCase):
    def test_fixed_targets(self):
        self.assertEqual(set(PLAN),driver.TARGETS)
        self.assertEqual(len(PLAN),28)
        self.assertTrue(all(name.startswith('crates/keld-update/') and '..' not in Path(name).parts for name in PLAN))
    def test_new_files_pinned_and_refuse_existing(self):
        for entry in PLAN.values():
            for variant in entry['variants']:
                if variant['before_sha256'] is None:
                    self.assertEqual(driver.sha(driver.transform(None,variant)),variant['after_sha256'])
                    with self.assertRaises(ValueError):driver.transform(b'old source',variant)
    def test_missing_duplicate_and_crlf_context_refuse(self):
        entry={'operations':[{'before':'unique\n','after':'changed\n'}],'after_sha256':None}
        for raw in [b'missing\n',b'unique\nunique\n',b'unique\r\n']:
            with self.assertRaises(ValueError):driver.transform(raw,entry)
    def test_unrelated_bytes_preserved(self):
        entry={'operations':[{'before':'anchor\n','after':'anchor\nnew\n'}],'after_sha256':None}
        self.assertEqual(driver.transform(b'unknown\nanchor\nmore unknown\n',entry),b'unknown\nanchor\nnew\nmore unknown\n')
if __name__=='__main__':unittest.main()
