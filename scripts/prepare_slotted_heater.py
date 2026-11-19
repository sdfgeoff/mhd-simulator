"""Prepare one radial heater-slot sensitivity; CPU CAD only, no force prediction.

The cut connects the smooth M6 bore to the +x outer face through the full
block height. It preserves every nozzle/liquid/feedstock/coil accessor byte.
"""
import copy
import hashlib
import json
from pathlib import Path
import struct
import shutil

import numpy as np

from make_conduction_hardware import box, triangles
from make_v6_conduction import as_solid
from prepare_lower_coil_package import accessor_bytes


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def export_review_and_plan(root, folder, path, record):
    assert sha(path) == record["project_sha256"]
    review = Path("hardware/designs/lowered-v6-heater-slot-review")
    review.mkdir(parents=True, exist_ok=False)
    for name in ["geometry.gltf", "geometry.bin", "heater-slotted.mhd.json", "manifest.json"]:
        shutil.copy2(folder / name, review / name)
    job_output = root / "lowered-coil-slotted-heater-v1-nvidia"
    plan = {
        "status": "Prepared stationary slot sensitivity; no numerical result",
        "air_inductance_path": str(root / "lowered-coil-cold-v1-air-inductance.json"),
        "jobs": [{"geometry": "lowered-v6-heater-slotted08", "shape": "single15-100",
                  "project": str(path), "project_sha256": sha(path),
                  "peak_A": 250, "rise_us": 15, "fall_us": 100,
                  "requested_steps": 1917, "end_time_s": .005001,
                  "output": str(job_output),
                  "command": ["target/debug/examples/hardware_probe", str(path), "1917",
                              str(job_output), "0", "9", "150", "3"]}],
        "additional_frozen_inputs": ["scripts/analyze_unipolar_coil.py", str(root / "lowered-coil-cold-v1-air-inductance.json")],
        "schedule": "Same0.5us to150us then3us through5.001ms as completed unslotted control; use frozen hardware_probe_tail binary via explicit queue option",
        "reference": "hardware/lowered-coil-cold-comparison.json",
        "limits": record["limits"],
        "postprocess_commands": [["python3", "scripts/analyze_unipolar_coil.py", "--plan",
                                  str(root / "lowered-coil-slotted-heater-v1-plan.json")]],
    }
    (root / "lowered-coil-slotted-heater-v1-plan.json").write_text(json.dumps(plan, indent=2) + "\n")


