"""Field trade for practical assembly gaps, reusing unchanged accepted static J."""

import json
from pathlib import Path
import numpy as np
from draw_hardware_layouts import load

root = Path("artifacts/hardware-research")
base, _ = load("m6-stub-v1/stub4")
variant, _ = load("m6-stub-clearance-v1/stub4")
for (n, a, _), (m, b, _) in zip(base[:4], variant[:4]):
    if n != m or not np.array_equal(a, b):
        raise ValueError("Conductive CAD changed")
source = root / "m6-stub-v1-stub4-cold30-h0050"
grid = json.loads((source / "grid.json").read_text())
nx, ny, nz = grid["dims"]
h = grid["cell_m"]
full = np.memmap(source / "em.bin", dtype="<f4", mode="r").reshape(nz, ny, nx, 16)
ids = np.memmap(source / "material-ids.bin", dtype="<u4", mode="r").reshape(nz, ny, nx)
rows = []
for name in ["m6-stub4-q4-observer", "m6-stub4-clearance-q4-observer"]:
    run = root / name
    r = json.loads((run / "report.json").read_text())
    offsets = [(a - b) / h for a, b in zip(r["origin_m"], grid["origin_m"])]
    if (
        any(abs(v - round(v)) > 1e-4 for v in offsets)
        or abs(r["cell_m"] / h - 1) > 1e-7
    ):
        raise ValueError("Grid mismatch")
    x, y, z = map(round, offsets)
    dx, dy, dz = r["dims"]
    sub = full[z : z + dz, y : y + dy, x : x + dx]
    observed_ids = np.fromfile(run / "material-ids.bin", dtype="<u4").reshape(
        dz, dy, dx
    )
    if not np.array_equal(observed_ids, ids[z : z + dz, y : y + dy, x : x + dx]):
        raise ValueError("Phase changed")
    obs = np.fromfile(run / "em.bin", dtype="<f4").reshape(dz, dy, dx, 16)
    mask = observed_ids == 4
    force = -float(
        np.cross(sub[..., 4:7].astype(float), obs[..., :3].astype(float))[..., 2][
            mask
        ].sum()
        * h**3
    )
    rows.append(
        {
            "observation": str(run),
            "mean_outlet_By_T": float(obs[..., 1][mask].mean()),
            "outlet_force_N_at30A": force,
        }
    )
ratio = rows[1]["outlet_force_N_at30A"] / rows[0]["outlet_force_N_at30A"]
record = {
    "status": "Completed source-field-only assembly-clearance comparison",
    "charge_source": str(source),
    "conductive_meshes_bit_identical": True,
    "cases": rows,
    "clearanced_geometry": {
        "pole_gap_mm": 3.4,
        "nozzle_face_clearance_mm": 0.3,
        "heater_top_clearance_mm": 1.0,
        "magnet_projection_mm": 2.0,
    },
    "clearance_force_ratio": ratio,
    "peak_A_matching_original90A_cold_force": 90 / ratio,
    "RMS_A_at_assumed200Hz_for_match": 90 / ratio * (0.000226875 * 200) ** 0.5,
    "scope": "Accepted fixed static J is unchanged by source positions at zero velocity and unit permeability. q4 applied-source windows aligned to accepted full charge grid. No contact, thermal, mount-eddy, demagnetization or moving-fluid solve.",
}
with Path("hardware/m6-stub-clearance-screening.json").open("x") as f:
    json.dump(record, f, indent=2)
    f.write("\n")
print(json.dumps(record, indent=2))
