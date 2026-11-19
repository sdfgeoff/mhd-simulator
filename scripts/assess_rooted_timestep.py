"""Assess completed single-variable electrode controls without claiming convergence."""

import argparse
import csv
import hashlib
import json
import math
from pathlib import Path
from rooted_control_pair import case_names, validate_pair, DESCRIPTIONS

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--study", choices=list(DESCRIPTIONS), default="timestep")
parser.add_argument("--reference-retract", type=float, default=.25)
parser.add_argument("--candidate-retract", type=float, default=.5)
parser.add_argument("--output", type=Path)
args = parser.parse_args()
root = Path("artifacts/hardware-research")
names = case_names(args.study, args.reference_retract, args.candidate_retract)
summaries = []
statuses = []
for name in names:
    directory = root / name
    status = json.loads((directory / "run-status.json").read_text())
    analysis_path = directory / "flow-analysis.json"
    analysis = json.loads(analysis_path.read_text())
    if status["status"] != "complete" or status.get("error") is not None or status["accepted_steps"] != status["requested_steps"]:
        raise ValueError("Require completed native controls")
    if abs(status["time_s"] - .0036) > 1e-12 or abs(analysis["frames"][-1]["time_us"] - 3600) > 1e-6:
        raise ValueError("Require matched 3.6 ms reports")
    if status["tail_timestep_s"] != status["timestep_s"]:
        raise ValueError("Expected uniform timesteps")
    criteria = status["charge_boundary_criteria"]
    if not math.isfinite(criteria["max_unit_relative_imbalance"]) or criteria["max_unit_relative_imbalance"] > criteria["unit_relative_tolerance"]:
        raise ValueError("Unit-voltage conservation failed")
    current_rows = list(csv.DictReader((directory / "current-diagnostics.csv").open()))
    if len(current_rows) != status["accepted_steps"] or [int(r["step"]) for r in current_rows] != list(range(1, status["accepted_steps"] + 1)):
        raise ValueError("Require diagnostics for every accepted step")
    if any(not math.isfinite(float(r[k])) for r in current_rows
           for k in ["charge_true_residual", "command_A", "timestep_s", "time_s"]):
        raise ValueError("Non-finite current diagnostics")
    if any(float(r["timestep_s"]) <= 0 for r in current_rows) or abs(sum(float(r["timestep_s"]) for r in current_rows) - status["time_s"]) > 1e-12:
        raise ValueError("Accepted diagnostic timesteps do not cover the native interval")
    residual = max(float(r["charge_true_residual"]) for r in current_rows)
    command_dose = sum(float(r["command_A"])**2 * float(r["timestep_s"])
                       for r in current_rows)
    if residual > status["charge_requested_relative_tolerance"]:
        raise ValueError("True local residual failed")
    energy = (status["lagged_electrical_work_J"] - status["joule_generated_J"]
              - status["lagged_mechanical_work_J"]) / status["lagged_electrical_work_J"]
    if not math.isfinite(energy) or abs(energy) > 1e-4:
        raise ValueError("Electrical energy identity failed")
    summaries.append({"source": str(directory), "native_status": status,
                      "analysis_sha256": hashlib.sha256(analysis_path.read_bytes()).hexdigest(),
                      "native_status_sha256": hashlib.sha256((directory / "run-status.json").read_bytes()).hexdigest(),
                      "current_diagnostics_sha256": hashlib.sha256((directory / "current-diagnostics.csv").read_bytes()).hexdigest(),
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
validate_pair(statuses, args.study, args.reference_retract, args.candidate_retract)
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
description, study_limit = DESCRIPTIONS[args.study]
result = {"status": description, "study": args.study,
          "runs": summaries, "comparisons": comparisons,
          "limits": [study_limit,
                     "The grid is still100um; breakup, wetting and thin wall layers remain unresolved.",
                     "One pulse, imposed permanent B, no thermal evolution or circuit switching.",
                     "Matching recorded native configuration does not prove historical project-file identity; use frozen queue manifests for input provenance."]}
output = args.output or Path(f"hardware/rooted-{args.study}-screening.json")
with output.open("x") as stream:
    json.dump(result, stream, indent=2, allow_nan=False)
    stream.write("\n")
print(json.dumps(comparisons, indent=2))
