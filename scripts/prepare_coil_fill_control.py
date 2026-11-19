"""Prepare a controlled free-level change; no GPU work or jetting prediction.

Only the liquid mesh changes. Preserve original solid/coil accessor bytes.
"""
import copy
import hashlib
import json
import math
import struct
from pathlib import Path

from make_hardware import lathe


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def payload(doc, data, primitive):
    a = doc["accessors"][primitive["attributes"]["POSITION"]]
    v = doc["bufferViews"][a["bufferView"]]
    assert a["componentType"] == 5126 and a["type"] == "VEC3"
    assert v.get("byteStride", 12) == 12 and v["buffer"] == 0
    start = v.get("byteOffset", 0) + a.get("byteOffset", 0)
    return bytes(data[start:start + a["count"] * 12])


def main():
    root = Path("artifacts/hardware-research")
    source = Path("hardware/designs/single-coil-250A/case.mhd.json")
    geometry = source.parent / "geometry.gltf"
    original = json.loads(geometry.read_text())
    assert len(original["buffers"]) == 1
    assert [m["name"] for m in original["meshes"]] == [
        "Stainless nozzle", "Liquid solder", "Fixed feed wire", "Series coil with return lead"
    ]
    binary_path = geometry.parent / original["buffers"][0]["uri"]
    original_data = binary_path.read_bytes()
    folder = root / "cases/coil-fill-control-v1"
    folder.mkdir(parents=True, exist_ok=False)
    doc = copy.deepcopy(original)
    # Wide pool below the roof; fixed feed tip at5.5mm remains immersed0.1mm.
    profile = [(.4, 5.6), (3, 5.6), (3, .15), (.25, .15),
               (.25, 0), (0, 0), (0, 5.5), (.4, 5.5)]
    triangles = lathe([(r, z, False) for r, z in profile])
    points = [p for tri in triangles for p in tri]
    liquid_bytes = b"".join(struct.pack("<fff", *(x * .001 for x in p)) for p in points)
    data = original_data + liquid_bytes
    primitive = doc["meshes"][1]["primitives"][0]
    accessor = doc["accessors"][primitive["attributes"]["POSITION"]]
    view = doc["bufferViews"][accessor["bufferView"]]
    view.update(byteOffset=len(original_data), byteLength=len(liquid_bytes))
    accessor.update(count=len(points), byteOffset=0,
                    min=[min(p[k] for p in points) * .001 for k in range(3)],
                    max=[max(p[k] for p in points) * .001 for k in range(3)])
    doc["buffers"][0].update(uri="low-fill.bin", byteLength=len(data))
    (folder / "low-fill.bin").write_bytes(data)
    (folder / "low-fill.gltf").write_text(json.dumps(doc, indent=2) + "\n")
    checks = []
    for i in [0, 2, 3]:
        old = payload(original, original_data, original["meshes"][i]["primitives"][0])
        new = payload(doc, data, doc["meshes"][i]["primitives"][0])
        assert old == new
        checks.append({"mesh": original["meshes"][i]["name"], "unchanged_accessor_sha256": hashlib.sha256(old).hexdigest()})
    # Polygonal lathe volume vs exact circular profile checks orientation/closure
    # and expected material removal. Topological closure is checked by lathe().
    volume = sum(sum(a[k] * (b[(k+1)%3]*c[(k+2)%3] - b[(k+2)%3]*c[(k+1)%3]) for k in range(3)) for a,b,c in triangles) / 6
    analytic_volume = math.pi * (.25**2 * .15 + 3**2 * (5.6 - .15) - .4**2 * .1)
    assert abs(volume / analytic_volume - 1) < .001
    base_path = root / "cases/unipolar-coil-v2/custom-symmetric100-flow-h010.mhd.json"
    base = json.loads(base_path.read_text())
    cases = []
    for label in ["high", "low"]:
        project = copy.deepcopy(base)
        project["geometry"] = str((geometry if label == "high" else folder / "low-fill.gltf").resolve())
        if label == "low":
            project["bodies"][1]["triangles"] = len(triangles)
        path = folder / (label + "-symmetric100-flow-h010.mhd.json")
        path.write_text(json.dumps(project, indent=2) + "\n")
        cases.append({"label": label, "project": str(path), "sha256": sha(path)})
    area_out = math.pi * .25**2
    common = .15 + area_out * (5.5 - .15) / (math.pi * 3**2)
    high = common + area_out * .5 / (math.pi * (3**2 - .4**2)) + area_out * 3 / (math.pi * (.6**2 - .4**2))
    low = common + area_out * .1 / (math.pi * (3**2 - .4**2))
    manifest = {
        "status": "Prepared matched free-level flow inputs; no native run or jetting result",
        "source_project_sha256": sha(source), "source_flow_project_sha256": sha(base_path),
        "source_geometry_sha256": sha(geometry), "source_binary_sha256": sha(binary_path),
        "unchanged_geometry_checks": checks, "cases": cases,
        "low_fill_volume_mm3": volume, "analytic_circular_volume_mm3": analytic_volume,
        "geometric_plug_flow_inertia": {
            "method": "Equivalent outlet slug length Aout*integral(dz/A_liquid), including fixed wire annulus. Geometric kinetic-energy estimate for net throughflow; excludes entrance, viscous, capillary and unsteady recirculation effects.",
            "high_level_mm": 9, "low_level_mm": 5.6,
            "high_equivalent_length_mm": high, "low_equivalent_length_mm": low,
            "ratio_high_over_low": high / low,
        },
        "free_surface_geometry": {
            "high_free_level_mm": 9., "high_annulus_outer_inner_radii_mm": [.6, .4],
            "high_free_annulus_area_mm2": math.pi*(.6**2-.4**2),
            "low_free_level_mm": 5.6, "low_annulus_outer_inner_radii_mm": [3., .4],
            "low_free_annulus_area_mm2": math.pi*(3.**2-.4**2),
            "low_over_high_free_area_ratio": (3.**2-.4**2)/(.6**2-.4**2),
            "scope": "Exact circular-profile areas before polygonal/voxel discretization. Lower fill is below the6mm chamber roof, outside the feed neck.",
        },
        "interpretation_correction": "Sixfold plug-flow inertia reduction does not predict better actuation. Upper free-interface area changes44.2fold and magnetic forcing also changes; compare resolved native upper/lower flow paths.",
        "limits": [
            "Lower free level changes conducting volume, free surface and force as well as hydraulic inertia; force is not assumed unchanged.",
            "Feed-neck radial gap0.2mm is onlyone200um/two100um cells; return-flow resistance needs refinement.",
            "0.1mm wire immersion is only one100um cell. Refine/inspect voxelization before treating wire contact as resolved.",
            "Original bottle-shaped chamber requires a joined cap or other manufacturing process; not a claimed one-piece bored part.",
            "100um flow inputs have onlyfivecells across outlet. No calibrated wetting, oxide, repetition or thermal qualification.",
        ],
    }
    (folder / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (root / "coil-fill-control-v1.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest["geometric_plug_flow_inertia"], indent=2))


if __name__ == "__main__":
    main()
