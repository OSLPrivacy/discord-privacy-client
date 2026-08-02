import importlib.util, pathlib, unittest
ROOT=pathlib.Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('sync', ROOT/'scripts/check-feature-view-sync.py')
module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
GOOD={'F-1':'shipped','F-2':'blocked'}
class FeatureViewSyncTests(unittest.TestCase):
    def test_rt_30_synchronized_fixture_passes(self): module.reconcile(GOOD, GOOD, GOOD)
    def test_rt_30_missing_or_unknown_id_refuses(self):
        with self.assertRaisesRegex(ValueError, 'F-2.*master'): module.reconcile({'F-1':'shipped'}, GOOD, GOOD)
        with self.assertRaisesRegex(ValueError, 'F-9.*(master|internal-checklist)'): module.reconcile(GOOD, {**GOOD,'F-9':'planned'}, GOOD)
    def test_rt_30_status_mismatch_names_document_and_id(self):
        with self.assertRaisesRegex(ValueError, 'F-1.*layman=planned'): module.reconcile(GOOD, {'F-1':'planned','F-2':'blocked'}, GOOD)
if __name__=='__main__': unittest.main()
