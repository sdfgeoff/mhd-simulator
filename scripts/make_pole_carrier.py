"""Review CAD for independent aluminium pole carriers and positive end stops.

Nominal envelopes only: tapped holes are pilot/clearance cylinders, not threads.
Carriers are excluded from physics until actual thermal contacts are specified.
"""

import argparse, copy, json, struct, math
from pathlib import Path
from draw_hardware_layouts import load
from make_conduction_hardware import box, cylinder, triangles
from make_hardware import emit, orient

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("output", type=Path)
p.add_argument("--magnet-drop-mm", type=float, default=0.2)
p.add_argument(
    "--source-project",
    type=Path,
    default=Path("hardware/designs/rooted08-hot-contact-m6/hot-contact.mhd.json"),
)
p.add_argument("--floor-inner-mm", type=float)
p.add_argument("--floor-end-chamfer-mm", type=float, default=0.0)
a = p.parse_args()
drop = a.magnet_drop_mm
if not 0 <= drop <= 1:
    p.error("Drop must be0..1mm")
if not math.isfinite(a.floor_end_chamfer_mm) or not 0 <= a.floor_end_chamfer_mm <= 0.2:
    p.error("Floor end chamfer must be0..0.2mm")
a.output.mkdir(parents=True, exist_ok=False)
project = json.loads(a.source_project.read_text())
base, _ = load(str(a.source_project.parent / project["geometry"]))
for i in [4, 5]:
    name, faces, solid = base[i]
    faces = [
        tuple(
            tuple(
                float(v) - (drop if axis == 2 else 0.0) for axis, v in enumerate(point)
            )
            for point in triangle
        )
        for triangle in faces
    ]
    base[i] = (name, faces, solid.translate([0, 0, -drop]))
# The source may already have wider or lowered poles. Round imported f32
# coordinates far below machining tolerances before constructing touching faces.
positive_pole = [point for tri in base[5][1] for point in tri]
y_offset = round(min(pt[1] for pt in positive_pole) - 1.7, 6)
carrier_drop = round(-min(pt[2] for pt in positive_pole) - 2.0, 6)
outlet_z = round(float(min(pt[2] for tri in base[1][1] for pt in tri)), 6)
floor_inner = 1.18 + y_offset if a.floor_inner_mm is None else a.floor_inner_mm
if not 0 < floor_inner < 12.32 + y_offset:
    p.error("Floor inner edge must lie inside the outer floor edge")
new = []
for sy in [-1, 1]:
    # Floor below the bars, back wall, and end walls with inward locating lips.
    floor_inner_unshifted = floor_inner - y_offset
    body = box(
        [35.4, 12.32 - floor_inner_unshifted, 0.5],
        [0, sy * (12.32 + floor_inner_unshifted) / 2, -2.25 - carrier_drop],
    )
    body = body + box([35.4, 1.5, 5.7], [0, sy * 13.05, 0.35 - carrier_drop])
    for sx in [-1, 1]:
        body = body + box([5.0, 10.6, 5.7], [sx * 15.2, sy * 7.0, 0.35 - carrier_drop])
        body = body + box([7.2, 0.5, 5.7], [sx * 14.1, sy * 1.45, 0.35 - carrier_drop])
        body = body + box(
            [5.0, 0.55, 5.7], [sx * 15.2, sy * 1.475, 0.35 - carrier_drop]
        )
        body = body + box([5.0, 4.0, 21.2], [sx * 14.5, sy * 14.3, 10.6])
        body = body + box([8.0, 8.0, 3.0], [sx * 14.5, sy * 14.3, 19.7])
        # M3 tapped-envelope pilot at the cold mounting tab.
        body = body - cylinder(1.25, 6.2, [sx * 14.5, sy * 14.3, 18.2])
        for y in [4.2, 9.2]:
            body = body - cylinder(0.8, 4.0, [sx * 15.2, sy * y, 1.4 - carrier_drop])
    if a.floor_end_chamfer_mm:
        offset = (-17.7 - 2.5 - carrier_drop + a.floor_end_chamfer_mm) / math.sqrt(2)
        for sx in [-1, 1]:
            body = body.trim_by_plane([-sx, 0, 1], offset)
    new.append(
        (f"Aluminium pole carrier {sy}", body.translate([0, sy * y_offset, 0]), 8)
    )
    for sx in [-1, 1]:
        # Keepers are above the bars with0.2mm lift clearance; their inner edge
        # clears the 2.5mm-wide supply foil by0.25mm in Y.
        keeper = box([7.2, 10.35, 0.5], [sx * 14.1, sy * 6.925, 3.45 - carrier_drop])
        for y in [4.2, 9.2]:
            keeper = keeper - cylinder(
                1.1, 1.0, [sx * 15.2, sy * y, 3.45 - carrier_drop]
            )
        new.append(
            (f"Aluminium keeper {sy} {sx}", keeper.translate([0, sy * y_offset, 0]), 8)
        )
        for y in [4.2, 9.2]:
            screw = cylinder(
                1.0, 4.0, [sx * 15.2, sy * y, 1.7 - carrier_drop]
            ) + cylinder(1.9, 2.0, [sx * 15.2, sy * y, 4.7 - carrier_drop])
            new.append(
                (
                    f"M2 keeper screw envelope {sy} {sx} {y}",
                    screw.translate([0, sy * y_offset, 0]),
                    6,
                )
            )
