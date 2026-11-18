"""Prepare charge-only full-domain and tiny applied-field jobs to avoid N*M work.

Static J at zero velocity is independent of the imposed field. Each charge solve
retains the complete conductor, while the field observation sees full source CAD.
"""

import json
from pathlib import Path

sources = [
    Path(
        "artifacts/hardware-research/cases/m6-hot-contact-wide-v1/hot-contact.mhd.json"
    ),
    Path(
        "artifacts/hardware-research/cases/m6-stub-drilled-v1/stub4-thermal-h010.mhd.json"
    ),
]
for p in sources:
    data = json.loads(p.read_text())
    data["electromagnetics"]["magnets"] = []
    with p.with_name(p.stem + "-current-only.mhd.json").open("x") as out:
        json.dump(data, out, indent=2)
        out.write("\n")
p = sources[0]
data = json.loads(p.read_text())
data["domain"].update(min_mm=[-0.3, -0.3, 0.0], max_mm=[0.3, 0.3, 0.6])
with p.with_name("hot-contact-field-observer.mhd.json").open("x") as out:
    json.dump(data, out, indent=2)
    out.write("\n")
