"""Compare completed native cold screens over a common full carrier cycle.

Uses trapezoidal time averages of accepted diagnostics, not a field solver.
The half-difference isolates phase-odd response; the half-sum records bias.
"""

import argparse
import csv
import hashlib
import json
from pathlib import Path


FIELDS = ["roi_force_z_N", "liquid_joule_power_W", "solid_joule_power_W",
          "radial_pressure_proxy_Pa"]


def average(directory, start, end):
    status = json.loads((directory / "run-status.json").read_text())
    if status["status"] != "complete" or status["time_s"] < end - 1e-12:
        raise ValueError(f"Incomplete requested interval: {directory}")
    source = directory / "diagnostics.csv"
    rows = [r for r in csv.DictReader(source.open())
            if start - 1e-12 <= float(r["time_s"]) <= end + 1e-12]
    times = [float(r["time_s"]) for r in rows]
    if len(times) < 2 or abs(times[0] - start) > 1e-12 or abs(times[-1] - end) > 1e-12:
        raise ValueError("Cycle endpoints must be accepted diagnostic times")
    if any(b <= a for a, b in zip(times, times[1:])):
        raise ValueError("Diagnostic times are not strictly increasing")
    means = {key: sum((float(a[key]) + float(b[key])) * .5 * (tb - ta)
                     for a, b, ta, tb in zip(rows, rows[1:], times, times[1:]))
             / (end - start) for key in FIELDS}
    return {"directory": str(directory), "diagnostics_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "accepted_status": status, "samples": len(rows), "mean": means}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plus", type=Path)
    parser.add_argument("minus", type=Path)
    parser.add_argument("--start-us", type=float, default=80.)
    parser.add_argument("--end-us", type=float, default=100.)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 0 <= args.start_us < args.end_us:
        parser.error("Choose an increasing nonnegative interval")
    pair = [average(p, args.start_us * 1e-6, args.end_us * 1e-6)
            for p in [args.plus, args.minus]]
    if any(pair[0]["accepted_status"][k] != pair[1]["accepted_status"][k]
           for k in ["dims", "cell_m", "origin_m", "ROI_z_m"]):
        raise ValueError("Phase controls must share grid and diagnostic region")
    result = {"interval_us": [args.start_us, args.end_us], "method": __doc__,
              "plus": pair[0], "minus": pair[1],
              "phase_odd_half_difference": {k: (pair[0]["mean"][k] - pair[1]["mean"][k]) / 2 for k in FIELDS},
              "phase_even_half_sum": {k: (pair[0]["mean"][k] + pair[1]["mean"][k]) / 2 for k in FIELDS},
              "limits": ["Cold prescribed currents, not moving liquid or a driver circuit.",
                         "Radial force moment is not solved pressure.",
                         "Single early full-cycle average does not establish periodic steady state or convergence."]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
