"""Extrapolate accepted cold DC Joule fields over a prescribed pulse pair.

This is an initial-field adiabatic estimate, not a heat-transfer solution.
The driver budget supplies the exact integral of squared piecewise-linear current.
"""

import argparse
import json
from pathlib import Path

import numpy as np


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cold", type=Path)
    parser.add_argument("project", type=Path)
    parser.add_argument("driver", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    cold = json.loads((args.cold / "report.json").read_text())
    project = json.loads(args.project.read_text())
    drive = json.loads(args.driver.read_text())
    grid = json.loads((args.cold / "grid.json").read_text())
    ids = np.fromfile(args.cold / "material-ids.bin", dtype="<u4")
    fields = np.memmap(args.cold / "em.bin", dtype="<f4", mode="r").reshape(-1, 16)
    if len(fields) != len(ids) or len(ids) != np.prod(grid["dims"]):
        raise ValueError("Cold field/grid dimensions disagree")
    current = float(cold["current_A"])
    if not np.isfinite(current) or current == 0:
        raise ValueError("A finite nonzero cold current is required")
    inputs = drive["inputs"]
    i2dt = drive["rms_current_a"] ** 2 / inputs["frequency_hz"]
    equivalent_seconds = i2dt / current**2
    duration = inputs["positive_us"] * 1e-6 * 4 / 3 + 25e-6
    cell = grid["cell_m"] ** 3
    rows = []
    for material in project["materials"]:
        selected = ids == material["id"]
        if not np.any(selected):
            continue
        q = fields[selected, 11].astype(np.float64)
        if not np.all(np.isfinite(q)) or np.any(q < 0):
            raise ValueError("Invalid accepted Joule field")
        if not np.any(q):
            continue
        thermal = material["thermal"]
        capacity = material["density_kg_m3"] * thermal["heat_capacity_j_kg_k"]
        diffusivity = thermal["conductivity_w_m_k"] / capacity
        rise = q * equivalent_seconds / capacity
        global_index = int(np.flatnonzero(selected)[np.argmax(q)])
        nx, ny, _ = grid["dims"]
        xyz = np.array([global_index % nx, global_index // nx % ny, global_index // (nx * ny)])
        position = np.array(grid["origin_m"]) + (xyz + .5) * grid["cell_m"]
        rows.append({
            "material": material["name"],
            "material_id": material["id"],
            "pulse_energy_j": float(q.sum() * cell * equivalent_seconds),
            "average_power_w": float(q.sum() * cell * equivalent_seconds * inputs["frequency_hz"]),
            "maximum_initial_field_adiabatic_rise_k": float(rise.max()),
            "maximum_position_mm": (position * 1000).tolist(),
            "thermal_diffusion_length_over_pulse_pair_mm": float(np.sqrt(diffusivity * duration) * 1000),
            "volumetric_heat_capacity_j_m3_k": capacity,
            "thermal_conductivity_w_m_k": thermal["conductivity_w_m_k"],
        })
    result = {
        "inputs": {k: str(v) for k, v in vars(args).items() if k != "output"},
        "cold_current_a": current,
        "pulse_integral_i_squared_a2_s": i2dt,
        "materials": rows,
        "limits": [
            "Initial cold-field extrapolation: fixed geometry, conductivity, current paths and material properties; no thermal PDE or temperature-dependent resistance.",
            "No phase change, external contact heating, rod/lead losses or cooling. Local maxima depend on the voxel resolution.",
            "Diffusion length is sqrt(k*t/(rho*cp)), an order-of-magnitude scale, not a simulated temperature correction.",
            "Repetition power assumes identical electrical pulses, not demonstrated repeatable droplet formation.",
        ],
    }
    with args.output.open("x") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
