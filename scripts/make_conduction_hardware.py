"""Generate an experimental all-metal cross-channel DC pump, not a qualified design.

Run with: TMPDIR=... uv run --with manifold3d python scripts/make_conduction_hardware.py
CAD boolean construction only; all numerical fields are evaluated on the GPU.
"""

import argparse
import copy
import json
import math
from pathlib import Path

import manifold3d as cad

from make_hardware import emit


def cylinder(radius, length, centre, axis="z"):
    result = cad.Manifold.cylinder(length, radius, circular_segments=96, center=True)
    if axis == "x":
        result = result.rotate([0, 90, 0])
    return result.translate(centre)


def box(size, centre):
    return cad.Manifold.cube(size, center=True).translate(centre)


def triangles(solid):
    if not solid.volume() > 0:
        raise ValueError("Expected a positive closed CAD volume")
    mesh = solid.to_mesh()
    return [
        tuple(tuple(float(v) for v in mesh.vert_properties[i][:3]) for i in tri)
        for tri in mesh.tri_verts
    ]


def write_project(parts, args):
    meshes = [
        (name, triangles(s) if hasattr(s, "to_mesh") else s) for name, s, _ in parts
    ]
    path = args.output / (args.name + ".gltf")
    emit(path, meshes, [])
    root = Path(__file__).resolve().parents[1]
    p = json.loads((root / "assets/nozzle-coil.mhd.json").read_text())
    p["geometry"] = path.name
    p["cad_curves"] = []
    p["bodies"] = [
        {
            "name": name + " / 0",
            "triangles": len(meshes[i][1]),
            "visible": True,
            "simulation": {
                "included": True,
                "material_id": mid,
                "temperature_c": 20.0 if mid in [15, 18] else 250.0,
                "fixed_temperature_c": None,
                "heat_source_w_m3": 0.0,
            },
        }
        for i, (name, _, mid) in enumerate(parts)
    ]
    material = {m["id"]: m for m in p["materials"]}
    steel = copy.deepcopy(material[6])
    steel.update(
        id=14,
        name="Thin-wall stainless screening",
        conductivity_s_m=args.wall_sigma,
        relative_permeability=1.0,
    )
    wire_mat = copy.deepcopy(material[4])
    wire_mat.update(
        id=15,
        name="Fixed solid solder feedstock screening",
        conductivity_s_m=6e6,
        dynamic_viscosity_pa_s=None,
        surface_tension_n_m=None,
    )
    copper = next(m for m in p["materials"] if m["name"].lower().startswith("copper"))
    extras = [steel, wire_mat]
    for mid, name in [
        (16, "Positive copper terminal"),
        (17, "Negative copper terminal"),
    ]:
        m = copy.deepcopy(copper)
        m.update(id=mid, name=name, conductivity_s_m=25e6, relative_permeability=1.0)
        extras.append(m)
    magnet = copy.deepcopy(steel)
    magnet.update(
        id=18,
        name="SmCo screening assumption",
        conductivity_s_m=1.1e6,
        notes="Assumed Br and conductivity; grade, temperature, coercivity and cooling must be qualified.",
    )
    extras.append(magnet)
    p["materials"].extend(extras)
    material[4].update(
        name="Sn99.3Cu0.7 liquid screening",
        conductivity_s_m=2.2e6,
        density_kg_m3=7000.0,
        dynamic_viscosity_pa_s=0.002,
        surface_tension_n_m=0.53,
    )
    # Applied-field evaluation uses the complete external magnet CAD. The
    # conduction domain only needs the pump and contacts, with insulated air
    # padding. This cropped domain does not model magnet eddy-current reaction.
    p["domain"] = {
        "min_mm": [
            -args.domain_x_mm,
            -getattr(args, "domain_y_mm", 1.8),
            -args.flight_mm,
        ],
        "max_mm": [
            args.domain_x_mm,
            getattr(args, "domain_y_mm", 1.8),
            args.domain_top_mm,
        ],
        "cell_mm": args.cell,
    }
    p["electromagnetics"].update(
        wires=[],
        uniform=[],
        magnets=[
            {
                "body": i,
                "magnetization_a_m": [0, -args.remanence / (4e-7 * math.pi), 0],
                "subdivisions": getattr(args, "magnet_subdivisions", 4),
                "smoothing_mm": 0.05,
            }
            for i, (_, _, mid) in enumerate(parts)
            if mid == 18
        ],
    )
    p["run"].update(fluid_enabled=False, thermal_enabled=False, prescribed_jet=None)
    p["fluid"]["surface"] = {"enabled": False}
    p["fluid"]["material_id"] = 4
    project = path.with_suffix(".mhd.json")
    project.write_text(json.dumps(p, indent=2) + "\n")
    print(project)
    return project


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--name", default="cross-channel-dc-h010")
    parser.add_argument("--cell", type=float, default=0.1)
    parser.add_argument("--wall-sigma", type=float, default=1.3e6)
    parser.add_argument("--flight-mm", type=float, default=5.0)
    parser.add_argument("--domain-x-mm", type=float, default=6.0)
    parser.add_argument("--domain-top-mm", type=float, default=8.0)
    parser.add_argument(
        "--wire-tip",
        type=float,
        default=4.9,
        help="Wire tip above the exit in mm; default is retracted above the 4.65 mm free level",
    )
    parser.add_argument(
        "--remanence",
        type=float,
        default=0.95,
        help="Assumed SmCo remanence in tesla; requires a specific supplier grade",
    )
    parser.add_argument(
        "--output", type=Path, default=Path("artifacts/hardware-research/cases")
    )
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    centre = 1.15
    outer = cylinder(1.25, 6.0, [0, 0, centre], "x")
    outer = outer ^ box([20, 20, 20], [0, 0, 10])
    outer = outer + cylinder(0.85, 4.0, [0, 0, centre + 2])
    main_void = cylinder(1.0, 5.5, [0, 0, centre], "x")
    nozzle_void = cylinder(0.25, 3.0, [0, 0, 0.5])
    feed_void = cylinder(0.6, 6.0, [0, 0, centre + 3])
    void = main_void + nozzle_void + feed_void
    wall = outer - void
    if not 1.2 < args.wire_tip < 6.15:
        raise ValueError("Wire tip must lie between 1.2 and 6.15 mm")
    wire = cylinder(0.4, 6.15 - args.wire_tip, [0, 0, (6.15 + args.wire_tip) / 2])
    liquid = ((void ^ outer) ^ box([20, 20, 4.65], [0, 0, 4.65 / 2])) - wire
    # Clamps contact the closed tube end faces. No insulating body is assumed.
    left = box([2, 2, 2], [-4, 0, centre])
    right = box([2, 2, 2], [4, 0, centre])
    magnets = [box([10, 10, 10], [0, sign * 7, centre]) for sign in [-1, 1]]
    parts = [
        ("Thin stainless pump capsule", wall, 14),
        ("Liquid solder", liquid, 4),
        ("Fixed feed wire", wire, 15),
        ("Positive copper contact", left, 16),
        ("Negative copper contact", right, 17),
    ]
    parts.extend((f"Bias magnet {i+1}", s, 18) for i, s in enumerate(magnets))
    write_project(parts, args)


if __name__ == "__main__":
    main()
