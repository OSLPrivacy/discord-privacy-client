"""RT-34 contract test for the live plan dispatcher.

The dispatcher is intentionally outside this product worktree; this test loads
that authority directly rather than creating a disconnected copy.
"""
import importlib.util
import pathlib
import unittest

SOURCE = pathlib.Path("/home/liamw/osl-plan/dispatch1.py")
SPEC = importlib.util.spec_from_file_location("dispatch1", SOURCE)
dispatch = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(dispatch)


GATE = {"p0_closures": {"t5-c9", "t19-f6"},
        "plan_a_implementation_ids": {"t1-a1"}}
OPEN = {"t5-c9": {"status": "running"}, "t19-f6": {"status": "ready"}}


class PlanBDispatchGateTests(unittest.TestCase):
    def task(self, task_id, body=""):
        return {"task_id": task_id, "title": "fixture", "body": body}

    def test_rt_34_blocks_feature_and_plan_a_implementation_while_p0_open(self):
        self.assertIn("P0", dispatch.plan_b_reason(self.task("t1-f1"), OPEN, GATE))
        self.assertIn("Plan-A", dispatch.plan_b_reason(self.task("t1-a1"), OPEN, GATE))

    def test_rt_34_permits_p0_and_plan_a_support_set_analysis(self):
        self.assertIsNone(dispatch.plan_b_reason(self.task("t5-c9"), OPEN, GATE))
        support = self.task("t1-a2", "Plan A support-set analysis only")
        self.assertIsNone(dispatch.plan_b_reason(support, OPEN, GATE))


if __name__ == "__main__":
    unittest.main()
