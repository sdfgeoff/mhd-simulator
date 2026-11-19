"""Plot completed native unipolar cases; explicitly stationary, not jetting."""
import csv
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

from analyze_unipolar_coil import reaction_sample

root = Path("artifacts/hardware-research")
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--comparison", type=Path, default=root / "unipolar-coil-v2-comparison-partial.json")
parser.add_argument("--output", type=Path, default=root / "diagrams/unipolar-native-progress.png")
args = parser.parse_args()
record = json.loads(args.comparison.read_text())
fig, axes = plt.subplots(3, 1, figsize=(10, 9), sharex=True, constrained_layout=True)
end_us = 0.0
for case in record["cases"]:
    rows = list(csv.DictReader((Path(case["source"]) / "diagnostics.csv").open()))
    t = np.array([float(r["time_s"]) for r in rows])
    end_us = max(end_us, t[-1] * 1e6)
    current = np.array([float(r["coil_current_A"]) for r in rows])
    linkage = np.array([reaction_sample(r) for r in rows])
    resistance = case["round_wire_assumption"]["resistance_ohm"] + case["series_assumption"]["resistance_ohm"]
    inductance = record["air_inductance"]["winding"][-1]["inductance_h"] + case["series_assumption"]["inductance_H"]
    voltage = resistance * (current[1:] + current[:-1]) / 2 + inductance * np.diff(current) / np.diff(t) + np.diff(linkage) / np.diff(t)
    assert np.allclose([min(voltage), max(voltage)], case["loaded_terminal_interval_mean_voltage_range_V"])
    label = case["geometry"] + " / " + case["shape"]
    axes[0].plot(t * 1e6, current, label=label)
    axes[1].plot((t[1:] + t[:-1]) * .5e6, voltage)
    axes[2].plot(t * 1e6, [float(r["radial_pressure_proxy_Pa"]) / 1000 for r in rows])
axes[0].set_ylabel("Coil current (A)")
axes[1].set_ylabel("Terminal interval mean (V)")
axes[2].set_ylabel("Radial force moment / volume (kPa)\npositive inward")
axes[2].set_xlabel("Time (µs)")
axes[0].legend()
for ax in axes:
    ax.grid(alpha=.25)
    ax.axhline(0, color="black", linewidth=.6)
    ax.set_xlim(0, min(400, end_us))
fig.suptitle(f"Single positive current pulse — {record['completed_cases']}/{record['expected_cases']} stationary cases complete\n200 µm native EM screen; fixed metal geometry and properties", fontsize=14)
fig.supxlabel("Voltage includes assumed hot winding + 2 mΩ / 0.2 µH series path; excludes switching overshoot.\nRadial moment is not outlet pressure. No moving fluid or droplet prediction.", fontsize=10)
output = args.output
output.parent.mkdir(parents=True, exist_ok=True)
fig.savefig(output, dpi=160)
print(output)
