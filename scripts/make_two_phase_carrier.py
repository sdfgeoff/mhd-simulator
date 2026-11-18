"""Reuse generated chamber CAD with two phased air-core windings.

Prescribed currents only; no per-coil voltage, mutual-load circuit or jet proof.
The core 3D RunState solver is unchanged. Body vertex payloads are retained.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import struct

from hardware_screen import coil
from make_hardware import emit, winding


def generated_bodies(path):
    data = json.loads(path.read_text())
    if not data["asset"].get("generator", "").startswith("MHD hardware study"):
        raise ValueError("This helper only reads the study's generated flat glTF")
    buffers = [(path.parent/b["uri"]).read_bytes() for b in data["buffers"]]
    result = []
    for mesh in data["meshes"]:
        for primitive in mesh["primitives"]:
            if primitive.get("mode", 4) != 4:
                continue
            accessor = data["accessors"][primitive["attributes"]["POSITION"]]
            view = data["bufferViews"][accessor["bufferView"]]
            if "indices" in primitive or accessor["componentType"] != 5126 or accessor["type"] != "VEC3" or view.get("byteStride", 12) != 12:
                raise ValueError("Expected unindexed packed float32 triangle vertices")
            start = view.get("byteOffset", 0)+accessor.get("byteOffset", 0)
            raw = buffers[view["buffer"]][start:start+accessor["count"]*12]
            points = [tuple(v*1000 for v in p) for p in struct.iter_unpack("<fff", raw)]
            result.append((mesh["name"], [tuple(points[i:i+3]) for i in range(0, len(points), 3)]))
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("project", type=Path)
    p.add_argument("output", type=Path)
    p.add_argument("--phase-deg", type=float, default=90.)
    p.add_argument("--peak-a", type=float, default=200.)
    a = p.parse_args()
    if not math.isfinite(a.phase_deg) or not math.isfinite(a.peak_a) or a.peak_a <= 0:
        p.error("Choose finite phase and positive peak current")
    if a.output.exists() or a.output.with_suffix(".gltf").exists():
        p.error("Choose a new output stem")
    data = json.loads(a.project.read_text())
    geometry = (a.project.parent/data["geometry"]).resolve()
    bodies = generated_bodies(geometry)
    c = coil(4.3, 4.5, 1., 2, 100.)
    centres = [2.5, 8.]
    curves = [winding(c, z, return_drop_mm=1.) for z in centres]
    names = ["Phase A lower coil with return lead", "Phase B upper coil with return lead"]
    cad_path = a.output.with_suffix(".gltf")
    a.output.parent.mkdir(parents=True, exist_ok=True)
    emit(cad_path, bodies, None, list(zip(names, curves)))
    # Each imported body uses the same packed float32 positions as the source.
    roundtrip = generated_bodies(cad_path)
    assert bodies == roundtrip, "Body geometry changed during repackaging"
    f32 = lambda x: struct.unpack("<f", struct.pack("<f", x))[0]
    data["geometry"] = cad_path.name
    data["cad_curves"] = [{"name": name+" / 0:0", "points_mm": [[f32(f32(v*.001)*1000) for v in point] for point in curve]} for name, curve in zip(names, curves)]
    dt, burst, ramp, frequency = .125e-6, 300e-6, 40e-6, 5e4
    wires = []
    for index, phase in enumerate([0., math.radians(a.phase_deg)]):
        samples = []
        count = round(burst/dt)
        for i in range(count+1):
            t = burst*i/count
            envelope = max(0., min(t/ramp, (burst-t)/ramp, 1.))
            samples.append([t, a.peak_a*envelope*math.sin(2*math.pi*frequency*t+phase)])
        samples.append([burst+200e-6, 0.])
        wires.append({"cad_curve": index, "reverse": False, "name": names[index]+" / 0:0", "core_radius_mm": .5, "current": {"kind": "table", "samples": samples}})
    data["electromagnetics"]["wires"] = wires
    data["run"].update(dt_s=dt, fluid_enabled=False, thermal_enabled=False, prescribed_jet=None)
    data["fluid"].update(dt_s=dt, surface={"enabled": False})
    data["domain"].update(min_mm=[-9., -9., -2.], max_mm=[9., 9., 12.])
    with a.output.open("x") as f:
        json.dump(data, f, indent=2)
        f.write("\n")
    metadata = {
        "source_project": str(a.project), "source_sha256": hashlib.sha256(a.project.read_bytes()).hexdigest(),
        "coil": c, "coil_centres_z_mm": centres, "relative_phase_deg": a.phase_deg,
        "peak_current_A_per_phase": a.peak_a, "carrier_hz": frequency, "burst_us": 300, "ramp_us": 40,
        "geometry_check": "Original generated triangle bodies survive float32 roundtrip unchanged. Both centreline coils are stored in the main glTF.",
        "return_drop_mm": 1., "nominal_clearance_between_upper_return_and_lower_winding_mm": .26,
        "primary_background": "https://arxiv.org/abs/1212.2105",
        "limits": [
            "Two phased coil sources are a hypothesis, not a demonstrated miniature induction pump or solder jet.",
            "Prescribed phase currents, without mutual-load voltage/circuit, driver or cooling qualification. The one-coil reaction observer is not applicable here.",
            "Nominal wire diameter/packing and return clearances need manufactured-CAD and insulation tolerance checks; no solid winding mesh is included.",
            "Chamber geometry and properties retained from the source; cold screening first, not a moving-fluid result.",
        ],
    }
    a.output.with_name(a.output.stem+"-two-phase-assumptions.json").write_text(json.dumps(metadata, indent=2)+"\n")
    print(a.output)


if __name__ == "__main__":
    main()
