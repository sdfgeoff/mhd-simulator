"""Compare translated source fields using the unchanged accepted static current."""

import json, subprocess
from pathlib import Path
import numpy as np
from draw_hardware_layouts import load

root = Path("artifacts/hardware-research")
source = root / "m6-rooted08-hot-current30-h010"
grid = json.loads((source / "grid.json").read_text())
nx, ny, nz = grid["dims"]
h = grid["cell_m"]
# Existing accepted charge field is reversibly archived; decoding is I/O, not a
# second numerical field implementation. All PDE/source fields are native GPU.
data = subprocess.check_output(["zstd", "-d", "-q", "-c", str(source / "em.bin.zst")])
fields = np.frombuffer(data, dtype="<f4").reshape(nz, ny, nx, 16)
ids = np.memmap(source / "material-ids.bin", dtype="<u4", mode="r").reshape(nz, ny, nx)
base, _ = load("hardware/designs/rooted08-hot-contact-m6/hot-contact.gltf")
final, _ = load("m6-carrier-review-wide-stops/assembly")
for i in [0, 1, 2, 3, 6, 7, 8]:
    name, a, _ = base[i]
    other, b, _ = final[i]
    if name != other or not np.array_equal(a, b):
        raise ValueError("Conductive-head geometry changed")
final_project = json.loads(
    (
        root / "cases/m6-carrier-review-wide-stops/assembly.inspection.mhd.json"
    ).read_text()
)
if any(b["simulation"]["included"] for b in final_project["bodies"][9:]):
    raise ValueError("Unexpected physics-enabled fixture")
rows = []
for name in ["m6-carrier-reference-q4-h010", "m6-carrier-review-field-q4-h010"]:
    run = root / name
    r = json.loads((run / "report.json").read_text())
    dx, dy, dz = r["dims"]
    offset = [(a - b) / h for a, b in zip(r["origin_m"], grid["origin_m"])]
    if any(abs(v - round(v)) > 1e-4 for v in offset) or abs(r["cell_m"] / h - 1) > 1e-7:
        raise ValueError("Grid mismatch")
    x, y, z = map(round, offset)
    phase = ids[z : z + dz, y : y + dy, x : x + dx]
    observed_ids = np.fromfile(run / "material-ids.bin", dtype="<u4").reshape(
        dz, dy, dx
    )
    if not np.array_equal(phase, observed_ids):
        raise ValueError("Phase changed in outlet window")
    B = (
        np.fromfile(run / "em.bin", dtype="<f4")
        .reshape(dz, dy, dx, 16)[..., :3]
        .astype(float)
    )
    J = fields[z : z + dz, y : y + dy, x : x + dx, 4:7].astype(float)
    mask = phase == 4
    force = -float(np.cross(J, B)[..., 2][mask].sum() * h**3)
    rows.append(
        {
            "source": str(run),
            "mean_liquid_By_T": float(B[..., 1][mask].mean()),
            "outlet_downward_force_N_at30A": force,
        }
    )
r = {
    "status": "Completed native applied-source translation check with fixed accepted J",
    "charge_source": str(source),
    "conductive_head_geometry_matches_accepted_charge": True,
    "final_review_new_fixtures_all_excluded": True,
    "cases": rows,
    "force_ratio_lowered_to_reference": rows[1]["outlet_downward_force_N_at30A"]
    / rows[0]["outlet_downward_force_N_at30A"],
    "matching_peak_A_to_previous110A_cold_force": 110
    * rows[0]["outlet_downward_force_N_at30A"]
    / rows[1]["outlet_downward_force_N_at30A"],
    "scope": "Both poles translated down0.2mm; nozzle/liquid/copper geometry and fixed properties unchanged. Static J at zero velocity is independent of imposed B; magnets remain electrically disconnected from the head. Native q4 applied fields sampled on the accepted100um grid; phase IDs aligned. Nonmagnetic carrier fixtures excluded from physics; no eddy response, hot recoil, moving fluid or jetting conclusion.",
}
with Path("hardware/m6-carrier-field-screening.json").open("w") as out:
    json.dump(r, out, indent=2)
    out.write("\n")
print(json.dumps(r, indent=2))
