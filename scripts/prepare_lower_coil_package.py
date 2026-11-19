"""Prepare a lowered winding and drawing-based heater geometry; no GPU run.

Run in the cached uv manifold3d/numpy/matplotlib environment. Source solid
accessor bytes are preserved; heater contact and wire-envelope clearances
are nominal checks, not toleranced manufacturing qualification.
"""
import copy
import hashlib
import json
import math
from pathlib import Path
import struct

import manifold3d as cad
import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.collections import LineCollection
from matplotlib.patches import Circle, Rectangle

from make_conduction_hardware import box, cylinder, triangles
from make_hardware import winding
from make_v6_conduction import as_solid


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def accessor_bytes(doc, binary, index):
    a = doc["accessors"][index]
    v = doc["bufferViews"][a["bufferView"]]
    assert a["componentType"] == 5126 and a["type"] == "VEC3" and v["buffer"] == 0
    start = v.get("byteOffset", 0) + a.get("byteOffset", 0)
    return bytes(binary[start:start + a["count"] * 12])


def section_lines(tris):
    lines = []
    for tri in np.asarray(tris):
        pts = []
        for a, b in zip(tri, np.roll(tri, -1, axis=0)):
            if abs(a[1]) < 1e-7:
                pts.append(a[[0, 2]])
            if a[1] * b[1] < 0:
                pts.append((a + (b-a) * (-a[1] / (b[1]-a[1])))[[0, 2]])
        unique = np.unique(np.round(pts, 6), axis=0) if pts else []
        if len(unique) == 2:
            lines.append(unique)
    return lines


