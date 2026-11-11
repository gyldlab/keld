import unittest,importlib.util
from pathlib import Path
s=importlib.util.spec_from_file_location('view',Path(__file__).with_name('preview_lifetimes.py'));v=importlib.util.module_from_spec(s);s.loader.exec_module(v)
class Tests(unittest.TestCase):
 def source(self):return '\nUNTOUCHED BODY\n'.join(a for a,b,n in v.OPERATIONS for _ in range(n))
 def test_exact_reversible_type_only(self):
  source=self.source();result=v.transform(source)
  for before,after,count in reversed(v.OPERATIONS):result=result.replace(after,before)
  self.assertEqual(result,source)
 def test_both_impls(self):
  result=v.transform(self.source());self.assertEqual(result.count("impl AuthenticatedLocalBaseline<'_> {"),2)
 def test_missing(self):
  with self.assertRaises(ValueError):v.transform(self.source().replace(v.OPERATIONS[0][0],'missing'))
 def test_duplicate(self):
  with self.assertRaises(ValueError):v.transform(self.source()+v.OPERATIONS[0][0])
 def test_repeated(self):
  with self.assertRaises(ValueError):v.transform(v.transform(self.source()))
if __name__=='__main__':unittest.main()
