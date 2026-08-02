import importlib.util,pathlib,unittest
ROOT=pathlib.Path(__file__).resolve().parents[2]; s=importlib.util.spec_from_file_location('t',ROOT/'osl-plan/trap_ledger.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
def trap():return {'id':'TRAP-STORE-1','location':'crates/store/tests/x.rs','context':'migration test','symptom':'read fails','actual_cause':'stale schema','failed_obvious_approach':'retry','discriminating_check':'inspect version','safe_invariant':'migrate before read','evidence':'test name','last_verified':'build 4 / 2026-08-02','review_by':'2026-09-01'}
class TrapLedgerTests(unittest.TestCase):
 def test_rt_32_compact_qualifying_fixture_passes(self):m.validate([trap()])
 def test_rt_32_duplicate_or_obsolete_is_refused(self):
  bad=trap();bad['id']='TRAP-STORE-2'
  with self.assertRaises(m.TrapError):m.validate([trap(),bad])
  with self.assertRaises(m.TrapError):m.validate([{'id':'wrong'}])
 def test_rt_32_overdue_active_trap_is_refused(self):
  with self.assertRaisesRegex(m.TrapError,'overdue hygiene review'):m.validate([trap()],'2026-09-02')
 def test_rt_32_master_placement_refused(self):
  with self.assertRaises(m.TrapError):m.check_placement('TRAP-UI-1 runtime warning','| subsystem | traps |')
if __name__=='__main__':unittest.main()
