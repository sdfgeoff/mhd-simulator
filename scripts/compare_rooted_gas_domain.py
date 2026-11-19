"""Gate a smaller exterior-gas approximation against the completed full geometry.

This only bounds selected primary jet metrics at the coarse grid. It does not
validate thermal physics, satellite statistics, or the full velocity field.
"""

import argparse, json
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("candidate", type=Path)
p.add_argument(
    "--reference",
    type=Path,
    default=Path(
        "artifacts/hardware-research/m6-rooted08-flow110-r25-vented-tol1e4-h010"
    ),
)
p.add_argument("--output", type=Path, required=True)
a = p.parse_args()


def read(run):
    status = json.loads((run / "run-status.json").read_text())
    if status["status"] != "complete":
        raise ValueError("Incomplete comparison: " + str(run))
    return json.loads((run / "flow-analysis.json").read_text())


reference = read(a.reference)
candidate = read(a.candidate)
checks = []


def check(name, value, limit):
    checks.append(
        {
            "metric": name,
            "absolute_relative_difference": value,
            "limit": limit,
            "pass": value <= limit,
        }
    )


check(
    "initial_liquid_volume",
    abs(
        candidate["frames"][0]["liquid_volume_nl"]
        / reference["frames"][0]["liquid_volume_nl"]
        - 1
    ),
    1e-6,
)
if len(reference["frames"]) != len(candidate["frames"]):
    raise ValueError("Different sample counts")
for ref, can in zip(reference["frames"][1:], candidate["frames"][1:]):
    if abs(ref["time_us"] - can["time_us"]) > 0.01:
        raise ValueError("Different observation times")
    tag = str(round(ref["time_us"])) + "us"
    if ref["below_tip_nl"] > 1:
        check(
            tag + "_below_tip_volume",
            abs(can["below_tip_nl"] / ref["below_tip_nl"] - 1),
            0.05,
        )
    check(tag + "_total_volume_drift", abs(can["relative_liquid_volume_change"]), 1e-5)
    if can["liquid_touches_domain_boundary_alpha01"]:
        checks.append({"metric": tag + "_boundary_contact", "pass": False})
    for threshold, regions in ref["connectivity_thresholds"].items():
        actual = can["connectivity_thresholds"][threshold]
        if not regions:
            # A new large detached region before reference breakup is material.
            checks.append(
                {
                    "metric": tag + "_early_detachment_" + threshold,
                    "pass": not any(r["volume_nl"] > 10 for r in actual),
                }
            )
            continue
        primary = max(regions, key=lambda r: r["volume_nl"])
        if primary["volume_nl"] < 10:
            continue
        if not actual:
            checks.append(
                {"metric": tag + "_primary_detachment_" + threshold, "pass": False}
            )
            continue
        other = max(actual, key=lambda r: r["volume_nl"])
        for key in ["volume_nl", "equivalent_diameter_mm"]:
            check(
                tag + "_" + key + "_" + threshold,
                abs(other[key] / primary[key] - 1),
                0.05,
            )
        for key, index in [("centre_mm", 2), ("velocity_m_s", 2)]:
            expected = primary[key][index]
            if abs(expected) > 0.1:
                check(
                    tag + "_" + key + "_" + threshold,
                    abs(other[key][index] / expected - 1),
                    0.05,
                )
record = {
    "status": "passed" if all(c["pass"] for c in checks) else "failed",
    "reference": str(a.reference),
    "candidate": str(a.candidate),
    "checks": checks,
    "scope": "Explicit5%coarse primary-metric gate for omitting exterior heater from gas geometry; not validation of complete fields,thermal paths orsatellitecounts. Fine compact-gas screen may proceed only onpass; its spatial/timestep controls remain separate.",
}
a.output.write_text(json.dumps(record, indent=2) + "\n")
print(record["status"])
if record["status"] != "passed":
    raise SystemExit(1)
