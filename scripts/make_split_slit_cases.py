"""Split electrode plates under a straight tube, following the board slit sketch.

Cold comparison inputs only. No seal, clamp, wetting, thermal or jetting result.
"""

import argparse, copy, json
from pathlib import Path
from draw_hardware_layouts import load
from make_conduction_hardware import box, cylinder, triangles
from make_hardware import emit, orient

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("output", type=Path)
p.add_argument("--gap-mm", type=float, default=0.2)
p.add_argument("--tube-top-mm", type=float, default=10.0)
p.add_argument("--reservoir-id-mm", type=float, default=1.0)
p.add_argument("--pool-top-mm", type=float, default=6.0)
a = p.parse_args()
if not 0.15 <= a.gap_mm <= 0.4:
    p.error("Gap must be0.15..0.4mm")
if not 1.5 <= a.tube_top_mm <= 10 or not 0.5 < a.pool_top_mm <= 9:
    p.error("Tube top must be1.5..10mm and pool top0.5..9mm")
if not 1 <= a.reservoir_id_mm <= 3.2:
    p.error("Reservoir ID must be1..3.2mm")
a.output.mkdir(exist_ok=False)
source = Path("hardware/designs/rooted08-hot-contact-m6/hot-contact.mhd.json")
base = json.loads(source.read_text())
original, _ = load(str(source.with_suffix("").with_suffix(".gltf")))
# Metal M6 holder, through-bored 2mm. Tube projects0.5mm below its metal sleeve.
holder_blank = cylinder(3, 6, [0, 0, 7]) + box(
    [6, max(2.8, a.reservoir_id_mm + 0.6), 3], [0, 0, 2.5]
)
if a.tube_top_mm == 10 and a.reservoir_id_mm == 1:
    holder = holder_blank - cylinder(1, 10, [0, 0, 5.5])
else:
    holder = holder_blank - cylinder(
        1, a.tube_top_mm - 0.5, [0, 0, (a.tube_top_mm + 0.5) / 2]
    )
    holder = holder - cylinder(
        a.reservoir_id_mm / 2, 10.5 - a.tube_top_mm, [0, 0, (10.5 + a.tube_top_mm) / 2]
    )
tube_length = a.tube_top_mm - 0.5
tube_centre = [0, 0, (a.tube_top_mm + 0.5) / 2]
tube = cylinder(1, tube_length, tube_centre) - cylinder(
    0.5, tube_length + 0.5, tube_centre
)
# Same bore above the plates; below the tube the opening is the bore/slot intersection.
neck_top = min(a.pool_top_mm, a.tube_top_mm)
upper = cylinder(0.5, neck_top - 0.5, [0, 0, (neck_top + 0.5) / 2])
if a.pool_top_mm > a.tube_top_mm:
    upper = upper + cylinder(
        a.reservoir_id_mm / 2,
        a.pool_top_mm - a.tube_top_mm,
        [0, 0, (a.pool_top_mm + a.tube_top_mm) / 2],
    )
