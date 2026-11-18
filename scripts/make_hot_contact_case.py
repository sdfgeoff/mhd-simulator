"""Hot contact fingers anchored to heater through off-the-shelf mica sheet.

50mm x1mm² straight copper supply extensions are a thermal test fixture,
not a prescription for electrical cable routing. Fixed-k, ideal-contact study.
"""

import argparse, copy, json
from pathlib import Path
from draw_hardware_layouts import load
from make_conduction_hardware import box, triangles
from make_hardware import emit, heater, orient
from make_v6_conduction import as_solid

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("output", type=Path)
p.add_argument("--anchor-width-mm", type=float, default=5.0)
a = p.parse_args()
if not 5 <= a.anchor_width_mm <= 14:
    p.error("Anchor width must be5..14mm")
a.output.mkdir(parents=True, exist_ok=False)
parts, _ = load("m6-stub-drilled-v1/stub4")
out = []
for i, (name, faces, solid) in enumerate(parts):
    if i in [2, 3]:
        sign = -1 if i == 2 else 1
        # Folded contact finger and a 6x5x0.4mm thermal anchor plate.
        solid = solid + box([1.0, 0.7, 3.3], [sign * 4.5, 0.0, 1.95])
        solid = solid + box([6.0, a.anchor_width_mm, 0.4], [sign * 7.0, 0.0, 3.7])
        solid = solid + box([50.0, 2.5, 0.4], [sign * 35.0, 0.0, 3.7])
        faces = orient(
            [
                tuple(tuple(float(v) for v in x) for x in t)
                for t in triangles(solid.set_tolerance(1e-5).simplify())
            ]
        )
    faces = [tuple(tuple(float(v) for v in x) for x in t) for t in faces]
    out.append((name, faces, solid))
for sign in [-1, 1]:
    solid = box([6.0, a.anchor_width_mm, 0.1], [sign * 7.0, 0.0, 3.95])
    out.append(
        (
            "Mica electrical insulator " + str(sign),
            orient(
                [tuple(tuple(float(v) for v in x) for x in t) for t in triangles(solid)]
            ),
            solid,
        )
    )
solid = as_solid(heater(4.0, 15.5))
out.append(("Heated aluminium block", heater(4.0, 15.5), solid))
for i, (name, _, solid) in enumerate(out):
    for other, _, s in out[i + 1 :]:
        overlap = (solid ^ s).volume()
        if overlap > 1e-5:
            raise ValueError(f"CAD overlap {name}/{other}: {overlap}")
path = a.output / "hot-contact.gltf"
emit(path, [(n, f) for n, f, _ in out], None)
project = json.loads(
    Path(
        "artifacts/hardware-research/cases/m6-stub-drilled-v1/stub4.mhd.json"
    ).read_text()
)
project["geometry"] = path.name
for body, (name, faces, _) in zip(project["bodies"], out):
    body["name"] = name + " / 0"
    body["triangles"] = len(faces)
mica = copy.deepcopy(next(m for m in project["materials"] if m["id"] == 7))
mica.update(
    id=19,
    name="Mica steady thermal screening",
    density_kg_m3=2900.0,
    notes="Fixed k0.528W/mK from Boyd mica-insulator table at25C; hot-property sensitivity and real contact resistance required. Cp880 retained as an unused screening placeholder; no transient material qualification.",
    reference="https://info.boydcorp.com/hubfs/Thermal/Air-Cooling/Boyd-Board-Level-Heatsinks-Catalog.pdf",
)
mica["thermal"]["conductivity_w_m_k"] = 0.528
project["materials"].append(mica)
for name, faces, _ in out[6:]:
    is_heater = "Heated" in name
    body = copy.deepcopy(project["bodies"][0])
    body.update(name=name + " / 0", triangles=len(faces))
    body["simulation"].update(
        material_id=8 if is_heater else 19,
        temperature_c=250.0,
        fixed_temperature_c=250.0 if is_heater else None,
    )
    project["bodies"].append(body)
half_y = max(3.8, a.anchor_width_mm / 2 + 0.5)
project["domain"].update(
    min_mm=[-60.5, -half_y, -0.2], max_mm=[60.5, half_y, 10.5], cell_mm=0.1
)
(a.output / "hot-contact.mhd.json").write_text(json.dumps(project, indent=2) + "\n")
(a.output / "manifest.json").write_text(
    json.dumps(
        {
            "status": "Generated full-CAD collision-checked thermal experiment; not qualified hardware",
            "project": str(a.output / "hot-contact.mhd.json"),
            "mica_each_mm": [6, a.anchor_width_mm, 0.1],
            "anchor_copper_each_mm": [6, a.anchor_width_mm, 0.4],
            "supply_extension_each_mm": [50, 2.5, 0.4],
            "nominal_supply_extension_area_mm2": 1.0,
            "heater_temperature_C": 250,
            "outer_supply_temperature_C": "imposed by native scalar rescaling",
            "scope": "Allheatercellsfixedreservoir. Thermal domain crops fixed heater but retains all unknown steel/liquid/copper paths. Real insulation/clamp contact resistance, current Joule heat, feed and air losses absent. No physical mounting fasteners modeled.",
        },
        indent=2,
    )
    + "\n"
)
print(a.output / "hot-contact.mhd.json")
