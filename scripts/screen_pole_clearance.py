"""Screen global CAD clearance and native GPU bias fields near the outlet.

No charge/fluid solve or jet prediction. Uses the native GPU applied-field mode;
the small observation domain must never be interpreted as a conduction domain.
"""

import argparse
import json
from pathlib import Path
import subprocess
import sys

import numpy as np

from draw_hardware_layouts import load
from make_hardware import heater
import manifold3d as cad


CASES = [
    ("original-disc", "disc", 10, 10, 5, -3.2, 4.2, False),
    ("raised-disc-collision", "disc", 10, 10, 5, -.2, 4.2, False),
    ("raised-disc-wide", "disc", 10, 10, 5, -.2, 8.4, False),
    ("raised-disc-machined", "disc", 10, 10, 5, -.2, 4.2, True),
    ("high-disc-wide", "disc", 10, 10, 5, 2.3, 8.4, False),
    ("flush-disc-wide", "disc", 10, 10, 5, 4.8, 8.4, False),
    ("short-block", "block", 10, 5, 5, 2.3, 4.2, True),
    ("short-block-stack", "block", 10, 5, 10, 2.3, 4.2, True),
    ("short-cube", "block", 5, 5, 5, 2.3, 4.2, True),
    ("short-cube-stack", "block", 5, 5, 10, 2.3, 4.2, True),
    ("stock-bar", "block", 25, 5, 5, 2.3, 4.2, True),
    ("stock-bar-stack", "block", 25, 5, 10, 2.3, 4.2, True),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", required=True)
    parser.add_argument("--subdivisions", type=int, default=4)
    parser.add_argument("--only", action="append", help="Run only this labelled layout; repeat for several")
    args = parser.parse_args()
    root = Path("artifacts/hardware-research")
    output = root / (args.prefix + "-results.json")
    if output.exists():
        raise FileExistsError(output)
    reference_heater = heater(6.5, 18.)
    # Convert only this reference solid; it is a clearance obstacle, not a source.
    points = np.array(reference_heater, dtype=np.float32).reshape(-1, 3)
    vertices, indices = np.unique(points, axis=0, return_inverse=True)
    reference_heater = cad.Manifold(cad.Mesh(vertices, indices.reshape(-1, 3).astype(np.uint32)))
    result = {"method": __doc__, "cases": [], "limits": [
        "Fixed uniform magnetization, unit permeability, assumed hot Br0.95T; no demagnetization or back iron.",
        "Small-domain fields only, not electrical/flow performance. Full CAD collision tests are separate.",
        "Reference heater is a23x16mm block fromz6.5to18mm; actual mount and heater clearances still need confirmation.",
        "Block dimensions do not establish stock availability or magnetization direction. Stacked cubes require aligned magnetization and a seam/tolerance check."]}
    for label, kind, width, height, depth, z, gap, flats in CASES:
        if args.only and label not in args.only:
            continue
        name = args.prefix + "-" + label
        project = root / "cases" / (name + ".mhd.json")
        out = root / name
        if project.exists() or out.exists():
            raise FileExistsError(name)
        command = [sys.executable, "scripts/make_v6_conduction.py", "--name", name,
                   "--magnet", kind, "--magnet-disc-diameter-mm", str(width),
                   "--magnet-block-width-mm", str(width), "--magnet-block-height-mm", str(height),
                   "--magnet-stack-mm", str(depth), "--magnet-z", str(z), "--pole-gap", str(gap),
                   "--magnet-subdivisions", str(args.subdivisions), "--contact-band", "tip",
                   "--tip-contact-width-mm", ".8", "--tip-contact-height-mm", ".4",
                   "--tip-contact-z-mm", ".3", "--tip-flat-width-mm", ".8",
                   "--tip-flat-height-mm", ".6", "--fill-height", "3.5"]
        if flats:
            command += ["--nozzle-flat-width-mm", "4"]
        with (root / (name + "-generate.log")).open("x") as log:
            subprocess.run(command, check=True, stdout=log, stderr=subprocess.STDOUT)
        parts, _ = load(name)
        magnets = [(n, f, s) for n, f, s in parts if n.startswith("Bias magnet")]
        obstacles = [(n, s) for n, _, s in parts if not n.startswith("Bias magnet")]
        collisions = [{"magnet": n, "body": other, "volume_mm3": (solid ^ obstacle).volume()}
                      for n, _, solid in magnets for other, obstacle in obstacles]
        collisions = [c for c in collisions if c["volume_mm3"] > 1e-6]
        heater_overlap = sum((solid ^ reference_heater).volume() for _, _, solid in magnets)
        bottom = min(float(f[..., 2].min()) for _, f, _ in magnets)
        row = {"label": label, "project": str(project), "kind": kind,
               "dimensions_xyz_mm": [width, depth, height], "centre_z_mm": z,
               "gap_mm": gap, "collar_clearance_flats_mm": 4 if flats else None,
               "projection_below_exit_mm": max(0., -bottom), "collisions": collisions,
               "reference_heater_overlap_mm3": heater_overlap}
        if collisions:
            row["status"] = "CAD collision; field run skipped"
        else:
            data = json.loads(project.read_text())
            data["domain"] = {"min_mm": [-.4, -.4, 0.], "max_mm": [.4, .4, .6], "cell_mm": .05}
            data["run"].update(fluid_enabled=False, thermal_enabled=False, dt_s=1e-6, prescribed_jet=None)
            data["fluid"]["surface"] = {"enabled": False}
            project.write_text(json.dumps(data, indent=2) + "\n")
            with (root / (name + ".log")).open("x") as log:
                subprocess.run(["target/debug/examples/applied_probe", str(project), str(out)],
                               check=True, stdout=log, stderr=subprocess.STDOUT)
            meta = json.loads((out / "report.json").read_text())
            ids = np.fromfile(out / "material-ids.bin", dtype="<u4")
            field = np.fromfile(out / "em.bin", dtype="<f4").reshape(-1, 16)
            selected = ids == 4
            b = field[selected, :3].astype(float)
            row.update(status="complete bias-field screen", liquid_cells=int(selected.sum()),
                       mean_B_T=b.mean(axis=0).tolist(), rms_B_T=float(np.sqrt(np.mean(np.sum(b*b, axis=1)))),
                       min_By_T=float(b[:, 1].min()), max_By_T=float(b[:, 1].max()), grid=meta["dims"],
                       maximum_B_minus_applied_T=float(np.max(abs(field[:, :3] - field[:, 12:15]))))
        result["cases"].append(row)
        output.write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(row), flush=True)


if __name__ == "__main__":
    main()
