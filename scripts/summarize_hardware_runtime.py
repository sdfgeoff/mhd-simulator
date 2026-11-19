"""Summarize saved host wall timings; no solver dispatches or benchmark claims."""

import csv
import hashlib
import json
import statistics
from collections import defaultdict
from pathlib import Path

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

directory = root / "coil-low-fill-flow-v1-0A-h020"
status = json.loads((directory / "run-status.json").read_text())
if status["status"] != "complete":
    raise ValueError("Require completed unpowered coil control")
source = directory / "run-performance.csv"
totals = defaultdict(float)
steps = set()
for row in csv.DictReader(source.open()):
    if row["accepted"] != "true":
        raise ValueError("Unexpected rejected-step profile")
    totals[row["stage"]] += float(row["cpu_wall_ms"])/1000
    steps.add(int(row["step"]))
if len(steps) != status["accepted_steps"]:
    raise ValueError("Missing stage profile steps")
wall = sum(totals.values())
coil = {"source": str(directory), "adapter": "AMD RADV RENOIR",
        "accepted_steps": len(steps), "profiled_host_wall_s": wall,
        "stages": [{"stage": k, "host_wall_s": v, "fraction": v/wall}
                   for k, v in sorted(totals.items(), key=lambda x: -x[1])],
        "timing_csv_sha256": hashlib.sha256(source.read_bytes()).hexdigest()}
result = {"status": "Measured saved timings, not a controlled hardware/algorithm benchmark",
          "direct_contact_runs": cases, "unpowered_coil_control": coil,
          "inferences": [
              "Direct-contact fluid work dominates the saved charge-plus-fluid wall time; finer-grid planning must allow about2hours of measured solver-stage time for the existing50um case.",
              "Pressure projection dominates the unpowered AMD control. This does not establish the bottleneck in the still-running powered coil case.",
              "A low-amplitude induction tail may offer savings, but iteration count alone is insufficient evidence to remove it. Require complete powered-case timings and physical error/energy checks first."
          ],
          "limits": [
              "Host wall time includes waiting for GPU work; it does not identify CPU computation as the bottleneck.",
              "Static setup, field-export I/O and postprocessing are excluded from direct-contact stage totals.",
              "Different grids, charge implementations, adapters, concurrent work, clocks and temperatures prevent treating these ratios as controlled speedups.",
              "No active run or frozen input is modified. No new solver optimization is claimed."
          ]}
output = Path("hardware/current-runtime-screening.json")
with output.open("x") as stream:
    json.dump(result, stream, indent=2)
    stream.write("\n")
print(json.dumps({"direct_contact_solver_s": [c["measured_solver_stage_s"] for c in cases],
                  "unpowered_coil_stage_fractions": coil["stages"]}, indent=2))
