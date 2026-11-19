"""Prepare exploratory rooted-tip flow domains with explicit ambient air paths."""

import argparse, copy, json
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--output", type=Path, default=Path("artifacts/hardware-research/cases"))
a = p.parse_args()
source = Path("hardware/designs/rooted08-hot-contact-m6/hot-contact.mhd.json")
base = json.loads(source.read_text())
base["geometry"] = str((source.parent / base["geometry"]).resolve())
for label, h, vented, tol in [
    ("h010", 0.1, False, 1e-5),
    ("h0050", 0.05, False, 1e-5),
    ("vented-h010", 0.1, True, 1e-5),
    ("vented-tol1e4-h010", 0.1, True, 1e-4),
]:
    j = copy.deepcopy(base)
    j["domain"].update(
        min_mm=[-3.8, -8.5, -8] if vented else [-3.8, -3.8, -4],
        max_mm=[3.8, 8.5, 16.5] if vented else [3.8, 3.8, 10.5],
        cell_mm=h,
    )
    j["fluid"]["relative_tolerance"] = tol
    for magnet in j["electromagnetics"]["magnets"]:
        magnet["subdivisions"] = 3
    path = a.output / ("m6-rooted08-flow-" + label + ".mhd.json")
    if path.exists():
        if json.loads(path.read_text()) != j:
            raise ValueError("Existing case differs: " + str(path))
    else:
        path.write_text(json.dumps(j, indent=2) + "\n")
    print(path)
