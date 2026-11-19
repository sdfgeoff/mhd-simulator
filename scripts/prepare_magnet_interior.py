"""Prepare small native source-field observations inside one SmCo pole."""

import json
from pathlib import Path

p = Path("hardware/designs/hot-contact-m6/hot-contact.mhd.json")
for q in [3, 4]:
    project = json.loads(p.read_text())
    project["geometry"] = str((p.parent / "hot-contact.gltf").resolve())
    project["domain"].update(
        min_mm=[-12.5, 1.7, -2.0], max_mm=[12.5, 11.7, 3.0], cell_mm=0.5
    )
    for magnet in project["electromagnetics"]["magnets"]:
        magnet["subdivisions"] = q
    with Path(f"artifacts/hardware-research/m6-magnet-interior-q{q}.mhd.json").open(
        "x"
    ) as out:
        json.dump(project, out, indent=2)
        out.write("\n")