def main():
    root = Path("artifacts/hardware-research")
    source = root / "cases/lowered-coil-cold-v1/heater-on.mhd.json"
    project = json.loads(source.read_text())
    geometry = source.parent / project["geometry"]
    doc = json.loads(geometry.read_text())
    original = (geometry.parent / doc["buffers"][0]["uri"]).read_bytes()
    folder = root / "cases/lowered-coil-slotted-heater-v1"
    folder.mkdir(parents=True, exist_ok=False)
    mesh_index = next(i for i, m in enumerate(doc["meshes"]) if m["name"].startswith("Drawing-based heater"))
    index = doc["meshes"][mesh_index]["primitives"][0]["attributes"]["POSITION"]
    data = accessor_bytes(doc, original, index)
    points = np.frombuffer(data, dtype="<f4").reshape(-1, 3, 3).astype(float) * 1000
    heater = as_solid(points)
    # 0.8mm aligns to voxel faces on the200um domain; four cells across.
    cutter = box([8.2, .8, 12.5], [4, 0, 11.25])
    slotted = heater - cutter
    assert len(slotted.decompose()) == 1, "Slot split heater into loose parts"
    assert (slotted ^ cutter).volume() < 1e-8
    result_tris = np.asarray(triangles(slotted))
    new_bytes = b"".join(struct.pack("<fff", *(p * .001)) for p in result_tris.reshape(-1, 3))
    binary = original + new_bytes
    doc["bufferViews"].append({"buffer": 0, "byteOffset": len(original), "byteLength": len(new_bytes)})
    doc["accessors"].append({"bufferView": len(doc["bufferViews"]) - 1, "componentType": 5126,
                             "count": len(result_tris) * 3, "type": "VEC3",
                             "min": (result_tris.reshape(-1,3).min(axis=0) * .001).tolist(),
                             "max": (result_tris.reshape(-1,3).max(axis=0) * .001).tolist()})
    doc["meshes"][mesh_index]["primitives"][0]["attributes"]["POSITION"] = len(doc["accessors"]) - 1
    doc["buffers"][0].update(uri="geometry.bin", byteLength=len(binary))
    (folder / "geometry.bin").write_bytes(binary)
    (folder / "geometry.gltf").write_text(json.dumps(doc, indent=2) + "\n")
    checks = []
    original_doc = json.loads(geometry.read_text())
    for i, mesh in enumerate(doc["meshes"]):
        if i == mesh_index:
            continue
        old_idx = original_doc["meshes"][i]["primitives"][0]["attributes"]["POSITION"]
        new_idx = mesh["primitives"][0]["attributes"]["POSITION"]
        old = accessor_bytes(original_doc, original, old_idx)
        new = accessor_bytes(doc, binary, new_idx)
        assert old == new
        checks.append({"mesh": mesh["name"], "sha256": hashlib.sha256(new).hexdigest()})
    project["geometry"] = "geometry.gltf"
    project["bodies"][3]["triangles"] = len(result_tris)
    path = folder / "heater-slotted.mhd.json"
    path.write_text(json.dumps(project, indent=2) + "\n")
    baseline = json.loads(source.read_text())
    restored = copy.deepcopy(project)
    restored["geometry"] = baseline["geometry"]
    restored["bodies"][3]["triangles"] = baseline["bodies"][3]["triangles"]
    assert restored == baseline
    record = {
        "status": "Prepared radial-slot CAD sensitivity only; no native EM, fluid or thermal result",
        "source_project": str(source), "source_project_sha256": sha(source),
        "source_geometry_sha256": sha(geometry),
        "project": str(path), "project_sha256": sha(path),
        "slot_width_mm": .8, "slot_path": "Smooth M6 bore to+x outer face, through entire5.5..17mm block height",
        "heater_connected_components": 1,
        "unslotted_volume_mm3": heater.volume(), "slotted_volume_mm3": slotted.volume(),
        "unchanged_mesh_payloads": checks,
        "unchanged_project_configuration_except": ["geometry path", "heater triangle count"],
        "motivation": "Interrupt circumferential heater eddy-current paths using a simple accessible saw/mill cut, before changing nozzle or adding ceramics. Effect size and force direction require native comparison.",
        "source": {"url": "https://doi.org/10.1002/mrm.25424", "title": "Dual optimization method of radiofrequency and quasistatic field simulations for reduction of eddy currents generated on7T radiofrequency coil shielding", "scope": "Primary MRI experiments support testing patterned current-path interruptions. Their result does not establish that one radial heater slot suppresses our fields or improves jetting; simple slot layouts can remain problematic."},
        "limits": [
            "Proposed custom cut, not a represented stock heater clamp slot or a qualified machining drawing.",
            "M6 threads, heatbreak, inserts and fasteners remain omitted; conductive bridges can restore current paths across the cut.",
            "Thread retention, tightening torque, thermal conduction and heater/sensor retention need separate checks. No slit through the pressure-containing nozzle is proposed.",
            "Four200um cells span the slot. This is not a convergence result, calibrated hot conductivity or working-head recommendation.",
        ],
    }
    (folder / "manifest.json").write_text(json.dumps(record, indent=2) + "\n")
    Path("hardware/lowered-coil-slot-plan.json").write_text(json.dumps(record, indent=2) + "\n")
    export_review_and_plan(root, folder, path, record)
    print(json.dumps(record, indent=2))


if __name__ == "__main__":
    main()
