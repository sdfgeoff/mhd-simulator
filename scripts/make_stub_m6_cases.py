"""Short M6 nozzles: retain heater interface, trade small magnet projection for a stout body."""

import argparse, copy, json, math
import manifold3d as cad
from pathlib import Path
from make_conduction_hardware import box, triangles
from make_hardware import emit, heater, lathe, orient
from make_v6_conduction import as_solid

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("output", type=Path)
p.add_argument("--cell-mm", type=float, default=0.05)
p.add_argument("--pole-gap-mm", type=float, default=3.0)
p.add_argument("--heater-clearance-mm", type=float, default=0.45)
p.add_argument("--only", choices=["stub3", "stub3p5", "stub4", "stub5"])
p.add_argument("--lower-drill-half-angle-deg", type=float, default=45.0)
p.add_argument("--boss-width-mm", type=float, default=0.7)
p.add_argument("--tip-root-radius-mm", type=float, default=0.0)
p.add_argument("--contact-height-mm", type=float, default=0.4)
p.add_argument("--chamber-bore-mm", type=float, help="Straight chamber diameter; omit to reproduce the historical stepped bore")
p.add_argument("--neck-flat-halfwidth-mm", type=float, default=1.4)
p.add_argument("--outlet-land-mm", type=float, default=0.6, help="Internal 0.4 mm bore length; exterior and contact heights stay fixed")
p.add_argument("--preserve-boolean-mesh",action="store_true",help="Skip optional mesh simplification for geometry diagnostics")
a = p.parse_args()
if not math.isfinite(a.outlet_land_mm) or not 0.6 <= a.outlet_land_mm <= 1.5:
    p.error("Outlet land must be0.6..1.5mm")
if not math.isfinite(a.neck_flat_halfwidth_mm) or not 1.4 <= a.neck_flat_halfwidth_mm <= 2.4:
    p.error("Neck halfwidth must be1.4..2.4mm")
if a.chamber_bore_mm is not None and (
    not math.isfinite(a.chamber_bore_mm) or not 1.5 <= a.chamber_bore_mm <= 3.2
    or a.chamber_bore_mm / 2 + .15 > a.neck_flat_halfwidth_mm
):
    p.error("Straight bore must be1.5..3.2mm and leave at least0.15mm side wall")
if a.pole_gap_mm / 2 < a.neck_flat_halfwidth_mm + .1:
    p.error("Magnets must clear neck flats by at least0.1mm per side")
if not 0.5 <= a.boss_width_mm <= 1.0:
    p.error("Boss width must be0.5..1mm")
if not 0 <= a.tip_root_radius_mm <= 0.2:
    p.error("Root radius must be0..0.2mm")
if (
    not 0.1 <= a.contact_height_mm <= 0.4
    or 0.1 + a.contact_height_mm > 0.6 - a.tip_root_radius_mm + 1e-9
):
    p.error("Contact must fit below root fillet")
if not math.isfinite(a.cell_mm) or a.cell_mm <= 0:
    p.error("Positive finite spacing required")
if not math.isfinite(a.pole_gap_mm) or a.pole_gap_mm < 2.8:
    p.error("Pole gap must clear the 2.8 mm body")
if not math.isfinite(a.heater_clearance_mm) or a.heater_clearance_mm < 0.3:
    p.error("Heater clearance must clear the shoulder")
if (
    not math.isfinite(a.lower_drill_half_angle_deg)
    or not 30 <= a.lower_drill_half_angle_deg <= 70
):
    p.error("Drill half-angle must be30..70deg")
a.output.mkdir(parents=True, exist_ok=False)
base = json.loads(Path("hardware/designs/slim-square07/case.mhd.json").read_text())
manifest = {
    "status": "Generated research geometry; no performance result",
    "source_interface": "https://robu-prod-media.s3.ap-south-1.amazonaws.com/uploads/2019/03/Schematic.pdf",
    "cases": [],
}


def mesh(s):
    return orient(
        [
            tuple(tuple(float(v) for v in x) for x in t)
            for t in triangles(s if a.preserve_boolean_mesh else s.set_tolerance(1e-5).simplify())
        ]
    )