# Model screws pass through pilot bores as if threaded. Their radial engagement
# is intentional and listed separately from forbidden solid-body interference.
checks = []
for name, solid, mid in new:
    for other, _, s in base:
        overlap = (solid ^ s).volume()
        if overlap > 1e-5:
            raise ValueError(f"Interference {name}/{other}: {overlap}")
    checks.append({"part": name, "base_overlap_mm3": 0.0})
for i, (name, solid, mid) in enumerate(new):
    for other, s, _ in new[i + 1 :]:
        overlap = (solid ^ s).volume()
        if overlap > 1e-5:
            intended = ("screw" in name and "carrier" in other) or (
                "carrier" in name and "screw" in other
            )
            if not intended:
                raise ValueError(f"Carrier interference {name}/{other}: {overlap}")
            checks.append(
                {
                    "parts": [name, other],
                    "intentional_pilot_thread_engagement_mm3": overlap,
                }
            )
# Central heatbreak/filament keepout is deliberately clear above the nozzle.
keepout = cylinder(3.2, 20.0, [0, 0, 20.0])
for name, s, _ in new:
    if (s ^ keepout).volume() > 1e-5:
        raise ValueError("Central heatbreak keepout " + name)
upper_keepout = cylinder(14.0, 24.0, [0, 0, 28.0])
for name, s, _ in new:
    if (s ^ upper_keepout).volume() > 1e-5:
        raise ValueError("Upper28mm keepout " + name)
parts = [
    (name, [tuple(tuple(float(v) for v in x) for x in t) for t in f])
    for name, f, _ in base
]
for name, s, _ in new:
    try:
        faces = orient(triangles(s.set_tolerance(1e-5).simplify()))
    except AssertionError:
        print("Mesh validation failed:", name, s.status())
        raise
    parts.append((name, faces))
emit(a.output / "assembly.gltf", parts, None)
emit(a.output / "carrier-only.gltf", parts[len(base) :], None)
project["geometry"] = "assembly.gltf"
for (name, solid, mid), (_, faces) in zip(new, parts[len(base) :]):
    body = copy.deepcopy(project["bodies"][0])
    body.update(name=name + " / 0", triangles=len(faces))
    body["simulation"].update(
        included=False, material_id=mid, temperature_c=25.0, fixed_temperature_c=None
    )
    project["bodies"].append(body)
