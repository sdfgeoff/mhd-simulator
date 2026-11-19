"""Prepare normal unified RunState zero/driven controls; no GPU work."""
import copy
import argparse
import hashlib
import json
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


root = Path("artifacts/hardware-research")
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--geometry", choices=["low-fill", "high-fill", "v6"], default="low-fill")
args = parser.parse_args()
tag = "coil-" + args.geometry + "-flow-v1"
folder = root / "cases" / tag
folder.mkdir(parents=True, exist_ok=False)
source = root / {
    "low-fill": "cases/coil-fill-control-v1/low-symmetric100-flow-h010.mhd.json",
    "high-fill": "cases/coil-fill-control-v1/high-symmetric100-flow-h010.mhd.json",
    "v6": "cases/unipolar-coil-v2/v6-symmetric100-cold-h020.mhd.json",
}[args.geometry]
base = json.loads(source.read_text())
cold_plan = root / "unipolar-coil-fast10-v2-plan.json"
cold_geometry = "v6" if args.geometry == "v6" else "custom"
cold_job = next(j for j in json.loads(cold_plan.read_text())["jobs"] if j["geometry"] == cold_geometry)
cold = json.loads(Path(cold_job["project"]).read_text())
assert base["cad_curves"] == cold["cad_curves"]
assert base["materials"] == cold["materials"]
assert base["fluid"]["material_id"] == 4
jobs = []
for peak in [0, 250]:
    p = copy.deepcopy(base)
    p["geometry"] = str((source.parent / p["geometry"]).resolve())
    p["domain"].update(min_mm=[-9, -9, -4], max_mm=[9, 9, 14.6], cell_mm=.2)
    p["run"].update(fluid_enabled=True, thermal_enabled=False, dt_s=.5e-6, prescribed_jet=None)
    p["fluid"]["surface"]["enabled"] = True
    p["fluid"]["acceleration_m_s2"] = [0, 0, -9.81]
    p["electromagnetics"] = copy.deepcopy(cold["electromagnetics"])
    p["electromagnetics"]["wires"][0]["current"]["samples"] = [
        [0, 0], [10e-6, peak], [110e-6, 0], [2.1e-3, 0]
    ]
    path = folder / f"{args.geometry}-{peak}A-flow-h020.mhd.json"
    path.write_text(json.dumps(p, indent=2) + "\n")
    output = root / f"{tag}-{peak}A-h020"
    jobs.append({"peak_A": peak, "project": str(path), "project_sha256": sha(path),
                 "output": str(output), "requested_steps": 1800, "end_time_s": 2.1e-3,
                 "command": ["target/debug/mhd-workbench", "--run-check", str(path), "1800",
                             "--output", str(output), "--field-every", "300",
                             "--tail-after-us", "300", "--tail-dt-us", "1.5"]})
plan = {
    "status": "Prepared zero/driven normal unified flow pair; no numerical result",
    "source_sha256": {str(source): sha(source), str(cold_plan): sha(cold_plan)},
    "geometry": args.geometry,
    "cold_geometry": cold_geometry,
    "jobs": jobs,
    "purpose": "Test moving interface under the original single unipolar coil mechanism, with a matched unpowered control. No prescribed pressure, jet or frozen force field.",
    "limits": [
        "200um exploratory mesh gives only2.5cells across0.5mm outlet; neither absence nor presence of a detached component certifies jetting.",
        {
            "low-fill": "Low free level5.6mm and fixed feed tip5.5mm leave0.1mm immersion, below coarse spacing. Inspect native voxelization and refine before conclusions.",
            "high-fill": "Original9mm free level lies in the0.6/0.4mm radius feed annulus:0.2mm radial gap is one cell. The free area is44.2 times smaller than low-fill; geometry and forcing change together. No resolved return-path or jetting claim. Bottle-shaped chamber needs a joined cap, not a simple bored one-piece part.",
            "v6": "Drawing-based V6 approximation:0.5mm outlet,0.9mm land,3.2mm inlet. Not a verified catalogue stainless0.5mm SKU. Existing V6 carrier metadata records these limits.",
        }[args.geometry],
        "Negative-z domain extends to−4mm versus−2mm in the cold screen; no assumption of identical native force is made.",
        "Common clock0.5us through300us, then1.5us for both EM/fluid; changing timestep does not freeze EM coupling.",
        "No thermal evolution, calibrated wetting, oxide, repetition or hardware qualification. Final duration2.1ms may require extension for breakup assessment.",
        "The source encircling coil clashes with a conventional aluminium heater block; alternate heating/package geometry remains unresolved. These are mechanism controls, not assembled hardware designs.",
    ],
}
path = root / (tag + "-plan.json")
path.write_text(json.dumps(plan, indent=2) + "\n")
Path("hardware/coil-" + args.geometry + "-flow-plan.json").write_text(json.dumps(plan, indent=2) + "\n")
print(path)
