"""Lumped electrical screening, not a droplet or shielding prediction.

Wheeler (1928) air-coil estimates; exact constant-RL bridge rise/fall.
The pressure column is the low-induction, long-cylinder radial-pressure
proxy sigma*R_liquid^2*B*dB/dt/4. Full 3D checks must replace it.
"""

import argparse
import csv
import itertools
import json
import math
from pathlib import Path

import numpy as np

MU0 = 4e-7 * math.pi
CU_RHO20 = 1.724e-8
CU_ALPHA = 0.00393


def coil(inner_mm, length_mm, wire_mm, layers, temperature_c):
    pitch = wire_mm * 1.08  # assumed enamel/packing allowance
    per_layer = int((length_mm - wire_mm) / pitch)
    if per_layer < 1:
        return None
    n = per_layer * layers
    radii = [(inner_mm + (j + 0.5) * pitch) * 1e-3 for j in range(layers)]
    axial = [(j + 0.5) * pitch * 1e-3 - length_mm * 0.5e-3 for j in range(per_layer)]
    wire_length = sum(2 * math.pi * r * per_layer for r in radii) + 0.10
    area = math.pi * (wire_mm * 0.5e-3) ** 2
    resistance = CU_RHO20 * (1 + CU_ALPHA * (temperature_c - 20)) * wire_length / area
    a, b, c = (
        (inner_mm + layers * pitch / 2) / 25.4,
        length_mm / 25.4,
        layers * pitch / 25.4,
    )
    inductance = 0.8 * a * a * n * n / (6 * a + 9 * b + 10 * c) * 1e-6
    # Exact centre field of discrete circular filament turns. Leads/helix,
    # conductor reaction and radial field variation are absent here.
    b_per_amp = sum(
        MU0 * r * r / (2 * (r * r + z * z) ** 1.5) for r in radii for z in axial
    )
    return {
        "turns": n,
        "layers": layers,
        "turns_per_layer": per_layer,
        "inner_radius_mm": inner_mm,
        "length_mm": length_mm,
        "wire_diameter_mm": wire_mm,
        "outer_diameter_mm": 2 * (inner_mm + layers * pitch),
        "wire_length_m": wire_length,
        "copper_temperature_c": temperature_c,
        "coil_r_ohm": resistance,
        "air_l_h": inductance,
        "air_b_per_amp_t": b_per_amp,
    }


def pulse(
    c, voltage, peak_a, frequency_hz, liquid_radius_mm, gamma=0.5, orifice_mm=0.5
):
    # Two-milliohm illustrative bridge/bus series resistance, not a MOSFET BOM.
    r, l = c["coil_r_ohm"] + 0.002, c["air_l_h"]
    if peak_a * r >= voltage:
        return None
    rise = -l / r * math.log1p(-peak_a * r / voltage)
    fall = l / r * math.log1p(peak_a * r / voltage)
    if (rise + fall) * frequency_hz >= 1:
        return None
    tr = np.linspace(0, rise, 513)
    tf = np.linspace(0, fall, 513)
    ir = voltage / r * (-np.expm1(-tr * r / l))
    iff = (peak_a + voltage / r) * np.exp(-tf * r / l) - voltage / r
    integrate = np.trapezoid if hasattr(np, "trapezoid") else np.trapz
    i2dt = float(integrate(ir * ir, tr) + integrate(iff * iff, tf))
    # Energy closure checks bridge excitation against RL storage and dissipation.
    input_j = float(voltage * (integrate(ir, tr) - integrate(iff, tf)))
    assert abs(input_j - r * i2dt) < 2e-5 * max(r * i2dt, 1e-12)
    b = c["air_b_per_amp_t"] * peak_a
    sigma, radius = 2.2e6, liquid_radius_mm * 1e-3
    pressure = (
        sigma
        * radius
        * radius
        * c["air_b_per_amp_t"] ** 2
        * ir
        * (voltage - r * ir)
        / (4 * l)
    )
    capillary = 4 * gamma / (orifice_mm * 1e-3)
    return {
        **c,
        "bus_v": voltage,
        "peak_a": peak_a,
        "frequency_hz": frequency_hz,
        "liquid_radius_mm": liquid_radius_mm,
        "rise_us": rise * 1e6,
        "fall_us": fall * 1e6,
        "air_b_peak_t": b,
        "copper_power_w": c["coil_r_ohm"] * i2dt * frequency_hz,
        "bridge_bus_power_w": 0.002 * i2dt * frequency_hz,
        "peak_magnetic_energy_j": 0.5 * l * peak_a * peak_a,
        "rms_a": math.sqrt(i2dt * frequency_hz),
        "idealized_pinch_peak_pa": float(max(pressure)),
        "idealized_push_impulse_pa_s": sigma * radius * radius * b * b / 8,
        "idealized_excess_impulse_pa_s": float(
            integrate(np.maximum(pressure - capillary, 0), tr)
        ),
        "hemispherical_capillary_pa": capillary,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", type=Path, default=Path("artifacts/hardware-research/lumped")
    )
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    rows = []
    for liquid_r, inner_r, length, wire, layers, temp, v, amps, hz in itertools.product(
        [1.0, 1.6, 3.0],
        [3.3, 4.3],
        [3.0, 6.0, 10.0, 14.0],
        [0.6, 1.0, 1.4, 2.0],
        [1, 2, 3, 4, 6, 8],
        [100.0, 180.0],
        [24.0, 48.0],
        [100.0, 150.0, 200.0, 250.0],
        [200.0, 500.0],
    ):
        if inner_r < liquid_r + 0.7:
            continue
        c = coil(inner_r, length, wire, layers, temp)
        if c is None or c["outer_diameter_mm"] > 32:
            continue
        row = pulse(c, v, amps, hz, liquid_r)
        if row:
            rows.append(row)
    with (args.output / "screen.csv").open("w", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=rows[0].keys())
        writer.writeheader()
        writer.writerows(rows)
    # Screening filters are explicit preferences, not established operating limits.
    shortlist = [
        r
        for r in rows
        if r["copper_power_w"] < 100 and r["peak_a"] <= 200 and r["bus_v"] == 48
    ]
    shortlist.sort(key=lambda r: r["idealized_excess_impulse_pa_s"], reverse=True)
    (args.output / "shortlist.json").write_text(json.dumps(shortlist[:40], indent=2))
    print(
        f"{len(rows)} electrically reachable rows; {len(shortlist)} pass screening preferences"
    )
    for row in shortlist[:5]:
        print(
            json.dumps(
                {
                    k: row[k]
                    for k in [
                        "liquid_radius_mm",
                        "inner_radius_mm",
                        "turns",
                        "wire_diameter_mm",
                        "length_mm",
                        "outer_diameter_mm",
                        "peak_a",
                        "frequency_hz",
                        "rise_us",
                        "air_b_peak_t",
                        "copper_power_w",
                        "idealized_excess_impulse_pa_s",
                    ]
                }
            )
        )


if __name__ == "__main__":
    main()