for below in [3.0, 3.5, 4.0, 5.0]:
    name = f"stub{below:g}".replace(".", "p")
    if a.only and a.only != name:
        continue
    length = below + 6
    fill = below + 2
    radius, upper, ro = 1.0, 1.6, 0.2
    if a.chamber_bore_mm is not None:
        radius = upper = a.chamber_bore_mm / 2
    lower_transition = a.outlet_land_mm + (radius - ro) / math.tan(
        math.radians(a.lower_drill_half_angle_deg)
    )
    transition = below - (upper - radius) / math.tan(math.radians(59))
    mouth_depth = 0.35 / math.tan(math.radians(30))
    profile = [
        (2.4, length, False),
        (3.0, length - 0.6, False),
        (3.0, below, False),
        (2.5, below, False),
        (2.5, below - 0.3, False),
        (3.5, below - 0.3, True),
        (3.5, 0.8, True),
        (0.5, 0.0, False),
        (ro, 0.0, False),
        (ro, a.outlet_land_mm, False),
        (radius, lower_transition, False),
        (radius, transition, False),
        (upper, below, False),
        (upper, length - mouth_depth, False),
        (1.95, length, False),
    ]
    solid = as_solid(lathe(profile))
    for sign in [-1, 1]:
        solid = solid - box(
            [20.0, 20.0, below - 0.23], [0.0, sign * (10 + a.neck_flat_halfwidth_mm), (below - 0.25 - 0.02) / 2]
        )
        solid = solid - box(
            [20.0, 20.0, below - 0.28], [sign * 13.0, 0.0, (below - 0.3 - 0.02) / 2]
        )
        if a.tip_root_radius_mm == 0:
            solid = solid - box(
                [20.0, 20.0, 0.62], [sign * (10 + a.boss_width_mm / 2), 0.0, 0.29]
            )
            solid = solid - box(
                [20.0, 20.0, 0.62], [0.0, sign * (10 + a.boss_width_mm / 2), 0.29]
            )
    if a.tip_root_radius_mm > 0:
        r = a.tip_root_radius_mm
        half = a.boss_width_mm / 2
        poly = [(half, -0.02), (10.0, -0.02), (10.0, 0.6)]
        poly += [
            (half + r + r * math.cos(t), 0.6 - r + r * math.sin(t))
            for t in [math.pi / 2 + i * math.pi / 32 for i in range(17)]
        ]
        cutter = (
            cad.CrossSection([poly])
            .extrude(20.0)
            .transform([[1, 0, 0, 0], [0, 0, 1, -10], [0, 1, 0, 0]])
        )
        for angle in [0, 90, 180, 270]:
            solid = solid - cutter.rotate([0, 0, angle])
    pool_profile = [
        (0.0, fill, False),
        (upper, fill, False),
        (upper, below, False),
        (radius, transition, False),
        (radius, lower_transition, False),
        (ro, a.outlet_land_mm, False),
        (ro, 0.0, False),
        (0.0, 0.0, False),
    ]
    pool_faces = lathe(list(reversed(pool_profile)))
    pool = as_solid(pool_faces)
    if (solid ^ pool).volume() > 1e-6:
        raise ValueError("Liquid overlap " + name)
    parts = [("Short M6 nozzle " + name, mesh(solid)), ("Liquid solder", pool_faces)]
    contacts = []
    for i, sign in enumerate([-1, 1]):
        copper = box(
            [5 - a.boss_width_mm / 2, a.boss_width_mm, a.contact_height_mm],
            [sign * (5 + a.boss_width_mm / 2) / 2, 0.0, 0.1 + a.contact_height_mm / 2],
        )
        contacts.append(copper)
        if (solid ^ copper).volume() > 1e-6 or (pool ^ copper).volume() > 1e-6:
            raise ValueError("Contact collision")
        parts.append(
            (("Positive" if i == 0 else "Negative") + " copper contact", mesh(copper))
        )
    magnets = []
    top = below - a.heater_clearance_mm
    for i, sign in enumerate([-1, 1]):
        magnet = box(
            [25.0, 10.0, 5.0], [0.0, sign * (a.pole_gap_mm / 2 + 5), top - 2.5]
        )
        magnets.append(magnet)
        if (solid ^ magnet).volume() > 1e-6 or (pool ^ magnet).volume() > 1e-6:
            raise ValueError("Magnet collision " + name)
        parts.append((f"Bias magnet {i + 1}", mesh(magnet)))
    reference = as_solid(heater(below, below + 11.5))
    overlap = sum((s ^ reference).volume() for s in [solid, *contacts, *magnets])
    if overlap > 1e-6:
        raise ValueError("Heater collision " + name)
    path = a.output / (name + ".gltf")
    emit(path, parts, None)
    project = copy.deepcopy(base)
    project["geometry"] = path.name
    project["domain"].update(
        min_mm=[-3.8, -3.8, -4.0], max_mm=[3.8, 3.8, length + 0.5], cell_mm=a.cell_mm
    )
    for body, (label, faces) in zip(project["bodies"], parts):
        body["name"] = label + " / 0"
        body["triangles"] = len(faces)
    output = path.with_suffix(".mhd.json")
    output.write_text(json.dumps(project, indent=2) + "\n")
    manifest["cases"].append(
        {
            "label": name,
            "project": str(output),
            "heater_start_mm": below,
            "overall_length_mm": length,
            "thread_length_mm": 6,
            "thread": "M6x1 envelope; helix omitted",
            "chamber_diameter_mm": 2 * radius,
            "upper_chamber_diameter_mm": 2 * upper,
            "straight_chamber": a.chamber_bore_mm is not None,
            "lower_drill_point_included_angle_deg": 2 * a.lower_drill_half_angle_deg,
            "outlet_mm": 0.4,
            "outlet_land_mm": a.outlet_land_mm,
            "boss_width_mm": a.boss_width_mm,
            "tip_root_radius_mm": a.tip_root_radius_mm,
            "contact_height_mm": a.contact_height_mm,
            "nominal_tip_wall_mm": (a.boss_width_mm - 0.4) / 2,
            "pool_level_mm": fill,
            "waist_Y_mm": 2 * a.neck_flat_halfwidth_mm,
            "wrench_X_AF_mm": 6.0,
            "pole_gap_mm": a.pole_gap_mm,
            "heater_clearance_mm": a.heater_clearance_mm,
            "magnet_bottom_mm": top - 5.0,
            "magnet_projection_below_exit_mm": 5.0 - top,
            "reference_heater_overlap_mm3": overlap,
            "scope": "Shorter, wider lower body with raised melt pool. Cold field/thermal screening input; no fit, torque, machining or jetting qualification.",
        }
    )
(a.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(json.dumps(manifest["cases"], indent=2))
