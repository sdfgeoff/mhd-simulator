"""Create a distinct prescribed-current carrier trial from generated coil CAD.

This does not solve a circuit. Use fit_carrier_circuit.py with the computed
reaction linkage and independently estimated air inductance to check a drive.
"""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("project", type=Path)
    p.add_argument("output", type=Path)
    p.add_argument("--carrier-hz", type=float, default=50000.0)
    p.add_argument("--peak-a", type=float, default=200.0)
    p.add_argument("--burst-us", type=float, default=300.0)
    p.add_argument("--ramp-us", type=float, default=40.0)
    p.add_argument("--dt-us", type=float, default=0.5)
    p.add_argument("--domain-radius-mm", type=float, default=9.0)
    p.add_argument("--domain-top-mm", type=float, default=12.0)
    p.add_argument("--fluid", action="store_true")
    a = p.parse_args()
    for key, value in vars(a).items():
        if isinstance(value, float) and (not math.isfinite(value) or value <= 0):
            raise ValueError(f"{key} must be finite and positive")
    period_count = a.carrier_hz * a.burst_us * 1e-6
    if (
        abs(period_count - round(period_count)) > 1e-8
        or a.ramp_us * 2 > a.burst_us
        or a.dt_us * a.carrier_hz * 1e-6 > 0.05
    ):
        raise ValueError(
            "Choose an integer number of carrier periods, at least 20 samples/period, and nonoverlapping symmetric ramps"
        )
    data = json.loads(a.project.read_text())
    if (
        len(data["electromagnetics"]["wires"]) != 1
        or data["electromagnetics"]["magnets"]
        or data["electromagnetics"]["uniform"]
    ):
        raise ValueError("This screen expects one coil without other field sources")
    burst, ramp, dt = a.burst_us * 1e-6, a.ramp_us * 1e-6, a.dt_us * 1e-6
    count = round(burst / dt)
    samples = []
    for i in range(count + 1):
        t = burst * i / count
        e = max(0.0, min(t / ramp, (burst - t) / ramp, 1.0))
        samples.append([t, a.peak_a * e * math.sin(2 * math.pi * a.carrier_hz * t)])
    samples.append([burst + 200e-6, 0.0])
    data["electromagnetics"]["wires"][0]["current"] = {
        "kind": "table",
        "samples": samples,
    }
    data["run"].update(
        dt_s=dt, fluid_enabled=a.fluid, thermal_enabled=False, prescribed_jet=None
    )
    data["fluid"].update(dt_s=dt, surface={"enabled": a.fluid})
    data["domain"].update(
        min_mm=[-a.domain_radius_mm, -a.domain_radius_mm, -2.0],
        max_mm=[a.domain_radius_mm, a.domain_radius_mm, a.domain_top_mm],
    )
    data["geometry"] = os.path.relpath(
        (a.project.parent / data["geometry"]).resolve(), a.output.parent.resolve()
    )
    a.output.parent.mkdir(parents=True, exist_ok=True)
    with a.output.open("x") as f:
        json.dump(data, f, indent=2)
        f.write("\n")
    metadata = {
        "configuration": {
            k: str(v) if isinstance(v, Path) else v for k, v in vars(a).items()
        },
        "source_sha256": hashlib.sha256(a.project.read_bytes()).hexdigest(),
        "limits": [
            "Prescribed current, not a circuit solution. Base project RL pulse estimates do not describe this carrier waveform.",
            "A resonant drive may have internal AC voltages above the DC supply. Check computed loaded voltage and capacitor current/voltage before judging feasibility.",
            "Thermal evolution, coil AC resistance/cooling, driver switching and capacitor losses unresolved.",
            "Cold force diagnostics are not an outlet pressure or jetting result.",
        ],
    }
    a.output.with_name(a.output.stem + "-carrier-assumptions.json").write_text(
        json.dumps(metadata, indent=2) + "\n"
    )
    print(a.output)


if __name__ == "__main__":
    main()