(a.output / "assembly.inspection.mhd.json").write_text(
    json.dumps(project, indent=2) + "\n"
)
# Individual closed STLs use millimetres; for review/remodelling, not shop release.
for i, (name, faces) in enumerate(parts[len(base) :]):
    if "screw" in name:
        continue
    with (a.output / f"part-{i:02}.stl").open("wb") as f:
        f.write(
            b"MHD carrier research CAD; millimetres; not shop release".ljust(80, b" ")
        )
        f.write(struct.pack("<I", len(faces)))
        for triangle in faces:
            u = [triangle[1][i] - triangle[0][i] for i in range(3)]
            v = [triangle[2][i] - triangle[0][i] for i in range(3)]
            n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ]
            length = math.sqrt(sum(x * x for x in n))
            n = [x / length for x in n]
            f.write(struct.pack("<12fH", *n, *(v for pt in triangle for v in pt), 0))
mins = [float("inf")] * 3
maxs = [-float("inf")] * 3
for _, faces in parts[len(base) :]:
    for tri in faces:
        for pt in tri:
            for axis in range(3):
                mins[axis] = min(mins[axis], pt[axis])
                maxs[axis] = max(maxs[axis], pt[axis])
record = {
    "status": "Nominal carrier review CAD; not a manufacturing or physics qualification",
    "source_case": str(a.source_project.parent),
    "source_project": str(a.source_project),
    "base_body_count": len(base),
    "carrier_y_offset_mm": y_offset,
    "carrier_placement_drop_mm": carrier_drop,
    "outlet_z_mm": outlet_z,
    "floor_end_chamfer_mm": a.floor_end_chamfer_mm,
    "parts": len(new),
    "new_part_bounds_mm": {
        "min": mins,
        "max": maxs,
        "size": [b - a for a, b in zip(mins, maxs)],
    },
    "modeled_standoff_below_outlet_mm": outlet_z - mins[2],
    "magnet_only_projection_mm": round(outlet_z + 2 + carrier_drop, 6),
    "magnet_drop_mm": drop,
    "floor_thickness_mm": 0.5,
    "nominal_keeper_to_copper_Y_clearance_mm": 0.5 + y_offset,
    "nominal_keeper_to_copper_X_clearance_mm": 0.5,
    "nominal_keeper_to_heater_Z_clearance_mm": 0.3 + carrier_drop,
    "nominal_magnet_lift_clearance_mm": 0.2,
    "carrier_arm_section_mm": [5.0, 4.0],
    "unsupported_length_mm": 15.0,
    "mount_pilot_diameter_mm": 2.5,
    "mount_pilot_depth_mm": 6.0,
    "keeper_pilot_diameter_mm": 1.6,
    "keeper_screw_nominal": "M2x4 shaft/head envelopes; threads/socket omitted",
    "central_heatbreak_keepout_diameter_mm": 6.4,
    "checks": checks,
    "limits": [
        "New carrier/keeper/screw bodies excluded from simulation; old electrical/thermal fields do not qualify their heat paths, eddy losses or contact bridges.",
        "Nominal clearance only; tolerances, corner reliefs, real thread roots, preload, contact stresses and thermal distortion absent.",
        "Monolithic CAD may be split into tray/end supports for machining; no fabrication process or cold-carriage interface is released.",
        "Cold-carriage mounting screws and machine structure are absent. Actual lowest assembly feature and clearance margin must be verified.",
    ],
}
record["part_files"] = [
    {
        "index": i,
        "name": name,
        "material_id": mid,
        "stl": None if "screw" in name else f"part-{i:02}.stl",
        "volume_mm3": solid.volume(),
    }
    for i, (name, solid, mid) in enumerate(new)
]
record["floor_central_opening_mm"] = 2 * floor_inner
record["upper_keepout_mm"] = {
    "diameter": 28.0,
    "z_min": 16.0,
    "z_max": 40.0,
    "scope": "Abstract clear envelope; actual heatsink/carriage interface not supplied.",
}
record["keeper_screw_head_mm"] = {"diameter": 3.8, "height": 2.0}
record["screw_dimension_reference"] = (
    "https://www.annapol.eu/documents/6ee345428db47b168b9ffe8bc9f44571/BN610-DE.pdf"
)
(a.output / "manifest.json").write_text(json.dumps(record, indent=2) + "\n")
print(
    json.dumps(
        {
            k: record[k]
            for k in ["parts", "new_part_bounds_mm", "modeled_standoff_below_outlet_mm"]
        },
        indent=2,
    )
)
