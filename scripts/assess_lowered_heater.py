"""Assess completed compact-coil heater inclusion controls, not droplet performance."""
import copy
import hashlib
import json
import math
from pathlib import Path

root = Path("artifacts/hardware-research")
source = root / "lowered-coil-cold-v2-nvidia-comparison.json"
record = json.loads(source.read_text())
assert record["completed_cases"] == record["expected_cases"] == 2
off, on = record["cases"]
assert [c["geometry"] for c in [off, on]] == ["lowered-v6-heater-off", "lowered-v6-heater-on"]
plan = json.loads((root / "lowered-coil-cold-v2-nvidia-plan.json").read_text())
projects = [json.loads(Path(j["project"]).read_text()) for j in plan["jobs"]]
expected = copy.deepcopy(projects[0])
assert expected["bodies"][3]["simulation"]["included"] is False
expected["bodies"][3]["simulation"]["included"] = True
assert expected == projects[1], "Heater inclusion is not the sole project difference"
for case in [off, on]:
    assert case["metal_work_balance_gate_1e_minus3_passed"]
    assert case["final_reaction_energy_fraction_gate_1e_minus8_passed"]
    status = json.loads((Path(case["source"]) / "run-status.json").read_text())
    assert status["status"] == "complete" and status["accepted_steps"] == 1917
    assert abs(status["time_s"] - .005001) < 1e-12
ratio = on["radial_force_moment_over_ROI_volume_Pa_range"][1] / off["radial_force_moment_over_ROI_volume_Pa_range"][1]
scale = math.sqrt(1 / ratio)
result = {
    "status": "Completed stationary heater-inclusion control; no fluid or jetting result",
    "source_comparison_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
    "source": str(source),
    "only_project_difference": "Body3 heater simulation.included false -> true; winding, liquid ROI, materials and domain unchanged",
    "peak_inward_moment_ratio_on_over_off": ratio,
    "peak_inward_moment_reduction_fraction": 1 - ratio,
    "metal_heat_increase_J_per_pulse": on["metal_heat_J"] - off["metal_heat_J"],
    "extra_metal_heat_W_at_assumed200Hz": (on["metal_heat_J"] - off["metal_heat_J"]) * 200,
    "restore_heater_off_peak_moment_stationary_scaling_only": {
        "current_multiplier": scale,
        "peak_A": 250 * scale,
        "RMS_A_at_assumed200Hz": on["rms_A_at_assumed200Hz"] * scale,
        "terminal_interval_voltage_range_V": [x * scale for x in on["loaded_terminal_interval_mean_voltage_range_V"]],
        "scope": "Same waveform and fixed conductivity, force scales with I squared. Matching the heater-off moment is not a required pressure, droplet criterion or validated driver setting.",
    },
    "axial_force_interpretation": "Positive z is up. Heater-off rise impulse is up and fall impulse down; heater-on net force integrals in both phases are up. Whole-liquid force integral is not the momentum imparted to an expelled drop: pressure, wall reaction and evolving free interfaces matter.",
    "limits": [
        "Simplified empty-bore heater at fixed15MS/m; clamp slot, inserts, heatbreak and conductive contact paths absent. This is neither assembled-hardware prediction nor a strict shielding bound.",
        "200um spatial grid,0.5us pulse steps then3us decay; no grid/time convergence for this package.",
        "Single prescribed250A15/100us pulse; no thermal evolution, winding cooling, switching or repeated-drop qualification.",
        "This compact winding avoids nominal block overlap but is not yet a recommended jetting head. Assess practical heater interruption/placement and moving fluid before selecting it.",
    ],
}
output = Path("hardware/lowered-coil-heater-assessment.json")
with output.open("x") as f:
    json.dump(result, f, indent=2, allow_nan=False)
    f.write("\n")
Path("hardware/lowered-coil-cold-comparison.json").write_bytes(source.read_bytes())
print(json.dumps(result, indent=2))
