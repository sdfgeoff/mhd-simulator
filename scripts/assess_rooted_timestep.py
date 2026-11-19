"""Assess completed matched-grid timestep or reversal controls without claiming convergence."""

import argparse
import csv
import hashlib
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--study", choices=["timestep", "retract"], default="timestep")
parser.add_argument("--output", type=Path)
args = parser.parse_args()
root = Path("artifacts/hardware-research")
names = ["m6-rooted08-flow110-r25-motionabs1e-06A-h010-dt3",
         "m6-rooted08-flow110-r25-motionabs1e-06A-h010-dt1p5"]
if args.study == "retract":
    names = ["m6-rooted08-flow110-r25-motionabs1e-06A-h010-dt1p5",
             "m6-rooted08-flow110-r50-motionabs1e-06A-h010-dt1p5"]
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
    current_rows = list(csv.DictReader((directory / "current-diagnostics.csv").open()))
    residual = max(float(r["charge_true_residual"]) for r in current_rows)
    command_dose = sum(float(r["command_A"])**2 * float(r["timestep_s"])
                       for r in current_rows)
    if residual > status["charge_requested_relative_tolerance"]:
        raise ValueError("True local residual failed")
    energy = (status["lagged_electrical_work_J"] - status["joule_generated_J"]
              - status["lagged_mechanical_work_J"]) / status["lagged_electrical_work_J"]
    if abs(energy) > 1e-4:
        raise ValueError("Electrical energy identity failed")
    summaries.append({"source": str(directory), "native_status": status,
                      "analysis_sha256": hashlib.sha256(analysis_path.read_bytes()).hexdigest(),
                      "maximum_true_local_residual": residual,
                      "accepted_step_command_I2dt_A2s": command_dose,
                      "accepted_step_command_RMS_A_at_assumed200Hz": (command_dose*200)**.5,
                      "command_dose_scope": "Piecewise constant accepted-step forcing; not exact continuous driver waveform. Rate200Hz is arithmetic only.",
                      "energy_identity_relative_defect": energy,
                      "final_regions_by_alpha": analysis["frames"][-1]["connectivity_thresholds"],
                      "final_below_tip_nl": analysis["frames"][-1]["below_tip_nl"],
                      "max_relative_volume_drift": max(abs(f["relative_liquid_volume_change"])
                                                       for f in analysis["frames"]),
                      "any_saved_boundary_liquid": any(f["liquid_touches_domain_boundary_alpha01"]
                                                       for f in analysis["frames"])})
    statuses.append(status)
for key in ["project", "grid", "charge_solver", "charge_requested_relative_tolerance",
            "peak_A", "positive_duration_s", "rise_s", "retract_delay_s",
            "contact_resistivity_ohm_m2", "frequency_hz", "uniform_acceleration_m_s2",
            "periodic_fluid_axes", "terminal_boundary", "pulse_start_s"]:
    if statuses[0][key] != statuses[1][key]:
        raise ValueError(f"Unexpected difference beyond timestep: {key}")
if statuses[0]["charge_boundary_criteria"]["motional_absolute_tolerance_A"] != statuses[1]["charge_boundary_criteria"]["motional_absolute_tolerance_A"]:
    raise ValueError("Different motional current criteria")
if args.study == "timestep":
    if statuses[0]["retract_fraction"] != statuses[1]["retract_fraction"]:
        raise ValueError("Different reversal in timestep control")
    if abs(statuses[1]["timestep_s"] / statuses[0]["timestep_s"] - .5) > 1e-8:
        raise ValueError("Expected timestep halving")
else:
    if statuses[0]["timestep_s"] != statuses[1]["timestep_s"] or [s["retract_fraction"] for s in statuses] != [.25, .5]:
        raise ValueError("Expected matched timestep and .25/.5 reversal")
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
result = {"status": "Completed fixed-grid timestep sensitivity; not temporal convergence" if args.study == "timestep" else "Completed fixed-grid reverse-current comparison; not grid convergence",
          "study": args.study,
          "runs": summaries, "comparisons": comparisons,
          "limits": ["Two timesteps do not establish an asymptotic convergence rate." if args.study == "timestep" else "Two reversal strengths at one grid do not establish an optimum or clean repeated jetting.",
                     "The grid is still100um; breakup, wetting and thin wall layers remain unresolved.",
                     "One pulse, imposed permanent B, no thermal evolution or circuit switching."]}
output = args.output or Path("hardware/rooted-timestep-screening.json" if args.study == "timestep" else "hardware/rooted-retract-screening.json")
with output.open("x") as stream:
    json.dump(result, stream, indent=2)
    stream.write("\n")
print(json.dumps(comparisons, indent=2))
