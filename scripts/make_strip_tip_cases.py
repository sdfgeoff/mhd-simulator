"""Matched replaceable-strip electrical screens; seal and clamp are not qualified."""

import argparse
import copy
import json
from pathlib import Path
from draw_hardware_layouts import load
from make_conduction_hardware import box, cylinder, triangles
from make_hardware import emit, orient

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--output",
    type=Path,
    default=Path("artifacts/hardware-research/cases/m6-strip-tip-v1"),
)
parser.add_argument("--thickness-mm", type=float, default=0.5)
args = parser.parse_args()
if args.thickness_mm not in [0.1, 0.2, 0.5]:
    parser.error("Supported thicknesses: .1, .2, .5mm")
t = args.thickness_mm
root = args.output
root.mkdir(exist_ok=False)
source = Path("artifacts/hardware-research/cases/m6-rooted08-v1/stub4.mhd.json")
base = json.loads(source.read_text())
parts, _ = load("m6-rooted08-v1/stub4")
rows = []
for width in [0.8, 2.8]:
    for material in ["steel", "copper"]:
        label = (
            material
            + str(width).replace(".", "")
            + ("t" + str(t).replace(".", "") if t != 0.5 else "")
        )
        # Existing raised pool, bore and magnets. Remove integral lower boss;
        # a 0.1mm insulating separation is an electrical assumption, not a seal.
        solids = [
            parts[0][2] - box([40, 40, 10 + t + 0.1], [0, 0, (t + 0.1 - 10) / 2]),
            parts[1][2],
        ]
        names = ["Stainless upper nozzle", "Molten solder"]
        mids = [14, 4]
        for sign, mid in [(-1, 16), (1, 17)]:
            solids.append(box([4.6, width, t], [sign * 2.7, 0, t / 2]))
            names.append(
                "Positive copper terminal" if sign < 0 else "Negative copper terminal"
            )
            mids.append(mid)
        solids.extend([parts[4][2], parts[5][2]])
        names.extend([parts[4][0], parts[5][0]])
        mids.extend([18, 18])
        solids.append(
            box([0.8, width, t], [0, 0, t / 2]) - cylinder(0.2, 0.7, [0, 0, 0.25])
        )
        names.append(material.title() + " replaceable orifice strip")
        mids.append(14 if material == "steel" else 20)
        for i, a in enumerate(solids):
            if str(a.status()) != "Error.NoError":
                raise ValueError("Topology")
            for b in solids[i + 1 :]:
                if (a ^ b).volume() > 1e-5:
                    raise ValueError("Overlap")
        meshes = [
            (name, orient(triangles(s.set_tolerance(1e-5).simplify())))
            for name, s in zip(names, solids)
        ]
        emit(root / (label + ".gltf"), meshes, None)
        project = copy.deepcopy(base)
        project["geometry"] = label + ".gltf"
        template = copy.deepcopy(project["bodies"][0])
        project["bodies"] = []
        for (name, faces), mid in zip(meshes, mids):
            body = copy.deepcopy(template)
            body.update(name=name + " / 0", triangles=len(faces))
            body["simulation"]["material_id"] = mid
            project["bodies"].append(body)
        copper = copy.deepcopy(next(m for m in project["materials"] if m["id"] == 16))
        copper.update(id=20, name="Continuous copper strip centre")
        project["materials"].append(copper)
        project["domain"].update(
            min_mm=[-3.8, -3.8, -4.0], max_mm=[3.8, 3.8, 10.5], cell_mm=0.05
        )
        for magnet in project["electromagnetics"]["magnets"]:
            magnet["subdivisions"] = 3
        path = root / (label + ".mhd.json")
        path.write_text(json.dumps(project, indent=2) + "\n")
        rows.append(
            {
                "label": label,
                "project": str(path),
                "strip_width_mm": width,
                "strip_thickness_mm": t,
                "bore_mm": 0.4,
                "insulating_separation_mm": 0.1,
                "strip_material": material,
            }
        )
(root / "manifest.json").write_text(
    json.dumps(
        {
            "cases": rows,
            "scope": "Cold fixed-property current-sharing comparison. Whole strip represented by three touching material regions to supply terminals. Ideal joints, no gasket/clamp/seal qualification, hot properties, motion or lifetime. Existing liquid geometry retained, including 0.1mm neck between strip and upper steel.",
        },
        indent=2,
    )
    + "\n"
)
print(json.dumps(rows, indent=2))
