import re
import unittest

from agent import add_neohorse_rollout_trace


class RolloutTraceTests(unittest.TestCase):
    def test_trace_is_stable_per_attempt_and_unique_across_attempts(self):
        instruction = "NEOHORSE_TASK_ID=repo-train-01-01\n\nFix the repository."
        first, marker = add_neohorse_rollout_trace(instruction, "/runs/job/trial-a")
        same, same_marker = add_neohorse_rollout_trace(instruction, "/runs/job/trial-a")
        _, next_marker = add_neohorse_rollout_trace(instruction, "/runs/job/trial-b")

        self.assertEqual(first, "Fix the repository.")
        self.assertEqual(first, same)
        self.assertEqual(marker, same_marker)
        self.assertNotEqual(marker, next_marker)
        self.assertRegex(marker, re.compile(r"^NEOHORSE_TRACE episode=[0-9a-f]{24} task=repo-train-01-01$"))

    def test_ordinary_instruction_is_not_modified(self):
        instruction = "Answer the user normally."
        self.assertEqual(add_neohorse_rollout_trace(instruction, "/run"), (instruction, None))


if __name__ == "__main__":
    unittest.main()
