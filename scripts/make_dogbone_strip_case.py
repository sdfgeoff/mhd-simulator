"""Full-length replaceable stainless strip and bolted copper contacts for review."""

import argparse, copy, json, math
from pathlib import Path
import manifold3d as cad
from draw_hardware_layouts import load
from make_conduction_hardware import box, cylinder, triangles
from make_hardware import emit, orient

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
parser.add_argument("--waist-half-length-mm", type=float, default=1.0)
args = parser.parse_args()
args.output.mkdir(exist_ok=False)
w = args.waist_half_length_mm
if not 0.4 <= w <= 1.0:
    parser.error("Waist half length .4..1mm")
source = Path("hardware/designs/rooted08-hot-contact-m6/hot-contact.mhd.json")
project = json.loads(source.read_text())
parts, _ = load(str(source.with_suffix("").with_suffix(".gltf")))
# 16mm overall strip, 0.8mm waist, 4mm end tabs, 0.5mm sheet.
outline = [
    (-8, -2),
    (-w - 1, -2),
    (-w, -0.4),
    (w, -0.4),
    (w + 1, -2),
    (8, -2),
    (8, 2),
    (w + 1, 2),
    (w, 0.4),
    (-w, 0.4),
    (-w - 1, 2),
    (-8, 2),
]
strip = cad.CrossSection([outline]).extrude(0.5) - cylinder(0.2, 0.7, [0, 0, 0.25])
for sign in [-1, 1]:
    strip = strip - cylinder(1.1, 0.7, [sign * 5.5, 0, 0.25])
shapes = [parts[0][2] - box([40, 40, 10.5], [0, 0, -4.75]), parts[1][2]]
names = ["Upper stainless nozzle with flat strip seat", "Liquid solder"]
mids = [14, 4]
for sign, mid in [(-1, 16), (1, 17)]:
    copper = (
        box([5.8, 4, 0.5], [sign * 6.1, 0, 0.75])
        + box([1, 0.7, 3.4], [sign * 8.5, 0, 2.2])
        + box([6, 10, 0.4], [sign * 7, 0, 3.7])
        + box([50, 2.5, 0.4], [sign * 35, 0, 3.7])
    )
    copper = copper - cylinder(1.1, 1, [sign * 5.5, 0, 0.75])
    shapes.append(copper)
    names.append("Positive copper contact" if sign < 0 else "Negative copper contact")
    mids.append(mid)
for i, sign in [(4, -1), (5, 1)]:
    shapes.append(parts[i][2].translate([0, sign * 0.6, -0.2]))
    names.append(parts[i][0])
    mids.append(18)
for i in [6, 7, 8]:
    shapes.append(parts[i][2])
    names.append(parts[i][0])
    mids.append(19 if i < 8 else 8)
shapes.append(strip)
names.append("Replaceable stainless dogbone strip")
mids.append(14)
for sign in [-1, 1]:
    # M2x3: 3.8x2mm head below strip; 4mm-AF x1.6mm nut above Cu.
    head = cylinder(1.9, 2, [sign * 5.5, 0, -1])
    shaft = cylinder(1, 3, [sign * 5.5, 0, 1.5])
    radius = 4 / math.sqrt(3)
    nut = (
        cad.CrossSection(
            [
                [
                    (
                        radius * math.cos(i * math.pi / 3),
                        radius * math.sin(i * math.pi / 3),
                    )
                    for i in range(6)
                ]
            ]
        )
        .extrude(1.6)
        .translate([sign * 5.5, 0, 1])
    )
    fastener = head + shaft + nut
    shapes.append(fastener)
    names.append("M2x3 screw and nut envelope " + str(sign))
    mids.append(14)
checks = []
for i, a in enumerate(shapes):
    if str(a.status()) != "Error.NoError" or len(a.decompose()) != 1:
        raise ValueError("Topology " + names[i])
    for j, b in enumerate(shapes[i + 1 :], i + 1):
        overlap = (a ^ b).volume()
        if overlap > 1e-5:
            raise ValueError(f"Overlap {names[i]} / {names[j]} {overlap}")
