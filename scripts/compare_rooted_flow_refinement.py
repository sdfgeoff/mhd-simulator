"""Compare completed paired space/forced-timestep runs without claiming convergence."""

import copy
import hashlib
import json
from pathlib import Path

root = Path("artifacts/hardware-research")
coarse = root / "m6-rooted08-flow110-r25-compactgas-h010"
fine = root / "m6-rooted08-flow110-r25-compactgas-h0050"
required = [
    p / f for p in [coarse, fine] for f in ["run-status.json", "flow-analysis.json"]
]
if any(not p.exists() for p in required):
    raise SystemExit("Pending completed native run/report; no comparison written")


def read(path):
    return json.loads(path.read_text())


cs, fs = [read(p / "run-status.json") for p in [coarse, fine]]
ca, fa = [read(p / "flow-analysis.json") for p in [coarse, fine]]
if cs["status"] != "complete" or fs["status"] != "complete":
    raise SystemExit("Native run incomplete; no comparison written")
if cs["accepted_steps"] != 1200 or fs["accepted_steps"] != 1320:
    raise ValueError("Unexpected step counts")
cp, fp = [read(Path(s["project"])) for s in [cs, fs]]
cp_check, fp_check = copy.deepcopy(cp), copy.deepcopy(fp)
del cp_check["domain"]["cell_mm"], fp_check["domain"]["cell_mm"]
if cp_check != fp_check:
    raise ValueError("Unexpected project differences beyond grid spacing")
for key in [
    "peak_A",
    "positive_duration_s",
    "rise_s",
    "retract_fraction",
    "retract_delay_s",
    "contact_resistivity_ohm_m2",
    "frequency_hz",
    "uniform_acceleration_m_s2",
    "periodic_fluid_axes",
    "tail_timestep_s",
]:
    if cs[key] != fs[key]:
        raise ValueError("Drive/physics mismatch: " + key)
if abs(fs["grid"]["cell_m"] / cs["grid"]["cell_m"] - 0.5) > 1e-5:
    raise ValueError("Unexpected cell spacing ratio")
if abs(fs["timestep_s"] / cs["timestep_s"] - 0.5) > 1e-5:
    raise ValueError("Unexpected forced-step ratio")
if abs(fs["time_s"] - cs["time_s"]) > max(cs["tail_timestep_s"], fs["tail_timestep_s"]):
    raise ValueError("Final times differ by more than one tail step")
summaries = []
for path, status, analysis in [(coarse, cs, ca), (fine, fs, fa)]:
    if abs(analysis["frames"][-1]["time_us"] * 1e-6 - status["time_s"]) > 1e-9:
        raise ValueError("Analysis does not reach native final time")
    defect = (
        status["lagged_electrical_work_J"]
        - status["joule_generated_J"]
        - status["lagged_mechanical_work_J"]
    )
    relative_defect = defect / max(
        abs(status["lagged_electrical_work_J"]), abs(status["joule_generated_J"]), 1e-15
    )
    if abs(relative_defect) > 1e-4:
        raise ValueError("Electrical energy identity failed")
    last = analysis["frames"][-1]
    largest = {
        threshold: max(regions, key=lambda r: r["volume_nl"]) if regions else None
        for threshold, regions in last["connectivity_thresholds"].items()
    }
    summaries.append(
        {
            "source": str(path),
            "native_status": status,
            "analysis_sha256": hashlib.sha256(
                (path / "flow-analysis.json").read_bytes()
            ).hexdigest(),
            "initial_liquid_nl": analysis["frames"][0]["liquid_volume_nl"],
            "initial_aperture_proxy": analysis["initial_aperture_proxy"],
            "max_abs_relative_liquid_volume_change": max(
                abs(r["relative_liquid_volume_change"]) for r in analysis["frames"]
            ),
            "any_saved_liquid_boundary_contact": any(
                r["liquid_touches_domain_boundary_alpha01"] for r in analysis["frames"]
            ),
            "electrical_energy_identity_relative_defect": relative_defect,
            "actual_over_lagged_fluid_work_minus_one": (
                status["fluid_mechanical_work_J"] / status["lagged_mechanical_work_J"]
                - 1
                if status["lagged_mechanical_work_J"]
                else None
            ),
            "final_below_tip_nl": last["below_tip_nl"],
            "final_largest_detached_region_by_threshold": largest,
            "all_final_detached_regions_by_threshold": last["connectivity_thresholds"],
        }
    )
comparisons = []
for threshold in ["0.01", "0.25", "0.5", "0.75"]:
    c, f = [
        s["final_largest_detached_region_by_threshold"][threshold] for s in summaries
    ]
    row = {
        "alpha_threshold": threshold,
        "coarse_has_detachment": c is not None,
        "fine_has_detachment": f is not None,
    }
    if c and f:
        row.update(
            diameter_relative_change=f["equivalent_diameter_mm"]
            / c["equivalent_diameter_mm"]
            - 1,
            volume_relative_change=f["volume_nl"] / c["volume_nl"] - 1,
            coarse_vertical_velocity_m_s=c["velocity_m_s"][2],
            fine_vertical_velocity_m_s=f["velocity_m_s"][2],
            vertical_velocity_difference_m_s=f["velocity_m_s"][2]
            - c["velocity_m_s"][2],
            vertical_velocity_relative_change=(
                f["velocity_m_s"][2] / c["velocity_m_s"][2] - 1
                if abs(c["velocity_m_s"][2]) > 1e-6
                else None
            ),
        )
    comparisons.append(row)
record = {
    "status": "Completed paired grid/forced-timestep sensitivity comparison; not a convergence or hardware validation claim",
    "runs": summaries,
    "comparisons": comparisons,
    "initial_liquid_relative_difference": summaries[1]["initial_liquid_nl"]
    / summaries[0]["initial_liquid_nl"]
    - 1,
    "final_time_difference_us": (fs["time_s"] - cs["time_s"]) * 1e6,
    "limits": [
        "Space and forced-pulse timestep change together. Tail timestep is unchanged. Their separate effects are not isolated.",
        "Discrete aperture and initial pool volume change with voxelization. This pair includes geometry-resolution effects.",
        "Largest detached component is selected independently at each threshold/run. All final components are preserved; this selection alone does not establish corresponding droplets or clean single-drop release.",
        "Saved component thresholds and a finite number of exports do not certify pinch-off or absence of satellites between frames.",
        "The compact exterior-gas approximation was checked separately at100um; its fine-grid influence remains unisolated.",
        "No thermal evolution, replenishment, calibrated wetting/oxide/contact resistance, self-field or physical driver validation. A single pulse does not establish hundreds of droplets per second.",
    ],
}
output = Path("hardware/m6-rooted-flow-refinement-screening.json")
with output.open("x") as out:
    json.dump(record, out, indent=2)
    out.write("\n")
print(json.dumps(comparisons, indent=2))
print(output)
