"""Add grid controls and an explicitly conditional strip-preload calculation."""

import json, math
from pathlib import Path
import numpy as np

root = Path("artifacts/hardware-research")
path = Path("hardware/m6-dogbone-strip-screening.json")
r = json.loads(path.read_text())
coarse = json.loads((root / "dogbone-short-h010-screening.json").read_text())
long_fine = json.loads((root / "m6-dogbone-charge30-h0050/report.json").read_text())
r["coarse100um_control"] = coarse
r["force_relative_change_50um_to100um"] = (
    r["outlet_force_N_at30A"] / coarse["outlet_force_N_at30A"] - 1
)
r["R_relative_change_50um_to100um"] = (
    r["cropped_terminal_R_ohm"] / coarse["cropped_terminal_R_ohm"] - 1
)
r["resistance_ratio_short_to_long_waist_50um"] = r["cropped_terminal_R_ohm"] / (
    long_fine["terminal_voltage_V"] / long_fine["current_A"]
)
r["rejected_tighter_solve"] = {
    "requested_tolerance": 5e-7,
    "iterations": 4000,
    "true_residual": 1.4574412e-6,
    "case": "long-neck50um",
    "disposition": "Not accepted. Accepted2e-6 result has0.039%current imbalance. These screens use an explicit0.1%conservation criterion, not the earlier0.01%criterion; mesh-force change is6.7%, much larger. No sub-percent performance claim.",
}
r["thermal_setup_failure"] = {
    "dispatch_workgroups_x": 66031,
    "adapter_limit": 65535,
    "remedy": "Thermal descriptor Ypadding5.5→5.4mm retains all unknown Cu/steel/liquid/mica; cropped heater is an explicitly fixed reservoir. No physics kernel changed.",
}
x = np.linspace(0, 3.2, 100001)
width = np.where(x <= 0.4, 0.8, np.where(x < 1.4, 0.8 + (x - 0.4) * 3.2, 4))
inertia = width * 0.5**3 / 12
E = 193000.0
area = 0.8**2 - math.pi * 0.2**2
r["conditional_seat_preload_screen"] = {
    "assumptions": "Simply supported SS strip at±3.2mm under central point load. Actual dogbone width profile,0.5mm thickness. Room-temperature E193GPa is illustrative, not a hot allowable. No joint/contact/FEM qualification.",
    "max_bending_stress_MPa_per_N": 3.2 * 0.5 / (4 * (0.8 * 0.5**3 / 12)),
    "centre_deflection_mm_per_N": float(
        np.trapezoid((3.2 - x) ** 2 / (2 * E * inertia), x)
    ),
    "nominal_sealing_land_mm2": area,
    "nominal_seal_pressure_MPa_per_N": 1 / area,
    "interpretation": "Control seat preload.10N gives about480MPa nominal bending stress in this model. Do not infer a torque or leak-tightness. Contact screws clamp tabs, not the nozzle seat.",
}
path.write_text(json.dumps(r, indent=2) + "\n")
print(
    json.dumps(
        {
            k: r[k]
            for k in [
                "force_relative_change_50um_to100um",
                "resistance_ratio_short_to_long_waist_50um",
                "conditional_seat_preload_screen",
            ]
        },
        indent=2,
    )
)
