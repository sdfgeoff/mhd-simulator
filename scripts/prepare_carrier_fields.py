"""Prepare native field windows for the final carrier's pole translation."""

import argparse, json
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument(
    "--carrier-project",
    type=Path,
    default=Path(
        "artifacts/hardware-research/cases/m6-carrier-review/assembly.inspection.mhd.json"
    ),
)
args = p.parse_args()
for label, source in [
    (
        "reference",
        Path("hardware/designs/rooted08-hot-contact-m6/hot-contact.mhd.json"),
    ),
    ("review", args.carrier_project),
]:
    j = json.loads(source.read_text())
    j["geometry"] = str((source.parent / j["geometry"]).resolve())
    j["domain"].update(min_mm=[-0.3, -0.3, 0.0], max_mm=[0.3, 0.3, 0.6], cell_mm=0.1)
    for magnet in j["electromagnetics"]["magnets"]:
        magnet["subdivisions"] = 4
    output = Path(f"artifacts/hardware-research/carrier-{label}-observer.mhd.json")
    with output.open("x") as f:
        json.dump(j, f, indent=2)
        f.write("\n")
