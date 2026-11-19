"""Summarize saved host wall timings; no solver dispatches or benchmark claims."""

import argparse
import csv
import hashlib
import json
import statistics
from collections import defaultdict
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", type=Path, default=Path("hardware/current-runtime-screening.json"))
args = parser.parse_args()
root = Path("artifacts/hardware-research")
cases = []
for name in ["m6-rooted08-flow110-r25-motionabs1e-06A-h010-dt3",
             "m6-rooted08-flow110-r25-motionabs1e-06A-h010-dt1p5",
             "m6-rooted08-flow110-r50-motionabs1e-06A-h010-dt1p5",
             "m6-rooted08-flow110-r25-chargefix-h0050"]:
    directory = root / name
    status = json.loads((directory / "run-status.json").read_text())
    if status["status"] != "complete":
        raise ValueError("Only completed native cases may enter the summary")
    source = directory / "iteration-costs.csv"
    rows = list(csv.DictReader(source.open()))
    if len(rows) != status["accepted_steps"]:
        raise ValueError("Missing accepted-step wall timings")
    stages = {key: {"sum_s": sum(float(r[key]) for r in rows)/1000,
                    "median_ms": statistics.median(float(r[key]) for r in rows)}
              for key in ["charge_wall_ms", "fluid_wall_ms"]}
    cases.append({"source": str(directory), "accepted_steps": len(rows),
                  "grid": status["grid"], "timestep_s": status["timestep_s"],
                  "charge_solver": status["charge_solver"], "timings": stages,
                  "measured_solver_stage_s": sum(s["sum_s"] for s in stages.values()),
                  "timing_csv_sha256": hashlib.sha256(source.read_bytes()).hexdigest()})

def coil_profile(name):
    directory = root / name
    status = json.loads((directory / "run-status.json").read_text())
    if status["status"] != "complete":
        raise ValueError("Require completed coil control")
    source = directory / "run-performance.csv"
    totals = defaultdict(float)
    late = defaultdict(float)
    steps = set()
    for row in csv.DictReader(source.open()):
        if row["accepted"] != "true":
            raise ValueError("Unexpected rejected-step profile")
        value = float(row["cpu_wall_ms"])/1000
        totals[row["stage"]] += value
        if float(row["time_s"]) > .0003 + 1e-12:
            late[row["stage"]] += value
        steps.add(int(row["step"]))
    if len(steps) != status["accepted_steps"]:
        raise ValueError("Missing stage profile steps")
    wall = sum(totals.values())
    diagnostics = list(csv.DictReader((directory / "run-diagnostics.csv").open()))
    tail = [r for r in diagnostics if float(r["time_s"]) >= .0003-1e-12]
    return {"source": str(directory), "adapter": "AMD RADV RENOIR",
            "accepted_steps": len(steps), "profiled_host_wall_s": wall,
            "stages": [{"stage": k, "host_wall_s": v, "fraction": v/wall}
                       for k, v in sorted(totals.items(), key=lambda x: -x[1])],
            "late_after300us_host_wall_s_by_stage": dict(late),
            "late_EM_solve_fraction_of_total_wall": late["EM solve"]/wall,
            "maximum_magnetic_energy_after300us_J": max(float(r["magnetic_j"]) for r in tail),
            "maximum_B_after300us_T": max(float(r["max_B_T"]) for r in tail),
            "timing_csv_sha256": hashlib.sha256(source.read_bytes()).hexdigest()}

coil = coil_profile("coil-low-fill-flow-v1-0A-h020")
powered = coil_profile("coil-low-fill-flow-v1-250A-h020")
result = {"status": "Measured saved timings, not a controlled hardware/algorithm benchmark",
          "direct_contact_runs": cases, "unpowered_coil_control": coil, "powered_coil_control": powered,
          "inferences": [
              "Direct-contact fluid work dominates the saved charge-plus-fluid wall time; finer-grid planning must allow about2hours of measured solver-stage time for the existing50um case.",
              "Pressure projection dominates the unpowered control, while induction solves dominate the completed powered control. Use the measured stage split rather than assuming one bottleneck for both.",
              "The powered source-free tail consumes substantial EM solve wall time while stored field energy is tiny. An explicit, default-off energy-floor approximation could be assessed against an unchanged native baseline; no such cutoff is implemented or validated here."
          ],
          "limits": [
              "Host wall time includes waiting for GPU work; it does not identify CPU computation as the bottleneck.",
              "Static setup, field-export I/O and postprocessing are excluded from direct-contact stage totals.",
              "Different grids, charge implementations, adapters, concurrent work, clocks and temperatures prevent treating these ratios as controlled speedups.",
              "No active run or frozen input is modified. No new solver optimization is claimed."
          ]}
output = args.output
with output.open("x") as stream:
    json.dump(result, stream, indent=2)
    stream.write("\n")
print(json.dumps({"direct_contact_solver_s": [c["measured_solver_stage_s"] for c in cases],
                  "unpowered_coil_stage_fractions": coil["stages"],
                  "powered_late_EM_fraction": powered["late_EM_solve_fraction_of_total_wall"],
                  "powered_tail_max_energy_J": powered["maximum_magnetic_energy_after300us_J"]}, indent=2))
