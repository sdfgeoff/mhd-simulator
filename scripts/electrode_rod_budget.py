"""Analytic dry-electrode resistance and heat scales; not a thermal simulation."""

import argparse
import json
import math
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("driver", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--diameter-mm", type=float, required=True)
    parser.add_argument("--dry-length-mm", type=float, required=True)
    parser.add_argument("--resistivity-ohm-m", type=float, default=1.25e-7)
    parser.add_argument("--thermal-conductivity-w-m-k", type=float, default=150.)
    parser.add_argument("--density-kg-m3", type=float, default=19300.)
    parser.add_argument("--heat-capacity-j-kg-k", type=float, default=140.)
    args = parser.parse_args()
    if any(not math.isfinite(v) or v <= 0 for v in vars(args).values() if isinstance(v, float)):
        raise ValueError("All physical inputs must be finite and positive")
    drive = json.loads(args.driver.read_text())
    area = math.pi * (args.diameter_mm * .001) ** 2 / 4
    length = args.dry_length_mm * .001
    resistance = args.resistivity_ohm_m * length / area
    rms = drive["rms_current_a"]
    i2dt = rms**2 / drive["inputs"]["frequency_hz"]
    q = args.resistivity_ohm_m * rms**2 / area**2
    report = {
        "inputs": {k: str(v) if isinstance(v, Path) else v for k, v in vars(args).items() if k != "output"},
        "rms_current_a": rms,
        "single_rod_resistance_ohm": resistance,
        "pair_average_joule_power_w": 2 * resistance * rms**2,
        "pair_peak_voltage_v": 2 * resistance * drive["inputs"]["peak_a"],
        "single_pulse_adiabatic_temperature_rise_k": args.resistivity_ohm_m * i2dt / (area**2 * args.density_kg_m3 * args.heat_capacity_j_kg_k),
        "steady_peak_rise_above_equal_fixed_end_temperatures_k": q * length**2 / (8 * args.thermal_conductivity_w_m_k),
        "limits": [
            "Uniform full current in each dry rod; excludes immersed-section current distribution and contact losses.",
            "Constant-property engineering assumptions; large predicted rises invalidate the approximation rather than qualify an operating temperature.",
            "Steady estimate solves k*T''+q=0 for equal fixed end temperatures; liquid and clamp cooling must actually maintain those boundaries. No radiation, gas convection or transient heat transfer.",
            "Identical repeated current pulses assumed; no demonstrated droplet repetition rate.",
        ],
    }
    with args.output.open("x") as output:
        json.dump(report, output, indent=2)
        output.write("\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
