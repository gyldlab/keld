"""Regression controls for the review-only Bun mutation oracle."""

import subprocess
import unittest

from negative_controls import expected_failure


class MutationOracleTests(unittest.TestCase):
    selected = "Prepare Retained keeps same port"
    name = selected + " and permits new preparation"
    marker = "retained-port-live"

    def accepts(self, diagnostic="", footer="", *, code=1):
        # Keep the exact same named test result in every case so the
        # discriminator is independent of the selector/exit checks.
        result = subprocess.CompletedProcess(
            [], code, "", f"{diagnostic}\n(fail) {self.name} [5.00ms]\n{footer}"
        )
        return expected_failure(result, self.name, self.selected, self.marker)

    def test_detects_only_the_expected_assertion(self):
        self.assertTrue(self.accepts("error: GH451_MUTATION_ASSERT: retained-port-live"))
        self.assertFalse(self.accepts("error: GH451_MUTATION_ASSERT: other-assertion"))
        self.assertFalse(self.accepts("error: GH451_MUTATION_ASSERT: retained-port-live", code=0))

    def test_rejects_same_named_exception_and_source_excerpt(self):
        self.assertFalse(
            self.accepts(
                '42 | expect(port.live, "GH451_MUTATION_ASSERT: retained-port-live")\n'
                "TypeError: unrelated setup failure"
            )
        )
        self.assertFalse(
            self.accepts(
                "error: GH451_MUTATION_ASSERT: retained-port-live\n"
                "TypeError: unrelated setup failure"
            )
        )

    def test_rejects_same_named_timeout_with_or_without_expected_marker(self):
        footer = "  ^ this test timed out after 5000ms."
        self.assertFalse(self.accepts(footer=footer))
        self.assertFalse(
            self.accepts("error: GH451_MUTATION_ASSERT: retained-port-live", footer)
        )


if __name__ == "__main__":
    unittest.main()
