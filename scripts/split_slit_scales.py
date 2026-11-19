"""Ideal plug-flow scales for selecting slit experiments, not a jetting solver.

Integrating incompressible axial momentum in two constant-area sections gives
Delta p = rho * sum(length / area) * dQ/dt. With constant net pressure and Q(0)=0,
the displaced volume is Delta p * t^2 / (2 * hydraulic_inertance).
"""

import json
import math
from pathlib import Path

source = Path("hardware/designs/split-slit-review")
manifest = json.loads((source / "manifest.json").read_text())
case = manifest["cases"][0]
project = json.loads((source / "alumina-charge.mhd.json").read_text())
material = next(m for m in project["materials"] if m["id"] == 4)
rho = material["density_kg_m3"]
gamma = material["surface_tension_n_m"]
sigma = material["conductivity_s_m"]
gap = case["slit_gap_mm"] * 1e-3
length = case.get("initial_wetted_slit_length_mm", case.get("slit_max_length_mm")) * 1e-3
land = case["plate_thickness_mm"] * 1e-3
area = case["nominal_exit_area_mm2"] * 1e-6
bore_area = math.pi * (0.5e-3) ** 2
pool_length = (case["pool_top_z_mm"] - 0.5) * 1e-3
inertance = rho * (pool_length / bore_area + land / area)
hydrostatic = rho * 9.81 * case["pool_top_z_mm"] * 1e-3
capillary_scale = 2 * gamma * (1 / gap + 1 / length)
rows = []
for diameter in [0.4, 0.5, 0.6]:
    volume = math.pi / 6 * (diameter * 1e-3) ** 3
    for field in [0.25, 0.35, 0.5]:
        for current in [30, 60, 110]:
            drive_pressure = current * field / length
            for capillary in [0, capillary_scale]:
                net = drive_pressure + hydrostatic - capillary
                duration = math.sqrt(2 * volume * inertance / net) if net > 0 else None
                rows.append(
                    {
                        "diameter_mm": diameter,
                        "target_volume_nl": volume * 1e12,
                        "assumed_B_T": field,
                        "assumed_liquid_current_A": current,
                        "assumed_opposing_pressure_Pa": capillary,
                        "drive_pressure_scale_Pa": drive_pressure,
                        "constant_pressure_displacement_time_us": (
                            duration * 1e6 if duration else None
                        ),
                        "end_exit_velocity_scale_m_s": (
                            2 * volume / duration / area if duration else None
                        ),
                        "rectangular_pulse_RMS_A_at200Hz": (
                            current * math.sqrt(duration * 200)
                            if duration and duration < 0.005
                            else None
                        ),
                    }
                )
record = {
    "status": "Analytical experiment-selection scales only; no slit jetting or current threshold established",
    "derivation": [
        "Hydraulic inertance = rho*(pool_length/bore_area + land_length/exit_area), treating each liquid section as uniform plug flow.",
        "For a uniform transverse field and current entirely crossing a rectangular liquid slit, axial force=I*B*gap; dividing by gap*slit_length gives the pressure scale I*B/slit_length.",
        "Constant net pressure from rest displaces V=Delta_p*T^2/(2*inertance). This is displaced liquid, not a detached spherical drop.",
        "The two opposing-pressure cases are zero and the geometric scale2*gamma*(1/gap+1/length). Neither is a measured or guaranteed capillary threshold.",
    ],
    "inputs": {
        "material": material["name"],
        "density_kg_m3": rho,
        "surface_tension_N_m": gamma,
        "conductivity_S_m": sigma,
        "exit_area_mm2": area * 1e6,
        "pool_length_mm": pool_length * 1e3,
        "hydraulic_inertance_Pa_s2_m3": inertance,
        "hydrostatic_assisting_pressure_Pa": hydrostatic,
        "capillary_pressure_scale_Pa": capillary_scale,
        "ideal_rectangular_liquid_crossing_resistance_ohm": gap
        / (sigma * land * length),
    },
    "cases": rows,
    "limits": [
        "All chosen fields and liquid currents are hypothetical inputs, not measured or extracted from the queued native fields. Actual terminal current may bypass the liquid.",
        "Uniform current/field, plug flow and fixed capillary opposition omit entrance/exit added mass, viscous loss, evolving curvature, contact-line motion, wetting and oxide forces.",
        "The actual aperture has circular ends and can wet farther along the slot. Its area is used for inertia, while the force/curvature scales use the nominal rectangular dimensions.",
        "No pulse ramp, reverse lobe, satellite formation, replenishment, circuit compliance, heating or repetition stability is solved. RMS is rectangular-pulse arithmetic, not an operating recommendation.",
        "Inherited material values are fixed screening assumptions for SnCu; these calculations do not calibrate Sn60Pb40 or temperature dependence.",
    ],
}
path = Path("hardware/split-slit-experiment-scales.json")
path.write_text(json.dumps(record, indent=2) + "\n")
for row in rows:
    if (
        row["diameter_mm"] == 0.5
        and row["assumed_B_T"] == 0.35
        and row["assumed_opposing_pressure_Pa"] > 0
    ):
        print(row)
print(path)
