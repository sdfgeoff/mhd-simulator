"""Require a tail-scheduled probe to preserve a completed reference's driven prefix."""

import argparse
import csv
import hashlib
import json
import math
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("reference", type=Path)
parser.add_argument("candidate", type=Path)
parser.add_argument("--through-us", type=float, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
if not math.isfinite(args.through_us) or args.through_us <= 0:
    parser.error("Require a finite positive prefix duration")
paths = [p / "diagnostics.csv" for p in [args.reference, args.candidate]]
rows = []
for directory, path in zip([args.reference, args.candidate], paths):
    status = json.loads((directory / "run-status.json").read_text())
    if status["status"] != "complete":
        raise ValueError("Both native probes must complete")
    rows.append([r for r in csv.DictReader(path.open())
                 if float(r["time_s"]) <= args.through_us * 1e-6 + 1e-12])
if len(rows[0]) != len(rows[1]) or len(rows[0]) < 2:
    raise ValueError("Different or missing driven-prefix samples")
columns = [k for k in rows[0][0]
           if k not in ["em_iterations", "true_relative_residual", "step"]]
worst = {k: 0. for k in columns}
for a, b in zip(*rows):
    if abs(float(a["time_s"]) - float(b["time_s"])) > 1e-12:
        raise ValueError("Different prefix clocks")
    for k in columns:
        # A blank reaction linkage is valid only in the native initial state.
        if a[k] == b[k] == "" and float(a["time_s"]) == 0:
            continue
        x, y = float(a[k]), float(b[k])
        if not all(map(math.isfinite, [x, y])):
            raise ValueError("Nonfinite native prefix")
        difference = abs(x - y)
        worst[k] = max(worst[k], difference)
        if difference > 1e-12 + 1e-5 * max(abs(x), abs(y)):
            raise ValueError(f"Driven-prefix mismatch in {k}: {x} versus {y}")
if abs(float(rows[0][-1]["time_s"]) - args.through_us * 1e-6) > 1e-12:
    raise ValueError("Prefix does not reach the requested transition")
result = {"status": "passed", "through_us": args.through_us,
          "samples": len(rows[0]), "absolute_tolerance": 1e-12,
          "relative_tolerance": 1e-5, "maximum_absolute_difference": worst,
          "sources_sha256": {str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                             for p in paths},
          "scope": "Driven prefix only. Separate native work and final decay gates remain required."}
with args.output.open("x") as output:
    json.dump(result, output, indent=2)
    output.write("\n")
print(args.output)