lower = cylinder(0.5, 0.5, [0, 0, 0.25]) ^ box([a.gap_mm, 2, 0.5], [0, 0, 0.25])
liquid = upper + lower
rows = []
for label, tube_mid, bridge in [
    ("alumina", 7, False),
    ("steel", 14, False),
    ("steel_touch", 14, True),
    ("alumina_touch", 7, True),
]:
    solids = [holder, liquid]
    names = ["M6 metal tube holder", "Liquid solder"]
    mids = [14, 4]
    for sign, mid in [(-1, 16), (1, 17)]:
        plate = box(
            [6 - a.gap_mm / 2, 3, 0.5], [sign * (6 + a.gap_mm / 2) / 2, 0, 0.25]
        )
        plate = (
            plate
            + box([1, 0.7, 3.65], [sign * 4.5, 0, 2.075])
            + box([6, 10, 0.4], [sign * 7, 0, 3.7])
            + box([50, 2.5, 0.4], [sign * 35, 0, 3.7])
        )
        if bridge:
            plate = plate + box([1.95, 2.8, 0.5], [sign * 2.025, 0, 0.75])
        solids.append(plate)
        names.append(
            "Positive split copper plate" if sign < 0 else "Negative split copper plate"
        )
        mids.append(mid)
    for i in [4, 5]:
        solids.append(original[i][2])
        names.append(original[i][0])
        mids.append(18)
    solids.append(tube)
    names.append("Alumina chamber tube" if tube_mid == 7 else "Stainless chamber tube")
    mids.append(tube_mid)
    for i in [6, 7]:
        solids.append(original[i][2])
        names.append(original[i][0])
        mids.append(19)
    solids.append(original[8][2])
    names.append(original[8][0])
    mids.append(8)
    for i, s in enumerate(solids):
        if str(s.status()) != "Error.NoError" or len(s.decompose()) != 1:
            raise ValueError("Topology " + names[i])
        for j, t in enumerate(solids[i + 1 :], i + 1):
            overlap = (s ^ t).volume()
            if overlap > 1e-5:
                raise ValueError(f"Overlap {names[i]}/{names[j]}: {overlap}")
    meshes = []
    for name, s in zip(names, solids):
        try:
            faces = orient(triangles(s.set_tolerance(1e-5).simplify()))
        except AssertionError as e:
            raise ValueError("Export topology " + name) from e
        meshes.append((name, faces))
    emit(a.output / (label + ".gltf"), meshes, None)
    project = copy.deepcopy(base)
    project["geometry"] = label + ".gltf"
    template = copy.deepcopy(project["bodies"][0])
    project["bodies"] = []
    for (name, faces), mid in zip(meshes, mids):
        body = copy.deepcopy(template)
        body.update(name=name + " / 0", triangles=len(faces))
        body["simulation"].update(
            material_id=mid,
            temperature_c=250.0,
            fixed_temperature_c=250.0 if mid == 8 else None,
            included=True,
        )
        project["bodies"].append(body)
    project["domain"].update(
        min_mm=[-12.5, -3.2, -2.2], max_mm=[12.5, 3.2, 10.5], cell_mm=0.05
    )
    project["electromagnetics"]["magnets"] = []
    path = a.output / (label + "-charge.mhd.json")
    path.write_text(json.dumps(project, indent=2) + "\n")
    observer = copy.deepcopy(project)
    observer["domain"].update(min_mm=[-0.6, -0.6, 0], max_mm=[0.6, 0.6, 0.5])
    observer["electromagnetics"]["magnets"] = copy.deepcopy(
        base["electromagnetics"]["magnets"]
    )
    for magnet in observer["electromagnetics"]["magnets"]:
        magnet["subdivisions"] = 4
    (a.output / (label + "-observer.mhd.json")).write_text(
        json.dumps(observer, indent=2) + "\n"
    )
    coarse = copy.deepcopy(project)
    coarse["domain"].update(
        min_mm=[-12.5, -5.3, -2.2], max_mm=[12.5, 5.3, 10.5], cell_mm=0.1
    )
    (a.output / (label + "-charge-h010.mhd.json")).write_text(
        json.dumps(coarse, indent=2) + "\n"
    )
    rows.append(
        {
            "label": label,
            "project": str(path),
            "tube_material_id": tube_mid,
            "plate_to_holder_bridge": bridge,
            "slit_gap_mm": a.gap_mm,
            "slit_max_length_mm": 1.0,
            "plate_thickness_mm": 0.5,
            "tube_OD_ID_length_mm": [2, 1, tube_length],
            "tube_top_z_mm": a.tube_top_mm,
            "reservoir_id_mm": a.reservoir_id_mm,
            "pool_top_z_mm": a.pool_top_mm,
            "liquid_volume_mm3": liquid.volume(),
            "nominal_exit_area_mm2": lower.slice(0.25).area(),
            "scope": "Tube contacts plates directly; normal case has0.5mm clearance between metal holder and plates. Touch case bridges that clearance. Alumina_touch deliberately tests a metal-holder shunt despite insulating chamber. Mechanical mounting/clamps absent; no sealing or current result yet.",
        }
    )
(a.output / "manifest.json").write_text(
    json.dumps(
        {
            "status": "Nominal split-slit comparison CAD; no physics qualification",
            "cases": rows,
            "source_sketch_attachment": "6a29e1d4-df86-44e8-a193-1bd473c76918",
            "tube_stock_reference": "https://almathcrucibles.com/product/alm21/",
            "stock_scope": "Manufacturer lists2mmOD/1mmID99.7%alumina tube; cut/lap short length, dimensions/fits and thermal contact not qualified.",
            "conductivity_scope": "Initial geometry cases use existing fixedCu25MS/m,SS1.1MS/m,liquid2.2MS/m andidealinterfaces. Aluminium bar conductivity needs a separately documented sensitivity; it is not silently assumed superior.",
        },
        indent=2,
    )
    + "\n"
)
print(json.dumps(rows, indent=2))
