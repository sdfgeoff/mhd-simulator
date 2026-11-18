"""Prepare tiny applied-field windows without cropping the charge solve."""

import argparse, json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "project",
    nargs="?",
    type=Path,
    default=Path("artifacts/hardware-research/cases/m6-stub-v1/stub4.mhd.json"),
)
source = parser.parse_args().project
for q in [3, 4]:
    project = json.loads(source.read_text())
    project["domain"].update(min_mm=[-0.25, -0.25, 0.0], max_mm=[0.25, 0.25, 0.6])
    for magnet in project["electromagnetics"]["magnets"]:
        magnet["subdivisions"] = q
    with source.with_name(f"stub4-observer-q{q}.mhd.json").open("x") as out:
        json.dump(project, out, indent=2)
        out.write("\n")
