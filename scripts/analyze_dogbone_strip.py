"""Compose native fixed-current and applied-field observations for the full strip."""

import argparse
import json
from pathlib import Path
import numpy as np
from native_field_io import read_native_fields

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--observer", default="m6-dogbone-observer-q4-h0050")
parser.add_argument("--charge", default="m6-dogbone-charge30-h0050")
parser.add_argument(
    "--output", type=Path, default=Path("hardware/m6-dogbone-strip-screening.json")
)
parser.add_argument("--thermal", default="thermal-dogbone-anchors-h010.json")
args = parser.parse_args()
root = Path("artifacts/hardware-research")
charge = root / args.charge
observer = root / args.observer
r = json.loads((charge / "report.json").read_text())
g = json.loads((charge / "grid.json").read_text())
o = json.loads((observer / "report.json").read_text())
nx, ny, nz = g["dims"]
dx, dy, dz = o["dims"]
h = g["cell_m"]
offset = [(a - b) / h for a, b in zip(o["origin_m"], g["origin_m"])]
if any(abs(x - round(x)) > 1e-4 for x in offset) or abs(o["cell_m"] / h - 1) > 1e-7:
    raise ValueError("Grid mismatch")
x, y, z = map(round, offset)
f = read_native_fields(charge / "em.bin", (nz, ny, nx, 16))
ids = np.memmap(charge / "material-ids.bin", mode="r", dtype="<u4").reshape(nz, ny, nx)
roi = ids[z : z + dz, y : y + dy, x : x + dx]
oid = np.fromfile(observer / "material-ids.bin", dtype="<u4").reshape(dz, dy, dx)
if not np.array_equal(roi, oid):
    raise ValueError("Phase mismatch")
b = read_native_fields(observer / "em.bin", (dz, dy, dx, 16))[..., :3].astype(float)
j = f[z : z + dz, y : y + dy, x : x + dx, 4:7].astype(float)
mask = oid == 4
force = -float(np.cross(j, b)[..., 2][mask].sum() * h**3)
balance_error = abs(r["return_current_A"] + r["current_A"]) / r["current_A"]
if (
    abs(r["power_relative_defect"]) > 1e-3
    or balance_error > 1e-3
    or r["true_relative_residual"] > 2.1e-6
):
    raise ValueError("Screen exceeds0.1% conservation or2.1e-6 residual gate")
reference = json.loads(Path("hardware/m6-rooted08-screening.json").read_text())[
    "fine50um_straight_fingers"
]["outlet_force_N_at30A"]
matching = 110 * reference / force
record = {
    "status": "Exploratory native full-strip screen; no seal, waveform or jetting qualification",
    "meets_previous_001_percent_conservation_gate": abs(r["power_relative_defect"])
    <= 1e-4
    and balance_error <= 1e-4,
    "current_balance_relative_error": balance_error,
    "conservation_gate_for_this_screen": 0.001,
    "previous_strict_gate": 0.0001,
    "outlet_force_N_at30A": force,
    "outlet_mean_liquid_By_T": float(b[..., 1][mask].mean()),
    "cold_force_ratio_to_rooted_integral_tip": force / reference,
    "peak_A_matching_rooted110A_cold_force": matching,
    "assumed_RMS_A_at200Hz_same_pulse_shape": matching * (0.000226875 * 200) ** 0.5,
    "cropped_terminal_R_ohm": r["terminal_voltage_V"] / r["current_A"],
    "charge_report": r,
    "scope": "Full16mm stainless strip with0.8mm waist andboltedCu contacts. Polesgap4.6mm,0.2mm lower. Native J computed in cropped20mm electricaldomain; outer anchors/leads omitted from its resistance. Nativeq4 B fromcompletePMCAD, composedonly onalignedliquidROI0..0.6mm. Fixedproperties,idealjoints,zero velocity, no hot recoil,currentfeedback,sealorwaveformqualification.",
}
thermal = root / args.thermal
if thermal.exists():
    t = json.loads(thermal.read_text())
    if (
        abs(t["power_identity_relative_error"]) > 1e-3
        or t["heat_return_relative_error"] > 1e-3
    ):
        raise ValueError("Thermal closure failed")
    record["separate_stationary_thermal"] = t
args.output.write_text(json.dumps(record, indent=2) + "\n")
print(
    json.dumps(
        {
            k: v
            for k, v in record.items()
            if k not in ["charge_report", "separate_stationary_thermal"]
        },
        indent=2,
    )
)
