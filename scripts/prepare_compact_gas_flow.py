"""Prepare a far-gas geometry comparison; use fine grid only after coarse validation.

Omitting the exterior heater is an approximation to the gas return path, not a
new hardware design. The full vented run is the comparison reference.
"""

import argparse, copy, json
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--output", type=Path, default=Path("artifacts/hardware-research/cases"))
a = p.parse_args()
source = Path("hardware/designs/rooted08-hot-contact-m6/hot-contact.mhd.json")
base = json.loads(source.read_text())
base["geometry"] = str((source.parent / base["geometry"]).resolve())
for h, label in [(0.1, "h010"), (0.05, "h0050")]:
    j = copy.deepcopy(base)
    j["domain"].update(min_mm=[-3.8, -3.8, -8], max_mm=[3.8, 3.8, 10.5], cell_mm=h)
    j["fluid"]["relative_tolerance"] = 1e-4
    j["bodies"][8]["simulation"]["included"] = False
    for magnet in j["electromagnetics"]["magnets"]:
        magnet["subdivisions"] = 3
    path = a.output / ("m6-rooted08-flow-compactgas-" + label + ".mhd.json")
    if path.exists():
        if json.loads(path.read_text()) != j:
            raise ValueError("Existing comparison differs")
    else:
        path.write_text(json.dumps(j, indent=2) + "\n")
    print(path)
