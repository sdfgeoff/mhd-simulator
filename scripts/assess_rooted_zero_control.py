"""Record zero-drive evidence with its explicitly different post-pulse timestep."""

import json
from pathlib import Path

root = Path("artifacts/hardware-research")
run = root / "m6-rooted08-flow0-vented-tol1e4-h010"
s = json.loads((run / "run-status.json").read_text())
a = json.loads((run / "flow-analysis.json").read_text())
if s["status"] != "complete":
    raise ValueError("Control incomplete")
record = {
    "status": "Completed zero-imposed-current comparison; not a timestep refinement check",
    "native_run_status": s,
    "analysis": a,
    "maximum_below_tip_nl": max(r["below_tip_nl"] for r in a["frames"]),
    "any_detached_region": any(
        bool(regions)
        for r in a["frames"]
        for regions in r["connectivity_thresholds"].values()
    ),
    "max_relative_liquid_volume_change": max(
        abs(r["relative_liquid_volume_change"]) for r in a["frames"]
    ),
    "scope": "SamefullventedCAD,gravity,surface tension,materials andpressuretol1e-4 asdrivenrun. Initial3usdt,then10ustail from360us;444stepsreach3.6ms. This supportsdrive interpretation but doesnotisolate timestep effects orqualifybreakup. Fine spatial/time comparison remains pending.",
}
Path("hardware/m6-rooted-zero-drive-screening.json").write_text(
    json.dumps(record, indent=2) + "\n"
)
print(
    {
        k: record[k]
        for k in [
            "maximum_below_tip_nl",
            "any_detached_region",
            "max_relative_liquid_volume_change",
        ]
    }
)
