"""Protect single-variable attribution in the hardware comparison reports."""

import copy
import json
from pathlib import Path
import unittest

from rooted_control_pair import case_names, validate_pair


class ControlPairTests(unittest.TestCase):
    def setUp(self):
        path = Path("artifacts/hardware-research") / case_names("delay")[0] / "run-status.json"
        self.reference = json.loads(path.read_text())

    def candidate(self, **changes):
        result = copy.deepcopy(self.reference)
        result.update(changes)
        return result

    def test_only_delay_changes(self):
        validate_pair([self.reference, self.candidate(retract_delay_s=0)], "delay")

    def test_interface_value_is_explicit(self):
        validate_pair([self.reference, self.candidate(contact_resistivity_ohm_m2=1e-9)], "interface")
        with self.assertRaises(ValueError):
            validate_pair([self.reference, self.candidate(contact_resistivity_ohm_m2=1e-8)], "interface")

    def test_reject_simultaneous_pulse_width_change(self):
        with self.assertRaisesRegex(ValueError, "positive_duration_s"):
            validate_pair([self.reference, self.candidate(retract_delay_s=0, positive_duration_s=200e-6)], "delay")

    def test_reject_simultaneous_current_change(self):
        with self.assertRaisesRegex(ValueError, "retract_fraction"):
            validate_pair([self.reference, self.candidate(retract_delay_s=0, retract_fraction=1)], "delay")

    def test_reject_contact_and_delay_change(self):
        with self.assertRaisesRegex(ValueError, "retract_delay_s"):
            validate_pair([self.reference, self.candidate(contact_resistivity_ohm_m2=1e-9, retract_delay_s=0)], "interface")

    def test_reject_different_conservation_allowance(self):
        candidate = self.candidate(retract_delay_s=0)
        candidate["charge_boundary_criteria"]["motional_absolute_tolerance_A"] = 1e-3
        with self.assertRaisesRegex(ValueError, "current criteria"):
            validate_pair([self.reference, candidate], "delay")

    def test_reject_nonfinite(self):
        with self.assertRaises(ValueError):
            validate_pair([self.reference, self.candidate(retract_delay_s=float("nan"))], "delay")

    def test_reject_unexpected_delay_baseline(self):
        reference = self.candidate(retract_delay_s=50e-6)
        with self.assertRaises(ValueError):
            validate_pair([reference, self.candidate(retract_delay_s=0)], "delay")


if __name__ == "__main__":
    unittest.main()
