"""Full-conductor static J with a tiny native B observation, at matched coarse grids."""

import json
from pathlib import Path
import numpy as np

root = Path("artifacts/hardware-research")
obs = root / "m6-hot-contact-field-observer-h010"
bg = json.loads((obs / "report.json").read_text())
dx, dy, dz = bg["dims"]
b = (
    np.fromfile(obs / "em.bin", dtype="<f4")
    .reshape(dz, dy, dx, 16)[..., :3]
    .astype(float)
)
bids = np.fromfile(obs / "material-ids.bin", dtype="<u4").reshape(dz, dy, dx)
mask = bids == 4
rows = []
for name in [
    "m6-stub-drilled-current30-h010",
    "m6-hot-contact-wide-current30-tol4e6-h010",
    "m6-hot-contact-wide-current30-tol8e6-h010",
]:
    run = root / name
    r = json.loads((run / "report.json").read_text())
    g = json.loads((run / "grid.json").read_text())
    nx, ny, nz = g["dims"]
    h = g["cell_m"]
    offsets = [(a - c) / h for a, c in zip(bg["origin_m"], g["origin_m"])]
    if (
        any(abs(v - round(v)) > 1e-4 for v in offsets)
        or abs(bg["cell_m"] / h - 1) > 1e-7
    ):
        raise ValueError("Unaligned field/charge grids")
    x, y, z = map(round, offsets)
    ids = np.memmap(run / "material-ids.bin", dtype="<u4", mode="r").reshape(
        nz, ny, nx
    )[z : z + dz, y : y + dy, x : x + dx]
    if not np.array_equal(ids, bids):
        raise ValueError("Phase mismatch")
    j = (
        np.memmap(run / "em.bin", dtype="<f4", mode="r")
        .reshape(nz, ny, nx, 16)[z : z + dz, y : y + dy, x : x + dx, 4:7]
        .astype(float)
    )
    force = -float(np.cross(j, b)[..., 2][mask].sum() * h**3)
    if abs(r["power_relative_defect"]) > 1e-4:
        raise ValueError("Charge energy closure failed")
    rows.append(
        {
            "source": str(run),
            "outlet_force_N_at30A": force,
            "R_ohm": r["terminal_voltage_V"] / r["current_A"],
            "requested_relative_tolerance": r["requested_relative_tolerance"],
            "true_relative_residual": r["true_relative_residual"],
            "power_relative_defect": r["power_relative_defect"],
            "return_current_relative_error": abs(r["return_current_A"] + r["current_A"])
            / r["current_A"],
        }
    )
record = {
    "status": "Completed native full-conductor charge and native tiny-field composition; no moving-liquid result",
    "cases": rows,
    "force_ratio_hot_anchor_to_straight_contact": rows[1]["outlet_force_N_at30A"]
    / rows[0]["outlet_force_N_at30A"],
    "tolerance8e6_to4e6_force_relative_change": rows[2]["outlet_force_N_at30A"]
    / rows[1]["outlet_force_N_at30A"]
    - 1,
    "tolerance8e6_to4e6_R_relative_change": rows[2]["R_ohm"] / rows[1]["R_ohm"] - 1,
    "scope": "100um conductor geometry for both cases; fixed source quadratureq3; allzero-velocity current paths retained. B computed from full magnet CAD at a small matching native window because full-domain N*M direct summation exceeded work budget. JxB postprocessing is not another PDE solver. Real joints, temperature feedback, driver and fluid motion omitted.",
    "rejected_charge_requests": [
        {"tolerance": 1e-6, "iterations": 4000, "true_relative_residual": 1.8074712e-6},
        {"tolerance": 2e-6, "iterations": 4000, "true_relative_residual": 2.0390498e-6},
    ],
    "note": "Long thin Cu extensions expose f32/conditioning limits; tighter failures are not accepted results. Coarse current composition checked using closure and4e-6/8e-6 sensitivity, not represented as a fine-grid qualification.",
}
with Path("hardware/m6-hot-contact-current-screening.json").open("x") as out:
    json.dump(record, out, indent=2)
    out.write("\n")
print(json.dumps(record, indent=2))
