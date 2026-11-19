"""Prepare single-pulse coil comparisons without launching GPU work.

Cold screens retain stationary liquid labels. Flow inputs use the normal unified
RunState. A nonnegative coil current is not a DC current through the solder.
"""

import copy
import argparse
import hashlib
import json
import math
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save_new_or_identical(path, value):
    if path.exists():
        if json.loads(path.read_text()) != value:
            raise RuntimeError("Existing input differs: " + str(path))
    else:
        path.write_text(json.dumps(value, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", default="unipolar-coil-v2")
    parser.add_argument("--end-us", type=float, default=600,
                        help="Stationary screen horizon; retain at least50us after pulse")
    parser.add_argument("--pulse", nargs=3, action="append", metavar=("NAME", "RISE_US", "FALL_US"),
                        help="Repeat for explicit positive triangular pulse shapes; default100/100 and50/250")
    args = parser.parse_args()
    if not math.isfinite(args.end_us) or args.end_us <= 0 or abs(args.end_us * 2 - round(args.end_us * 2)) > 1e-8:
        parser.error("Require a positive half-microsecond screen horizon")
    if not args.tag or any(
        c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-"
        for c in args.tag
    ):
        parser.error("Use an alphanumeric/hyphen tag")
    root = Path("artifacts/hardware-research")
    folder = root / "cases" / args.tag
    folder.mkdir(parents=True, exist_ok=True)
    sources = {
        "custom": Path("hardware/designs/single-coil-250A/case.mhd.json"),
        "v6": root / "cases/v6-carrier-matched-v2/v6-cold.mhd.json",
    }
    bases = {label: json.loads(path.read_text()) for label, path in sources.items()}
    # Verify matching actuator, material and liquid/feed electrical assumptions;
    # different chamber volumes/lands remain intentional geometry differences.
    for key in ["cad_curves", "materials", "electromagnetics"]:
        if bases["custom"][key] != bases["v6"][key]:
            raise RuntimeError("Unexpected actuator/material mismatch: " + key)
    shapes = [("symmetric100", 100, 100), ("fast50-slow250", 50, 250)]
    if args.pulse:
        shapes = []
        for label, rise, fall in args.pulse:
            rise, fall = float(rise), float(fall)
            if (not label or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-" for c in label)
                    or not all(math.isfinite(t) and t > 0 and abs(t * 2 - round(t * 2)) < 1e-8 for t in [rise, fall])
                    or rise + fall > 300):
                parser.error("Require safe labels and positive half-microsecond durations totaling at most300us")
            shapes.append((label, rise, fall))
        if len({s[0] for s in shapes}) != len(shapes):
            parser.error("Pulse labels must be unique")
    if any(args.end_us < rise + fall + 50 for _, rise, fall in shapes):
        parser.error("Retain at least50us of zero-current decay")
    jobs = []
    source_hashes = {str(path): sha(path) for path in sources.values()}
    for geometry, base in bases.items():
        for label, rise, fall in shapes:
            project = copy.deepcopy(base)
            project["geometry"] = str(
                (sources[geometry].parent / base["geometry"]).resolve()
            )
            project["domain"].update(
                min_mm=[-9, -9, -2], max_mm=[9, 9, 14.6], cell_mm=0.2
            )
            project["run"].update(
                fluid_enabled=False, thermal_enabled=False, dt_s=0.5e-6
            )
            project["run"]["prescribed_jet"] = None
            project["fluid"]["surface"]["enabled"] = False
            project["fluid"]["acceleration_m_s2"] = [0, 0, -9.81]
            # Sparse linear table is exact for these ramps; no AC carrier.
            project["electromagnetics"]["wires"][0]["current"] = {
                "kind": "table",
                "samples": [
                    [0, 0],
                    [rise * 1e-6, 250],
                    [(rise + fall) * 1e-6, 0],
                    [args.end_us * 1e-6, 0],
                ],
            }
            path = folder / f"{geometry}-{label}-cold-h020.mhd.json"
            save_new_or_identical(path, project)
            output = root / f"{args.tag}-{geometry}-{label}-cold-h020"
            jobs.append(
                {
                    "geometry": geometry,
                    "shape": label,
                    "project": str(path),
                    "rise_us": rise,
                    "fall_us": fall,
                    "peak_A": 250,
                    "requested_steps": round(args.end_us * 2),
                    "end_time_s": args.end_us * 1e-6,
                    "output": str(output),
                    "project_sha256": sha(path),
                    "command": [
                        "target/debug/examples/hardware_probe",
                        str(path),
                        str(round(args.end_us * 2)),
                        str(output),
                        "0",
                        "9",
                    ],
                }
            )
            if geometry == "custom":
                # Prepared only; choose an actual driven flow after assessing
                # the cold force, loaded voltage and numerical losses.
                project["run"]["fluid_enabled"] = True
                project["fluid"]["surface"]["enabled"] = True
                project["domain"]["cell_mm"] = 0.1
                flow_path = folder / f"{geometry}-{label}-flow-h010.mhd.json"
                save_new_or_identical(flow_path, project)
    plan = {
        "status": "Prepared cold comparisons and optional flow inputs; no GPU result",
        "source_sha256": source_hashes,
        "jobs": jobs,
        "reference": "https://doi.org/10.1038/s44334-025-00029-w",
        "rationale": "Compare single nonnegative triangular coil-current pulses. Rise and fall can produce differently timed radial force lobes. Published shaped-pulse jetting motivates the test, not these exact amplitudes/durations or a solder outcome.",
        "scope": [
            "Same actuator/materials, common cold domain/grid/dt; custom and V6 geometry and liquid volume intentionally differ.",
            "Cold force moment is a diagnostic, not pressure, breakup or a droplet threshold.",
            f"Cold trial includes full{args.end_us:g}us; assess final reaction-field energy and work balance to verify sufficient decay.",
            "Retain existing 200um cold screens for continuity, then refine promising pulse and flow; optional100um flow has onlyfivecells across0.5mm outlet and1.5cells across0.15mm land.",
            "Estimate loaded terminal voltage from native reaction linkage plus independently checked air inductance and explicit winding/lead R. AirL*dI/dt alone does not qualify24/48V operation.",
            "No new heater arrangement, wall temperature, winding manufacture, contact angle, oxide or repetition qualification.",
        ],
    }
    path = root / f"{args.tag}-plan.json"
    save_new_or_identical(path, plan)
    print(path)


if __name__ == "__main__":
    main()
