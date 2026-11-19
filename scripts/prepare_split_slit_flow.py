"""Prepare open-slit native flow inputs; does not launch or qualify a run."""

import argparse
import copy
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--case",
    choices=["alumina", "steel", "steel_touch", "alumina_touch"],
    default="alumina",
)
args = parser.parse_args()
source = Path("hardware/designs/split-slit-review")
output = Path("artifacts/hardware-research/cases")
base = json.loads((source / (args.case + "-observer.mhd.json")).read_text())
if not base["bodies"][9]["name"].startswith("Heated aluminium block"):
    raise ValueError("Unexpected body order; identify the heater before omitting it")
base["geometry"] = str((source / base["geometry"]).resolve())
rows = []
for cell, label in [(0.1, "h010"), (0.05, "h0050")]:
    project = copy.deepcopy(base)
    project["domain"].update(
        min_mm=[-3.8, -3.8, -6], max_mm=[3.8, 3.8, 10.5], cell_mm=cell
    )
    project["fluid"]["relative_tolerance"] = 1e-4
    # Preserve the complete source CAD for applied B; omit only the exterior
    # heater obstacle from the fluid/charge grid so gas can return around it.
    project["bodies"][9]["simulation"]["included"] = False
    for magnet in project["electromagnetics"]["magnets"]:
        magnet["subdivisions"] = 3
    path = output / ("split-slit-" + args.case + "-flow-" + label + ".mhd.json")
    if path.exists():
        if json.loads(path.read_text()) != project:
            raise ValueError("Existing flow input differs: " + str(path))
    else:
        path.write_text(json.dumps(project, indent=2) + "\n")
    rows.append(
        {
            "project": str(path),
            "cell_mm": cell,
            "nominal_cells_across_gap": 0.2 / cell,
            "suggested_first_probe": "Zero-current retention with gravity/surface tension, then a matched driven run only after electrical assessment. No result yet.",
        }
    )
record = {
    "status": "Prepared exploratory inputs only; no slit flow result",
    "case": args.case,
    "cases": rows,
    "limits": [
        "The full gap between plates is open. No artificial end plugs constrain wetting to the initial1mm bore footprint.",
        "Zero-current motion must be assessed before assigning any release to EM force. Check slot-wise spreading, detached components, mass conservation, boundary contact and gas connectivity.",
        "100/50um grids give only2/4cells across the gap; breakup and contact-line behavior are not converged by construction.",
        "Inherited neutral-wall treatment is not calibrated alumina/copper/solder wetting, pinning or oxide behavior. No experiment-specific retention guarantee follows.",
        "Exterior heater omitted from gas obstacles; gas-domain sensitivity remains open for this geometry. No thermal evolution, solidification or feed replenishment.",
        "conduction_flow supports body and x-ends terminal modes. Pass x-ends explicitly to resolve finite modeled copper resistance, as in the matched slit electrical screens. Outside leads remain separate.",
        "The initial liquid volume and hydraulic geometry differ from the circular nozzle. Its110A pulse is not transferred as a qualified operating point.",
    ],
}
path = output / ("split-slit-" + args.case + "-flow-preparation.json")
path.write_text(json.dumps(record, indent=2) + "\n")
print(path)
