"""Record the completed coarse pulse without treating it as waveform qualification."""

import json
from pathlib import Path
import numpy as np
from native_field_io import read_native_fields

root = Path("artifacts/hardware-research")
run = root / "m6-rooted08-flow110-r25-vented-tol1e4-h010"
status = json.loads((run / "run-status.json").read_text())
analysis = json.loads((run / "flow-analysis.json").read_text())
if status["status"] != "complete" or status["accepted_steps"] != 1200:
    raise ValueError("Expected complete primary trial")
checks = []
for p in sorted(run.glob("frame-*/metadata.json")):
    m = json.loads(p.read_text())
    nx, ny, nz = m["dims"]
    f = read_native_fields(p.parent / "fluid.bin", (nz, ny, nx, 16))
    row = {"time_s": m["time_s"]}
    for name, mask in [
        ("all_mobile", f[..., 12] > 0.5),
        ("liquid_alpha_half", (f[..., 12] > 0.5) & (f[..., 14] >= 0.5)),
    ]:
        div = f[..., 5][mask].astype(float)
        row[name] = {
            "RMS_divergence_s_inv": float(np.sqrt(np.mean(div * div))),
            "max_abs_divergence_s_inv": float(np.max(abs(div))),
        }
    checks.append(row)
last = analysis["frames"][-1]
primary = {}
for threshold, regions in last["connectivity_thresholds"].items():
    if regions:
        primary[threshold] = max(regions, key=lambda r: r["volume_nl"])
record = {
    "status": "Completed exploratory coarse single pulse; zero-current and refinement controls not yet assessed",
    "native_run_status": status,
    "analysis": analysis,
    "frame_divergence_checks": checks,
    "final_largest_detached_region_by_threshold": primary,
    "max_relative_liquid_volume_change": max(
        abs(r["relative_liquid_volume_change"]) for r in analysis["frames"]
    ),
    "electrical_minus_joule_minus_lagged_mechanical_J": status[
        "lagged_electrical_work_J"
    ]
    - status["joule_generated_J"]
    - status["lagged_mechanical_work_J"],
    "actual_vs_lagged_mechanical_relative_difference": status["fluid_mechanical_work_J"]
    / status["lagged_mechanical_work_J"]
    - 1,
    "interpretation": [
        "A detached leading region appears by2.7ms atallfouralpha thresholds. Final equivalentdiameter0.49..0.56mm anddownwardspeedabout0.93m/s; these are threshold-dependent coarse metrics, not physical validation.",
        "Trailingliquid / possible secondary breakup is threshold-sensitive andslow. No cleanone-drop-per-pulse orhundreds-Hz claim.",
        "100umgridhas4cellsacrossthe0.4mmbore. Knowncapillary/slow-motion errors remain. Zero-drivecontrol, spatial/timestep sensitivity andreturn-to-rest are pending.",
        "InductionlessimposedPM approximation, fixedmaterialproperties, idealcontacts; no thermal evolution,wirefeed,oxide/contactchemistry,current self-field ordriver switching.",
        "The0.8mmreplaceablestrip/carrierCAD is a separate geometry branch; it is not the model in this flow result.",
    ],
}
Path("hardware/m6-rooted-flow-first-pulse-screening.json").write_text(
    json.dumps(record, indent=2) + "\n"
)
print(
    {
        k: record[k]
        for k in [
            "max_relative_liquid_volume_change",
            "electrical_minus_joule_minus_lagged_mechanical_J",
            "actual_vs_lagged_mechanical_relative_difference",
        ]
    }
)
