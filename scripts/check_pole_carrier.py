"""Check nominal mesh gaps, STL round trips and intentional thread envelopes."""

import argparse, json, struct, math
from pathlib import Path
import numpy as np
import manifold3d as cad
from draw_hardware_layouts import load

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--case", default="m6-carrier-review-wide-stops")
parser.add_argument(
    "--output", type=Path, default=Path("hardware/m6-carrier-review-screening.json")
)
args = parser.parse_args()
root = Path("artifacts/hardware-research/cases") / args.case
manifest = json.loads((root / "manifest.json").read_text())
parts, _ = load(args.case + "/assembly")
base_count = manifest.get("base_body_count", 9)
fixtures = parts[base_count:]
gaps = {}
for label, targets in [
    ("copper", parts[2:4]),
    ("heater", [parts[8]]),
    ("nozzle", [parts[0]]),
    ("mica", parts[6:8]),
]:
    candidates = [
        (float(a.min_gap(b, 10.0)), name, other)
        for name, _, a in fixtures
        for other, _, b in targets
    ]
    value, name, other = min(candidates)
    gaps[label] = {
        "minimum_gap_mm": value,
        "fixture_part": name,
        "target_part": other,
        "search_cap_mm": 10.0,
    }
expected_copper = min(
    manifest["nominal_keeper_to_copper_Y_clearance_mm"],
    manifest["nominal_keeper_to_copper_X_clearance_mm"],
)
if abs(gaps["copper"]["minimum_gap_mm"] - expected_copper) > 1e-4:
    raise ValueError("Unexpected copper gap")
if (
    abs(
        gaps["heater"]["minimum_gap_mm"]
        - manifest["nominal_keeper_to_heater_Z_clearance_mm"]
    )
    > 1e-4
):
    raise ValueError("Unexpected heater gap")
fasteners = [part for part in parts[:base_count] if "M2x3 screw and nut" in part[0]]
if fasteners:
    value, name, other = min(
        (float(a.min_gap(b, 10.0)), name, other)
        for name, _, a in fixtures
        for other, _, b in fasteners
    )
    gaps["contact_fasteners"] = {
        "minimum_gap_mm": value,
        "fixture_part": name,
        "target_part": other,
        "search_cap_mm": 10.0,
    }
    if value < 0.19:
        raise ValueError("Contact fastener clearance below0.19mm review threshold")
roundtrips = []
for part in manifest["part_files"]:
    if part["stl"] is None:
        continue
    data = (root / part["stl"]).read_bytes()
    n = struct.unpack_from("<I", data, 80)[0]
    if len(data) != 84 + 50 * n:
        raise ValueError("STL length")
    records = np.frombuffer(
        data,
        offset=84,
        dtype=np.dtype(
            [("normal", "<f4", (3,)), ("vertex", "<f4", (3, 3)), ("attr", "<u2")]
        ),
    )
    vertices, indices = np.unique(
        records["vertex"].reshape(-1, 3), axis=0, return_inverse=True
    )
    solid = cad.Manifold(
        cad.Mesh(vertices.astype(np.float32), indices.reshape(-1, 3).astype(np.uint32))
    )
    if (
        str(solid.status()) != "Error.NoError"
        or solid.volume() <= 0
        or len(solid.decompose()) != 1
    ):
        raise ValueError("STL topology " + part["stl"])
    error = abs(solid.volume() / part["volume_mm3"] - 1)
    if error > 1e-5:
        raise ValueError("STL volume " + part["stl"])
    if np.max(abs(np.linalg.norm(records["normal"], axis=1) - 1)) > 1e-5:
        raise ValueError("STL normals")
    roundtrips.append(
        {
            "file": part["stl"],
            "triangles": n,
            "closed_manifold": True,
            "volume_relative_difference": error,
        }
    )
engagements = [
    row
    for row in manifest["checks"]
    if "intentional_pilot_thread_engagement_mm3" in row
]
if len(engagements) != 8:
    raise ValueError("Expected one pilot/thread engagement per M2 screw")
r = {
    "status": "Completed nominal CAD interference, distance and STL checks; not qualified hardware",
    "manifest": manifest,
    "measured_mesh_gaps": gaps,
    "STL_roundtrips": roundtrips,
    "mass_g_assumed_solid_materials": sum(
        row["volume_mm3"] * (0.0027 if row["material_id"] == 8 else 0.008)
        for row in manifest["part_files"]
    ),
    "scope": "Fixed nominal geometry. The eight screw/pilot radial intersections represent intended thread engagement. Other carrier-part intersections and all new-part/source-model intersections are rejected by generator. No preload, tolerances, thread roots, thermal distortion or physics inclusion.",
}
# Conservative uniform cantilever screen, not frame/joint finite-element analysis.
load = 20.0
length = 18.2
elastic_modulus = 69000.0
inertia = 5 * 4**3 / 12 - math.pi * 3**4 / 64
r["conditional_beam_screen"] = {
    "pole_load_N": load,
    "load_per_support_N": load / 2,
    "unsupported_length_mm": length,
    "section_mm": [5, 4],
    "subtracted_full_length_hole_diameter_mm": 3,
    "second_moment_mm4": inertia,
    "assumed_E_MPa": elastic_modulus,
    "elastic_gap_closure_um": 2
    * (load / 2)
    * length**3
    / (3 * elastic_modulus * inertia)
    * 1000,
    "nominal_root_stress_MPa": (load / 2) * length * 2 / inertia,
    "scope": "Uniform cantilevers clamped at upper tabs; full hole subtracted throughout. Excludes joint compliance, local stresses, preload and thermal motion.",
}
with args.output.open("x") as out:
    json.dump(r, out, indent=2)
    out.write("\n")
print(
    json.dumps(
        {
            "gaps": gaps,
            "STL_files_checked": len(roundtrips),
            "modeled_fixture_mass_g": r["mass_g_assumed_solid_materials"],
        },
        indent=2,
    )
)
