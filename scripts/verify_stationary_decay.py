"""Verify an opt-in native stationary endpoint from its complete accepted CSV.

Energy accounting only: no field/fluid solver and no inferred droplet result.
"""
import argparse
import csv
import hashlib
import json
import math
from pathlib import Path
from analyze_unipolar_coil import reaction_sample


def verify(directory):
    status_path = directory / "run-status.json"
    status = json.loads(status_path.read_text())
    if status["status"] != "complete" or status.get("completion_criterion") != "stationary_decay":
        raise ValueError("Require native stationary-decay completion")
    steps = status["accepted_steps"]
    if not 0 < steps <= status["requested_max_steps"]:
        raise ValueError("Invalid accepted/max step counts")
    project_path = Path(status["project"])
    project = json.loads(project_path.read_text())
    cfg = project["electromagnetics"]
    if (project["run"]["fluid_enabled"] or project["run"]["thermal_enabled"]
        or project["fluid"]["surface"]["enabled"] or cfg["uniform"] or cfg["magnets"]
        or len(cfg["wires"]) != 1):
        raise ValueError("Endpoint is only for stationary, fixed-property, single-coil screens")
    after, _ = status["zero_current_tail_schedule_s"]
    wave = cfg["wires"][0]["current"]
    if (wave["kind"] != "table" or wave["samples"][0][1] != 0 or wave["samples"][-1][1] != 0
        or any(v != 0 for t, v in wave["samples"] if t >= after)):
        raise ValueError("Source must start at zero and remain zero after the declared transition")
    # Check interpolation at the transition, not only later sample points.
    for (ta, va), (tb, vb) in zip(wave["samples"], wave["samples"][1:]):
        if ta <= after <= tb and va + (vb-va)*(after-ta)/(tb-ta) != 0:
            raise ValueError("Source is nonzero at the tail transition")
    diagnostic_path = directory / "diagnostics.csv"
    rows = list(csv.DictReader(diagnostic_path.open()))
    if [int(r["step"]) for r in rows] != list(range(steps+1)):
        raise ValueError("Missing accepted native records")
    if abs(float(rows[-1]["time_s"]) - status["time_s"]) > 1e-12:
        raise ValueError("CSV and native endpoint times differ")
    if any(float(rows[0][key]) != 0 for key in ["time_s", "coil_current_A", "accumulated_joule_J", "reaction_magnetic_J", "reaction_BE_J"]) or reaction_sample(rows[0]) != 0:
        raise ValueError("Require an initially unforced, zero-energy native state")
    work = loss = 0.0
    confirmations = 0
    for i, row in enumerate(rows):
        time = float(row["time_s"])
        if not math.isfinite(time) or (i and time <= float(rows[i-1]["time_s"])):
            raise ValueError("Invalid accepted diagnostic clock")
        current = float(row["coil_current_A"])
        linkage = reaction_sample(row)
        if i:
            work -= linkage * (current-float(rows[i-1]["coil_current_A"]))
        loss += float(row["reaction_BE_J"])
        heat, field = float(row["accumulated_joule_J"]), float(row["reaction_magnetic_J"])
        accounted = heat+loss+field
        defect = abs(work-accounted)/max(abs(work),abs(accounted),1e-15)
        valid = all(math.isfinite(v) for v in [current,linkage,heat,field,work,loss,defect])
        passed = (valid and float(row["time_s"]) >= after and current == 0
                  and heat > 0 and field >= 0 and loss >= 0
                  and field/heat <= 1e-8 and defect <= 1e-3)
        confirmations = confirmations+1 if passed else 0
    if confirmations < 8 or status["stationary_decay_witness"]["confirming_steps"] < 8:
        raise ValueError("Eight successive accepted records must pass the original energy gates")
    profile = list(csv.DictReader((directory / "performance.csv").open()))
    covered = {int(r["step"]) for r in profile if r["accepted"] == "true" and int(r["step"]) > 0}
    if covered != set(range(1,steps+1)):
        raise ValueError("Incomplete accepted-step profiling coverage")
    if not (directory / f"frame-{steps:08}" / "metadata.json").exists():
        raise ValueError("Missing accepted endpoint field export")
    return {"status":"passed", "completion_criterion":"stationary_decay",
            "accepted_steps":steps,"maximum_requested_steps":status["requested_max_steps"],
            "accepted_end_time_s":status["time_s"],"reaction_energy_over_heat":field/heat,
            "metal_work_relative_defect":defect,"confirming_records":confirmations,
            "accepted_profile_steps":len(covered),
            "source_sha256":{str(p):hashlib.sha256(p.read_bytes()).hexdigest()
                             for p in [project_path,status_path,diagnostic_path]},
            "scope":"Stationary endpoint verification only. No field clamping, moving-fluid shortcut, repetition, thermal or droplet-performance claim."}


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory",type=Path)
    args=parser.parse_args()
    result=verify(args.directory)
    output=args.directory/"stationary-decay-verification.json"
    output.write_text(json.dumps(result,indent=2,allow_nan=False)+"\n")
    print(json.dumps(result,indent=2))