def main():
    root = Path("artifacts/hardware-research")
    folder = root / "cases/lowered-coil-package-v2"
    folder.mkdir(parents=True, exist_ok=False)
    source = root / "cases/unipolar-coil-v2/v6-symmetric100-cold-h020.mhd.json"
    base = json.loads(source.read_text())
    geometry = source.parent / base["geometry"]
    doc = json.loads(geometry.read_text())
    assert len(doc["buffers"]) == 1 and len(base["bodies"]) == 3
    binary_path = geometry.parent / doc["buffers"][0]["uri"]
    original = binary_path.read_bytes()
    binary = bytearray(original)
    solids = []
    solid_payloads = []
    for mesh in doc["meshes"]:
        primitive, = mesh["primitives"]
        if primitive["mode"] == 4:
            data = accessor_bytes(doc, original, primitive["attributes"]["POSITION"])
            solids.append(np.frombuffer(data, dtype="<f4").astype(float).reshape(-1, 3, 3) * 1000)
            solid_payloads.append(hashlib.sha256(data).hexdigest())
    assert len(solids) == 3

    coil = {"wire_diameter_mm": 1.2, "turns_per_layer": 3, "layers": 4,
            "inner_radius_mm": 4.3, "outer_diameter_mm": 2 * (4.3 + 4 * 1.296)}
    points = winding(coil, 2.0, return_drop_mm=1.5)
    # Route the outer return1.3mm beyond the round-wire winding envelope.
    winding_ro = 4.3 + 3.5 * 1.296 + .6
    points[-4] = (winding_ro + 1.3, points[-4][1], points[-4][2])
    points[-3] = (winding_ro + 1.3, points[-3][1], points[-3][2])
    assert points[0] == points[-1]

    # E3D heater drawing:23x16x11.5; M6 axis8mm from right edge,
    # cartridge axis6.5mm to its left,4mm above bottom. Assume0.5mm
    # assembly gap above the nominal nozzle hex topz5mm.
    bottom, top = 5.5, 17.0
    heater = box([23, 16, 11.5], [-3.5, 0, (bottom + top)/2])
    heater -= cylinder(3, 13, [0, 0, (bottom + top)/2])
    for x, z, radius in [(-6.5, bottom + 4, 3), (6, bottom + 5.75, 1.55)]:
        hole = cad.Manifold.cylinder(17, radius, circular_segments=96, center=True).rotate([90, 0, 0]).translate([x, 0, z])
        heater -= hole
    for x in [-12.5, 6]:
        heater -= cylinder(1.5, 13, [x, 0, (bottom + top)/2])
    heater_tris = triangles(heater)
    overlap = (as_solid(solids[0].tolist()) ^ heater).volume()
    assert overlap < 1e-3, overlap

    def append_positions(points_mm):
        positions = np.asarray(points_mm).reshape(-1, 3)
        offset = len(binary)
        binary.extend(b"".join(struct.pack("<fff", *(p * .001)) for p in positions))
        doc["bufferViews"].append({"buffer": 0, "byteOffset": offset, "byteLength": len(binary)-offset})
        doc["accessors"].append({"bufferView": len(doc["bufferViews"])-1, "componentType": 5126,
                                 "count": len(positions), "type": "VEC3",
                                 "min": (positions.min(axis=0)*.001).tolist(), "max": (positions.max(axis=0)*.001).tolist()})
        return len(doc["accessors"])-1

    curve = next(m for m in doc["meshes"] if m["primitives"][0]["mode"] == 3)
    curve_index = doc["meshes"].index(curve)
    curve["name"] = "Lower12turn4layercoil"
    next(node for node in doc["nodes"] if node.get("mesh") == curve_index)["name"] = curve["name"]
    curve["primitives"][0]["attributes"]["POSITION"] = append_positions(points)
    imported_points = (np.frombuffer(accessor_bytes(doc, binary, curve["primitives"][0]["attributes"]["POSITION"]),
                                    dtype="<f4").reshape(-1,3) * np.float32(1000)).tolist()
    heater_name = "Drawing-based heater with empty cartridge and sensor bores"
    doc["meshes"].append({"name": heater_name, "primitives": [{"mode": 4, "attributes": {"POSITION": append_positions(heater_tris)}}]})
    doc["nodes"].append({"name": heater_name, "mesh": len(doc["meshes"])-1})
    doc["scenes"][0]["nodes"].append(len(doc["nodes"])-1)
    doc["buffers"][0].update(uri="geometry.bin", byteLength=len(binary))
    (folder / "geometry.bin").write_bytes(binary)
    (folder / "geometry.gltf").write_text(json.dumps(doc, indent=2) + "\n")
    checks = []
    for i in range(3):
        digest = hashlib.sha256(accessor_bytes(doc, binary, doc["meshes"][i]["primitives"][0]["attributes"]["POSITION"])).hexdigest()
        assert digest == solid_payloads[i]
        checks.append({"mesh": doc["meshes"][i]["name"], "unchanged_sha256": digest})
    cold_cases = []
    for included in [False, True]:
        p = copy.deepcopy(base)
        p["geometry"] = "geometry.gltf"
        p["cad_curves"][0].update(name="Lower12turn4layercoil / 0:0", points_mm=imported_points)
        p["electromagnetics"]["wires"][0].update(name="Lower12turn4layercoil", current={"kind": "table", "samples": [[0,0],[15e-6,250],[115e-6,0],[300e-6,0]]})
        p["domain"].update(min_mm=[-18,-12,-6], max_mm=[14,12,21.2], cell_mm=.2)
        p["run"].update(fluid_enabled=False, thermal_enabled=False, dt_s=.5e-6)
        p["fluid"]["surface"]["enabled"] = False
        material = next(m for m in p["materials"] if m["id"] == 8)
        material.update(conductivity_s_m=15e6, name="Aluminium6061 fixed conductivity sensitivity",
                        notes="15MS/m is an uncalibrated hot-conductivity scenario, not a temperature-dependent material curve.")
        p["bodies"].append({"name": heater_name + " / 0", "triangles": len(heater_tris), "visible": True,
                            "simulation": {"included": included, "material_id": 8, "temperature_c": 250,
                                           "fixed_temperature_c": None, "heat_source_w_m3": 0}})
        path = folder / ("heater-on.mhd.json" if included else "heater-off.mhd.json")
        path.write_text(json.dumps(p, indent=2) + "\n")
        cold_cases.append({"heater_included": included, "project": str(path), "sha256": sha(path)})

    upper = max(p[2] for p in points) + .6
    lower = min(p[2] for p in points) - .6
    radial_gap = 4.3 + .5*1.296 - .6 - 3.5/math.cos(math.pi/6)
    assert bottom-upper > .9 and radial_gap > .3
    manifest = {"status": "Prepared packaging/cold-EM controls; no GPU force, voltage or jetting result",
                "source_project": str(source), "source_sha256": sha(source), "source_geometry_sha256": sha(geometry),
                "source_binary_sha256": sha(binary_path), "unchanged_solid_checks": checks,
                "coil": {**coil, "turns": 12, "centre_z_mm": 2, "bare_wire_radius_mm": .6,
                         "bare_winding_outer_diameter_mm": 2*winding_ro, "including_return_min_z_mm": lower,
                         "including_return_max_z_mm": upper, "heater_vertical_clearance_mm": bottom-upper,
                         "nozzle_hex_circumscribed_radial_clearance_mm": radial_gap},
                "heater": {"bounds_mm": [[-15,-8,bottom],[8,8,top]], "nozzle_overlap_mm3": overlap,
                           "fixed_conductivity_S_m": 15e6, "cases": cold_cases},
                "sources": [{"url": "https://cdn03.plentyone.com/ioseuwg7moqp/propertyItems/4488763/V6Datasheet_2019%5B1%5D.pdf",
                             "author": "E3D", "page": 7, "local_sha256": sha(root / "sources/v6-2019-datasheet.pdf"),
                             "use": "Heater envelope and bore locations. The0.5mm assembly gap is our explicit assumption."}],
                "limits": ["Round-wire bounds use bare1.2mm copper; enamel, bobbin, tolerance, hot deformation and winding retention not qualified.",
                           "Thread helix, heater clamp slot, fasteners, thermistor/heater inserts, heatbreak and leads beyond the modeled return are omitted. Empty bores are not an assembled heater.",
                           "Al15MS/m is a sensitivity only. Heater-on/off isolates inclusion of this simplified metal body, not a calibrated heater or strict shielding bound.",
                           "12turns and coilposition change together versus older10turns; no matched-turn or force equivalence is implied.",
                           "Source V6-like0.5mm outlet is not a verified catalogue stainless SKU. No thermal or single-pulse fluid result for this package.",
                           "The return's nominal2.04mm projection is not final surface stand-off; support and insulation must count too."]}
    (folder / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    Path("hardware/lowered-coil-package.json").write_text(json.dumps(manifest, indent=2) + "\n")

    fig, axes = plt.subplots(1, 2, figsize=(12, 6), constrained_layout=True)
    for ax, line, title in zip(axes, [base["cad_curves"][0]["points_mm"], points], ["Earlier 10-turn coil", "Lower 12-turn, 4-layer proposal"]):
        ax.add_collection(LineCollection(section_lines(solids[0]), colors="#555555", linewidths=1))
        ax.add_collection(LineCollection(section_lines(heater_tris), colors="#bb5555", linewidths=1))
        helix = np.asarray(line[:-4])
        cut = []
        for a,b in zip(helix, helix[1:]):
            if abs(a[1]) < 1e-8:
                cut.append(a[[0,2]])
            elif a[1]*b[1] < 0:
                cut.append((a+(b-a)*(-a[1]/(b[1]-a[1])))[[0,2]])
        for x,z in np.unique(np.round(cut,6),axis=0):
            ax.add_patch(Circle((x,z),.6,facecolor="#cc7722",edgecolor="#774400",alpha=.8))
        ret = np.asarray(line[-5:])
        ax.plot(ret[:,0],ret[:,2],color="#cc7722",linewidth=2,label="Return centreline")
        ax.axhline(0,color="black",linestyle=":",linewidth=.7)
        ax.axhline(bottom,color="#bb5555",linestyle="--",linewidth=.7)
        ax.set(xlim=(-16,14),ylim=(-4,19),xlabel="x (mm)",ylabel="z (mm)",title=title)
        ax.set_aspect("equal")
        ax.grid(alpha=.15)
    axes[0].annotate("Coil overlaps block",xy=(-5,7.5),xytext=(-14,2.4),arrowprops={"arrowstyle":"->","color":"#aa3333"},color="#aa3333",fontsize=9)
    axes[1].text(-14,-3.1,f"Nominal metal gap to block: {bottom-upper:.2f} mm\nReturn + wire extends {-lower:.2f} mm below outlet",fontsize=9)
    fig.suptitle("Coil placement with the heater block present — section/envelope study\nBare-wire geometry only; no cooling, insulation or ejection qualification",fontsize=13)
    figure = root / "diagrams/lowered-coil-package.png"
    fig.savefig(figure,dpi=160)
    print(json.dumps(manifest["coil"],indent=2))
    print(figure)


if __name__ == "__main__":
    main()