meshes = [
    (name, orient(triangles(s.set_tolerance(1e-5).simplify())))
    for name, s in zip(names, shapes)
]
emit(args.output / "assembly.gltf", meshes, None)
project["geometry"] = "assembly.gltf"
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
for magnet in project["electromagnetics"]["magnets"]:
    magnet["subdivisions"] = 4
project["domain"].update(
    min_mm=[-60.5, -5.5, -2.2], max_mm=[60.5, 5.5, 10.5], cell_mm=0.1
)
(args.output / "assembly.mhd.json").write_text(json.dumps(project, indent=2) + "\n")
charge = copy.deepcopy(project)
charge["electromagnetics"]["magnets"] = []
charge["domain"].update(min_mm=[-10, -2.3, -2.2], max_mm=[10, 2.3, 10.5], cell_mm=0.05)
(args.output / "charge.mhd.json").write_text(json.dumps(charge, indent=2) + "\n")
observer = copy.deepcopy(project)
observer["domain"].update(min_mm=[-0.3, -0.3, 0], max_mm=[0.3, 0.3, 0.6], cell_mm=0.05)
(args.output / "observer.mhd.json").write_text(json.dumps(observer, indent=2) + "\n")
thermal = copy.deepcopy(project)
thermal["domain"].update(min_mm=[-60.5, -5.4, -2.2], max_mm=[60.5, 5.4, 10.5])
(args.output / "thermal-h010.mhd.json").write_text(json.dumps(thermal, indent=2) + "\n")
coarse = copy.deepcopy(charge)
coarse["domain"]["cell_mm"] = 0.1
(args.output / "charge-h010.mhd.json").write_text(json.dumps(coarse, indent=2) + "\n")
coarse = copy.deepcopy(observer)
coarse["domain"]["cell_mm"] = 0.1
(args.output / "observer-h010.mhd.json").write_text(json.dumps(coarse, indent=2) + "\n")
faces = np.asarray(meshes[9][1], dtype=float)
with (args.output / "strip-mm.stl").open("wb") as out:
    out.write(b"Review SS strip; millimetres, not shop release".ljust(80, b" "))
    out.write(struct.pack("<I", len(faces)))
    for tri in faces:
        normal = np.cross(tri[1] - tri[0], tri[2] - tri[0])
        normal /= np.linalg.norm(normal)
        out.write(struct.pack("<12fH", *normal, *tri.reshape(-1), 0))
(args.output / "manifest.json").write_text(
    json.dumps(
        {
            "status": "Nominal closed collision-checked geometry, not seal or mounting qualification",
            "source": str(source),
            "strip_length_mm": 16,
            "waist_length_mm": 2 * w,
            "waist_width_mm": 0.8,
            "end_tab_width_mm": 4,
            "thickness_mm": 0.5,
            "bore_mm": 0.4,
            "contact_screw_positions_x_mm": [-5.5, 5.5],
            "pole_gap_mm": 4.6,
            "screw_head_projection_mm": 2,
            "magnet_projection_mm": 2.2,
            "contact_to_pole_nominal_clearance_mm": 0.3,
            "scope": "Full-length SS strip and bolted Cu end contacts. Ideal flat direct-metal seat. Heater-anchor clamping and nozzle-seat preload are not modeled. No torque, sealing finish, tool access, thread form or thermal expansion qualification. Carrier geometry not included; previous carriers require repositioning for wider poles. Charge domain crops outer anchors/leads; its resistance omits those outer paths.",
            "dimension_sources": [
                "https://www.annapol.eu/documents/6ee345428db47b168b9ffe8bc9f44571/BN610-DE.pdf",
                "https://www.accu.co.uk/hexagon-nuts/7884-HPN-M2-A2",
            ],
        },
        indent=2,
    )
    + "\n"
)
print(args.output)
