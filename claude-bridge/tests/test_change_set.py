import importlib.util, pathlib, unittest
ROOT = pathlib.Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("change_set", ROOT / "osl-plan/change_set.py")
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)

class ChangeSetTests(unittest.TestCase):
    def valid(self, kind):
        return {"change_id":"x", "classification":[kind], "semantic_change":True,
          "authorities": list(module.RULES[kind]), "affected_artifacts":["docs/x.md", "tests/x.py"],
          "layman_update_not_due":"does not alter product explanation", "compact_memory":{"field_changed":False}}
    def test_rt_29_required_authorities_refuse(self):
        for kind in ("product-intent", "security", "dependency-owner-blocker", "deadline"):
            bad=self.valid(kind); bad["authorities"].pop()
            with self.assertRaises(module.ChangeSetError): module.validate(bad)
    def test_rt_29_no_semantic_change_needs_no_layman_churn(self):
        module.validate({"change_id":"evidence", "classification":[], "semantic_change":False, "authorities":[], "affected_artifacts":[]})
if __name__ == "__main__": unittest.main()
