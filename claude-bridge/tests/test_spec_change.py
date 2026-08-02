import importlib.util, pathlib, unittest
ROOT=pathlib.Path(__file__).resolve().parents[2]; s=importlib.util.spec_from_file_location('sc',ROOT/'osl-plan/spec_change.py'); m=importlib.util.module_from_spec(s); s.loader.exec_module(m)
def packet(): return {'owner_wording':'Persist encrypted profile','date':'2026-08-02','affected_ids':['F-1'],'supersession':{'old claim':'superseded'},'impact_map':list(m.IMPACT_AREAS),'contract':{'acceptance':'a','defaults':'d','migration':'m'},'dag':{'recalculated':True,'eta_current':True},'views':{'synchronized':True},'compatibility':{key:'planned' for key in m.COMPATIBILITY},'owner_notice':'plain language','change_set':{'change_id':'x','classification':['durable-design'],'semantic_change':True,'authorities':['design-guide','tokens','components','assets','screenshots','tests'],'affected_artifacts':['x'],'layman_update_not_due':'no copy change'},'feature_views':{'master':{'F-1':'planned'},'layman':{'F-1':'planned'},'checklist':{'F-1':'planned'}}}
class SpecChangeTests(unittest.TestCase):
 def test_rt_31_packet_passes(self): m.validate(packet())
 def test_rt_31_refuses_missing_contract_dag_or_compatibility(self):
  for key, mutate in [('migration',lambda p:p['contract'].pop('migration')),('dag',lambda p:p['dag'].update(eta_current=False)),('rollback',lambda p:p['compatibility'].pop('rollback'))]:
   with self.subTest(key=key):
    bad=packet(); mutate(bad)
    with self.assertRaises(m.SpecChangeError):m.validate(bad)
if __name__=='__main__':unittest.main()
