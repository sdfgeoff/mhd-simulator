"""Prepare heater-on/off DC shunting controls over the complete heater envelope."""
import copy
import hashlib
import json
from pathlib import Path

root = Path("artifacts/hardware-research")
source = root / "cases/m6-rooted08-hot-v1/hot-contact-current-only.mhd.json"
base = json.loads(source.read_text())
base["geometry"] = str((source.parent / base["geometry"]).resolve())
folder = root / "cases/rooted-heater-dc-v1"
folder.mkdir(parents=True, exist_ok=False)
jobs = []
for included in [False, True]:
    p = copy.deepcopy(base)
    # Full heater is x±11.5,y±8,z4..15.5mm. Existing full-lead screen
    # clipped its y/z extent. Equipotential Cu bodies allow a compact lead crop.
    p["domain"].update(min_mm=[-12, -8.5, -.5], max_mm=[12, 8.5, 16], cell_mm=.1)
    p["bodies"][8]["simulation"]["included"] = included
    assert p["bodies"][8]["simulation"]["material_id"] == 8
    p["run"].update(fluid_enabled=False, thermal_enabled=False, prescribed_jet=None)
    p["fluid"]["surface"]["enabled"] = False
    p["electromagnetics"].update(wires=[], magnets=[], uniform=[])
    label = "on" if included else "off"
    path = folder / f"heater-{label}.mhd.json"
    path.write_text(json.dumps(p, indent=2) + "\n")
    output = root / f"rooted-heater-dc-v1-{label}"
    jobs.append({"label": label, "project": str(path),
                 "project_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                 "output": str(output),
                 "command": ["target/debug/examples/conduction_probe", str(path), "30", str(output), "0", "body", "1e-5"]})
record = {
    "status": "Prepared complete-heater stationary DC control; no native result",
    "question": "Does conducting aluminium around the M6 root divert current from the liquid in the external-contact head?",
    "source_project": str(source), "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
    "jobs": jobs,
    "gates": {"true_relative_residual": 1e-5, "boundary_current_defect": 1e-5, "power_relative_defect": 1e-4},
    "scope": "Only heater inclusion differs. Same100um grid, geometry, fixed hot conductivities and30A command. Static zero-B DC current sharing only; no induction, motional force, droplet or thermal evolution.",
    "limits": [
        "Complete simplified heater envelope is included. Previous full-lead stationary DC screen includes heater material but crops y/z; it is not a full-heater control.",
        "Cu contact bodies held equipotential. This isolates head shunting; long-lead resistance, clamp contact resistance and current crowding in extended leads are excluded.",
        "Smooth ideal contacting M6 geometry, mica and material conductivities are model assumptions; thread contact resistance and real assembly bridges need measurement.",
        "Existing frozen conduction_probe binary uses the earlier split-f32 implementation. Its hash and actual numerical gates must be recorded; do not attribute current source-v2 code to it.",
        "No PM field solve is needed for static current sharing at zero velocity with unit permeability; force changes require separate J cross B evaluation.",
    ],
}
(root / "rooted-heater-dc-v1-plan.json").write_text(json.dumps(record, indent=2) + "\n")
Path("hardware/rooted-heater-dc-plan.json").write_text(json.dumps(record, indent=2) + "\n")
print(root / "rooted-heater-dc-v1-plan.json")
