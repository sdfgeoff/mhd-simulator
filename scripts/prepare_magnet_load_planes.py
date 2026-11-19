"""Native midplane field windows for Maxwell-stress pole attraction."""

import json
from pathlib import Path

source = Path(
    "artifacts/hardware-research/cases/m6-rooted08-hot-v1/hot-contact.mhd.json"
)
for x, z in [(25, 15), (40, 25)]:
    j = json.loads(source.read_text())
    j["geometry"] = str((source.parent / "hot-contact.gltf").resolve())
    j["domain"].update(
        min_mm=[-x, -0.125, 0.5 - z], max_mm=[x, 0.125, 0.5 + z], cell_mm=0.25
    )
    for magnet in j["electromagnetics"]["magnets"]:
        magnet["subdivisions"] = 4
    with Path(f"artifacts/hardware-research/m6-load-plane-{x}-{z}.mhd.json").open(
        "x"
    ) as out:
        json.dump(j, out, indent=2)
        out.write("\n")
