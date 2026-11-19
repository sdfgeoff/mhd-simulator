"""Assess completed same-grid timestep controls without claiming convergence."""

import csv
import hashlib
import json
from pathlib import Path

root = Path("artifacts/hardware-research")
names = ["m6-rooted08-flow110-r25-motionabs1e-06A-h010-dt3",
         "m6-rooted08-flow110-r25-motionabs1e-06A-h010-dt1p5"]
summaries = []
statuses = []
for name in names:
    directory = root / name
    status = json.loads((directory / "run-status.json").read_text())
    analysis_path = directory / "flow-analysis.json"
    analysis = json.loads(analysis_path.read_text())
    if status["status"] != "complete" or status["accepted_steps"] != status["requested_steps"]:
        raise ValueError("Require completed native controls")
    if abs(status["time_s"] - .0036) > 1e-12 or abs(analysis["frames"][-1]["time_us"] - 3600) > 1e-6:
        raise ValueError("Require matched 3.6 ms reports")
    if status["tail_timestep_s"] != status["timestep_s"]:
        raise ValueError("Expected uniform timesteps")
    criteria = status["charge_boundary_criteria"]
    if criteria["max_unit_relative_imbalance"] > criteria["unit_relative_tolerance"]:
        raise ValueError("Unit-voltage conservation failed")
    residual = max(float(r["charge_true_residual"]) for r in
                   csv.DictReader((directory / "current-diagnostics.csv").open()))
    if residual > status["charge_requested_relative_tolerance"]:
        raise ValueError("True local residual failed")
    energy = (status["lagged_electrical_work_J"] - status["joule_generated_J"]
              - status["lagged_mechanical_work_J"]) / status["lagged_electrical_work_J"]
    if abs(energy) > 1e-4:
        raise ValueError("Electrical energy identity failed")
    summaries.append({"source": str(directory), "native_status": status,
                      "analysis_sha256": hashlib.sha256(analysis_path.read_bytes()).hexdigest(),
                      "maximum_true_local_residual": residual,
                      "energy_identity_relative_defect": energy,
                      "final_regions_by_alpha": analysis["frames"][-1]["connectivity_thresholds"],
                      "final_below_tip_nl": analysis["frames"][-1]["below_tip_nl"],
                      "max_relative_volume_drift": max(abs(f["relative_liquid_volume_change"])
                                                       for f in analysis["frames"]),
                      "any_saved_boundary_liquid": any(f["liquid_touches_domain_boundary_alpha01"]
                                                       for f in analysis["frames"])})
    statuses.append(status)
for key in ["project", "grid", "charge_solver", "charge_requested_relative_tolerance",
            "peak_A", "positive_duration_s", "rise_s", "retract_fraction", "retract_delay_s",
            "contact_resistivity_ohm_m2", "frequency_hz", "uniform_acceleration_m_s2",
            "periodic_fluid_axes", "terminal_boundary", "pulse_start_s"]:
    if statuses[0][key] != statuses[1][key]:
        raise ValueError(f"Unexpected difference beyond timestep: {key}")
if statuses[0]["charge_boundary_criteria"]["motional_absolute_tolerance_A"] != statuses[1]["charge_boundary_criteria"]["motional_absolute_tolerance_A"]:
    raise ValueError("Different motional current criteria")
if abs(statuses[1]["timestep_s"] / statuses[0]["timestep_s"] - .5) > 1e-8:
    raise ValueError("Expected timestep halving")
comparisons = []
for threshold in ["0.01", "0.25", "0.5", "0.75"]:
    regions = [s["final_regions_by_alpha"][threshold] for s in summaries]
    row = {"alpha_threshold": threshold, "region_counts": list(map(len, regions))}
    if all(regions):
        a, b = [max(r, key=lambda c: c["volume_nl"]) for r in regions]
        row.update(leading_diameter_relative_change=b["equivalent_diameter_mm"]/a["equivalent_diameter_mm"]-1,
                   leading_volume_relative_change=b["volume_nl"]/a["volume_nl"]-1,
                   leading_vertical_velocity_difference_m_s=b["velocity_m_s"][2]-a["velocity_m_s"][2])
    comparisons.append(row)
result = {"status": "Completed fixed-grid timestep sensitivity; not temporal convergence",
          "runs": summaries, "comparisons": comparisons,
          "limits": ["Two timesteps do not establish an asymptotic convergence rate.",
                     "The grid is still100um; breakup, wetting and thin wall layers remain unresolved.",
                     "One pulse, imposed permanent B, no thermal evolution or circuit switching."]}
output = Path("hardware/rooted-timestep-screening.json")
with output.open("x") as stream:
    json.dump(result, stream, indent=2)
    stream.write("\n")
print(json.dumps(comparisons, indent=2))
