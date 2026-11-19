"""Collect thicker radiused-tip cold/current/thermal screens without merging scopes."""

import json, math
from pathlib import Path
import numpy as np

root = Path("artifacts/hardware-research")


def read(name, file="report.json"):
    return json.loads((root / name / file).read_text())


def force(run, observer=None):
    path = root / run
    g = read(run, "grid.json")
    nx, ny, nz = g["dims"]
    h = g["cell_m"]
    f = np.memmap(path / "em.bin", dtype="<f4", mode="r").reshape(nz, ny, nx, 16)
    ids = np.memmap(path / "material-ids.bin", dtype="<u4", mode="r").reshape(
        nz, ny, nx
    )
    if observer:
        o = read(observer)
        dx, dy, dz = o["dims"]
        offset = [(a - b) / h for a, b in zip(o["origin_m"], g["origin_m"])]
        if any(abs(v - round(v)) > 1e-4 for v in offset):
            raise ValueError("Grid alignment")
        x, y, z = map(round, offset)
        j = f[z : z + dz, y : y + dy, x : x + dx, 4:7].astype(float)
        subids = ids[z : z + dz, y : y + dy, x : x + dx]
        oid = np.fromfile(root / observer / "material-ids.bin", dtype="<u4").reshape(
            dz, dy, dx
        )
        if not np.array_equal(subids, oid):
            raise ValueError("Phase alignment")
        B = (
            np.fromfile(root / observer / "em.bin", dtype="<f4")
            .reshape(dz, dy, dx, 16)[..., :3]
            .astype(float)
        )
        return float(-np.cross(j, B)[..., 2][oid == 4].sum() * h**3)
    z = g["origin_m"][2] + (np.arange(nz) + 0.5) * h
    mask = (ids == 4) & (z[:, None, None] >= 0) & (z[:, None, None] < 0.0006)
    return float(-f[..., 10][mask].astype(float).sum() * h**3)


straight = "m6-rooted08-cold30-h0050"
hot = "m6-rooted08-hot-current30-h010"
fine = read(straight)
coarse = read(hot)
F = force(straight)
Fc = force(hot, "m6-hot-contact-field-observer-h010")
ref = json.loads(Path("hardware/m6-stub-drilled-screening.json").read_text())
oldhot = json.loads(Path("hardware/m6-hot-contact-current-screening.json").read_text())[
    "cases"
][1]
thermal = read("m6-rooted08-thermal110-h0050")
steady = json.loads(
    (root / "thermal-rooted08-hot-v1-weak-anchor-h010.json").read_text()
)
steady["method"] = (
    "Native scalar GPU FV: entire aluminium heater ID8 fixed+0.5K; outer copper planes fixed-0.5K; all steel/liquid/copper/mica unknown. Raw method sentence inherited old default-mode text; numerical output unchanged."
)
steady["limits"][0] = (
    "Steady anchors-mode screen: entire aluminium heater ID8 fixed-temperature reservoir; steel/liquid/copper/mica solved; effective sheet k represents contact-quality sensitivity."
)
for r in [fine, coarse]:
    if abs(r["power_relative_defect"]) > 1e-4:
        raise ValueError("Electrical power closure")
if (
    steady["heat_return_relative_error"] > 1e-4
    or abs(steady["power_identity_relative_error"]) > 1e-4
):
    raise ValueError("Thermal closure")
R = coarse["terminal_voltage_V"] / coarse["current_A"]
dose = 0.000226875 * 110**2
record = {
    "status": "Completed separate cold, steady thermal and one-shot thermal screens; no jetting result",
    "geometry": {
        "boss_mm": 0.8,
        "nominal_wall_mm": 0.2,
        "root_radius_mm": 0.1,
        "contact_height_mm": 0.35,
        "contact_top_mm": 0.45,
        "fillet_start_mm": 0.5,
        "bore_mm": 0.4,
        "land_mm": 0.6,
    },
    "fine50um_straight_fingers": {
        "source": str(root / straight),
        "outlet_force_N_at30A": F,
        "force_ratio_to_previous07": F / ref["outlet_force_N_at30A"],
        "peak_A_matching_previous93A_outlet_force": 93
        * ref["outlet_force_N_at30A"]
        / F,
        "charge_report": fine,
    },
    "coarse100um_full_hot_fixture": {
        "source": str(root / hot),
        "field_observer": "m6-hot-contact-field-observer-h010",
        "outlet_force_N_at30A": Fc,
        "force_ratio_times110A_to_previous07_at93A": Fc
        * 110
        / (oldhot["outlet_force_N_at30A"] * 93),
        "R_ohm_including_two50mm_1mm2_extensions": R,
        "voltage_V_at_comparison110A": R * 110,
        "average_Joule_W_at_assumed200Hz": R * dose * 200,
        "charge_report": coarse,
    },
    "comparison_waveform": {
        "peak_A": 110,
        "positive_us": 250,
        "ramp_us": 20,
        "reverse_fraction": 0.25,
        "reverse_us": 250 / 3,
        "gap_us": 25,
        "I2dt_A2s": dose,
        "RMS_A_at_assumed200Hz": math.sqrt(dose * 200),
    },
    "one_shot_tip_thermal50um": thermal,
    "stationary_heated_anchor_thermal100um": steady,
    "limits": [
        "Matched cold force is not matched droplet volume/speed. No rooted-tip moving-fluid result.",
        "Straight-finger fine charge and full-fixture coarse charge are different domains/geometries; comparisons at each grid are explicitly separate.",
        "Thermal models are separate: one pulse starts uniformly250C with ideal joints and insulated ROI; steady fixture omits Joule heat, feed and external losses. Do not add their temperatures.",
        "No concentricity, clamp preload, real joint resistance, material temperature feedback or operating lifetime qualification.",
    ],
    "tool_geometry_source": {
        "url": "https://www.ns-tool.com/en/products/detail/107",
        "verified": "2026-11-19",
        "scope": "Maker catalogue lists a1mm corner-radius cutter withR0.1 (08-00700-01001). Confirms nominal radius availability; not a machining process or tool-selection qualification.",
    },
}
with Path("hardware/m6-rooted08-screening.json").open("x") as out:
    json.dump(record, out, indent=2)
    out.write("\n")
print(
    "F fine",
    F,
    "F coarse",
    Fc,
    "R",
    R,
    "V110",
    R * 110,
    "P200",
    R * dose * 200,
    "coarse matched ratio",
    record["coarse100um_full_hot_fixture"]["force_ratio_times110A_to_previous07_at93A"],
)
