"""Lumped current-driver compliance/heat budget, not a switch-level design.

Uses the same push/retract timing as conduction_flow. R and L are explicit
engineering assumptions; the head resistance should come from a cold screen.
"""

import argparse
import json
import math
from pathlib import Path


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in [
        "peak-a",
        "positive-us",
        "head-mohm",
        "parasitic-mohm",
        "series-uh",
        "frequency-hz",
        "bus-v",
        "bulk-uf",
    ]:
        p.add_argument("--" + name, type=float, required=True)
    p.add_argument("--retract-fraction", type=float, default=0.25)
    p.add_argument("--retract-delay-us", type=float, default=25.0)
    p.add_argument("--rise-us", type=float, default=20.0)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    values = vars(a)
    if (
        any(
            not math.isfinite(v) or v < 0
            for v in values.values()
            if isinstance(v, float)
        )
        or min(a.peak_a, a.positive_us, a.frequency_hz, a.bus_v, a.bulk_uf) <= 0
    ):
        raise ValueError(
            "Choose finite nonnegative parameters and a positive pulse, frequency, bus and capacitance"
        )
    duration = a.positive_us * 1e-6
    rise = min(a.rise_us * 1e-6, duration / 4)
    if rise <= 0:
        raise ValueError("Rise time must be positive")
    segments = []
    for peak, t in [
        (a.peak_a, duration),
        (-a.peak_a * a.retract_fraction, duration / 3),
    ]:
        segments.extend(
            [(rise, 0.0, peak), (t - 2 * rise, peak, peak), (rise, peak, 0.0)]
        )
    occupied = duration * 4 / 3 + a.retract_delay_us * 1e-6
    if occupied * a.frequency_hz >= 1:
        raise ValueError("Pulse pair overlaps the next period")
    r_head, r_other = a.head_mohm * 0.001, a.parasitic_mohm * 0.001
    inductance = a.series_uh * 1e-6
    integral_i2 = sum(dt * (lo * lo + lo * hi + hi * hi) / 3 for dt, lo, hi in segments)
    required_v = [
        i * (r_head + r_other) + inductance * (hi - lo) / dt
        for dt, lo, hi in segments
        for i in [lo, hi]
    ]
    energy = 0.5 * inductance * a.peak_a**2
    result = {
        "inputs": {k: v for k, v in values.items() if k != "output"},
        "method": "Exact integrals of piecewise-linear prescribed current; V=IR+L dI/dt. Head modeled as constant cold resistance. Four-quadrant current regulation assumed.",
        "rms_current_a": math.sqrt(integral_i2 * a.frequency_hz),
        "head_heat_w": r_head * integral_i2 * a.frequency_hz,
        "parasitic_heat_w": r_other * integral_i2 * a.frequency_hz,
        "required_voltage_min_v": min(required_v),
        "required_voltage_max_v": max(required_v),
        "ideal_bus_compliance": max(map(abs, required_v)) <= a.bus_v,
        "peak_inductor_energy_j": energy,
        "bulk_rise_if_all_peak_inductor_energy_returns_v": math.sqrt(
            a.bus_v**2 + 2 * energy / (a.bulk_uf * 1e-6)
        )
        - a.bus_v,
        "limitations": [
            "No PWM ripple, switching/gate losses, current-loop dynamics, bus/lead ringing or protection design.",
            "No capacitor ESR/ripple rating or switch SOA/temperature qualification.",
            "Input R and L assumptions must be replaced with measured assembled values; motional voltage is omitted from this lumped check.",
            "Electrical compliance is not a droplet or repetition-rate prediction.",
        ],
    }
    with a.output.open("x") as f:
        json.dump(result, f, indent=2)
        f.write("\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
