"""Extract an existing native applied B field after checking identical sources.

Applied mode uses prescribed magnetization without material reaction. Changing
passive chamber/electrode geometry therefore does not change this model's B.
No new field solve or interpolation is performed.
"""

import hashlib
import json
from pathlib import Path

import numpy as np

from draw_hardware_layouts import load

root = Path("artifacts/hardware-research")
source = root / "m6-rooted08-cold30-h0050"
target = Path("hardware/designs/split-slit-review/alumina-observer.mhd.json")
report = json.loads((source / "report.json").read_text())
source_project_path = Path(report["project"])
sp = json.loads(source_project_path.read_text())
tp = json.loads(target.read_text())
sm, _ = load(str(source_project_path.parent / sp["geometry"]))
tm, _ = load(str(target.parent / tp["geometry"]))
for key in ["wires", "uniform"]:
    if sp["electromagnetics"].get(key, []) or tp["electromagnetics"].get(key, []):
        raise ValueError("Unexpected additional applied sources")
source_magnets = sp["electromagnetics"]["magnets"]
target_magnets = tp["electromagnetics"]["magnets"]
if len(source_magnets) != len(target_magnets):
    raise ValueError("Different magnet count")
hashes = []
for a, b in zip(source_magnets, target_magnets):
    settings_a = {k: v for k, v in a.items() if k not in ["body", "subdivisions"]}
    settings_b = {k: v for k, v in b.items() if k not in ["body", "subdivisions"]}
    if settings_a != settings_b:
        raise ValueError("Magnet source settings differ")
    triangles_a, triangles_b = sm[a["body"]][1], tm[b["body"]][1]
    if not np.array_equal(triangles_a, triangles_b):
        raise ValueError("Magnet geometry differs")
    hashes.append(hashlib.sha256(triangles_a.tobytes()).hexdigest())
g = json.loads((source / "grid.json").read_text())
h = g["cell_m"]
desired_origin = np.array([-0.0006, -0.0006, 0])
offset = (desired_origin - g["origin_m"]) / h
if np.max(abs(offset - np.rint(offset))) > 1e-3:
    raise ValueError("Unaligned crop")
x, y, z = np.rint(offset).astype(int)
dims = np.array([24, 24, 120])
if min(x, y, z) < 0 or np.any(np.array([x, y, z]) + dims > g["dims"]):
    raise ValueError("Crop outside source")
nx, ny, nz = g["dims"]
fields = np.memmap(source / "em.bin", dtype="<f4", mode="r", shape=(nz, ny, nx, 16))
b = np.array(fields[z : z + 120, y : y + 24, x : x + 24, :3])
origin = (np.array(g["origin_m"]) + np.array([x, y, z]) * h).tolist()
out = root / "split-slit-reused-field-h0050"
out.mkdir(exist_ok=False)
np.save(out / "B.npy", b)
record = {
    "status": "Reused existing native applied field; sources checked identical apart from quadrature",
    "source_run": str(source),
    "source_project_sha256": hashlib.sha256(
        source_project_path.read_bytes()
    ).hexdigest(),
    "target_project_sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
    "magnet_triangle_sha256": hashes,
    "field_file_sha256": hashlib.sha256((out / "B.npy").read_bytes()).hexdigest(),
    "dims": dims.tolist(),
    "origin_m": origin,
    "cell_m": h,
    "source_subdivisions": [m["subdivisions"] for m in source_magnets],
    "queued_outlet_observer_subdivisions": [m["subdivisions"] for m in target_magnets],
    "By_range_T_over_crop": [float(b[..., 1].min()), float(b[..., 1].max())],
    "limits": [
        "This reuses the native prescribed-magnetization, unit-permeability applied-field model. It includes no eddy-current reaction, temperature feedback or nonlinear magnetic material.",
        "Source uses3subdivisions and queued outlet observer4. Compare overlapping samples before combining results; that overlap check alone does not prove whole-pool quadrature convergence.",
        "No material IDs or current field are reused: these must come from the new slit charge solution.",
        "Crop covers the initial liquid bore/slot region only, not a future spreading or moving meniscus.",
    ],
}
(out / "report.json").write_text(json.dumps(record, indent=2) + "\n")
print(out)
print("By range over crop:", record["By_range_T_over_crop"])
