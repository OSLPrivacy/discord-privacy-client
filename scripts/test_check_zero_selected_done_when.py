import textwrap
import unittest

from scripts import check_zero_selected_done_when as checker


class ZeroSelectedDoneWhenTests(unittest.TestCase):
    def parse(self, text):
        return checker.parse_tasks([("fixture.txt", textwrap.dedent(text).strip())])

    def test_flags_zero_failures_without_expected_selection(self):
        tasks = self.parse(
            """
            TASK 9001 - weak checker
            gates: none
            build: test
            who: agent
            done when: the full link checker reports zero failures.
            """
        )

        findings = checker.find_zero_selected_risks(tasks)

        self.assertEqual([finding.task.task_id for finding in findings], ["9001"])
        self.assertEqual(findings[0].expected_selection_count, 0)
        self.assertEqual(findings[0].zero_failure_count, 1)

    def test_expected_number_task_is_not_flagged(self):
        tasks = self.parse(
            """
            TASK 0010 [x] - prove the three blocker fixes together
            gates: 0007, 0008, 0009
            build: test
            who: agent
            done when: the command selects at least the expected number of tests, that number is greater than zero and is written in the result, there are zero failures.
            """
        )

        self.assertEqual(checker.find_zero_selected_risks(tasks), [])

    def test_ignores_self_reference_and_non_selection_failures(self):
        tasks = self.parse(
            """
            TASK 3751 - find every test that can pass with zero tests selected
            gates: none
            build: test
            who: agent
            done when: feeding the checker a made-up done-when that says zero failures with no expected number makes it flag exactly 1.

            TASK 3412 - write the switch between the two routes
            gates: none
            build: test
            who: agent
            done when: a direct command uses the grab, and the count of silent failures is 0.
            """
        )

        self.assertEqual(checker.find_zero_selected_risks(tasks), [])

    def test_made_up_done_when_flags_exactly_one(self):
        tasks = self.parse(
            """
            TASK 9999 - made-up weak line
            gates: none
            build: test
            who: agent
            done when: the command reports zero failures.
            """
        )

        self.assertEqual(len(checker.find_zero_selected_risks(tasks)), 1)


if __name__ == "__main__":
    unittest.main()
