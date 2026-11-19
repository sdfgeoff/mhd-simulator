"""Assess completed native slit probes; does not solve electrical/fluid fields."""

import argparse
import hashlib
import json
from pathlib import Path

import numpy as np

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--root", type=Path, default=Path("artifacts/hardware-research"))
parser.add_argument("--plan", type=Path)
parser.add_argument(
    "--output", type=Path, default=Path("hardware/split-slit-electrical-screening.json")
)
args = parser.parse_args()
plan = json.loads(
    (args.plan or args.root / "split-slit-chargefix-v1-plan.json").read_text()
)


def job_path(label, kind):
    return Path(
        next(j for j in plan["jobs"] if j["label"] == label and j["kind"] == kind)[
            "output"
        ]
    )


missing = [
    j["output"]
    for j in plan["jobs"]
    if not (Path(j["output"]) / "report.json").exists()
]
if missing:
    raise SystemExit("Pending reports; no result written:\n" + "\n".join(missing))


def read(path, name="report.json"):
    return json.loads((path / name).read_text())


def fields(path, grid):
    nx, ny, nz = grid["dims"]
    data = np.memmap(path / "em.bin", dtype="<f4", mode="r", shape=(nz, ny, nx, 16))
    ids = np.memmap(
        path / "material-ids.bin", dtype="<u4", mode="r", shape=(nz, ny, nx)
    )
    return data, ids


observer = job_path("common", "field-h0050")
og = read(observer)
of, oi = fields(observer, og)
liquid = oi == 4
if not np.any(liquid):
    raise ValueError("No observer liquid")
b = np.array(of[..., :3], dtype=float)
reuse_path = args.root / "split-slit-reused-field-h0050"
reuse = read(reuse_path)
observer_project = Path(
    next(j for j in plan["jobs"] if j["kind"] == "field-h0050")["command"][1]
)
if (
    hashlib.sha256(observer_project.read_bytes()).hexdigest()
    != reuse["target_project_sha256"]
):
    raise ValueError("Slit applied-source project changed after field reuse checks")
if (
    hashlib.sha256((reuse_path / "B.npy").read_bytes()).hexdigest()
    != reuse["field_file_sha256"]
):
    raise ValueError("Reused field hash differs")
reused_b = np.load(reuse_path / "B.npy")
outlet_q3 = reused_b[: og["dims"][2]]
quadrature_overlap_error = float(
    np.linalg.norm((outlet_q3 - b)[liquid]) / np.linalg.norm(b[liquid])
)
if quadrature_overlap_error > 0.03:
    raise ValueError("Outlet quadrature disagreement exceeds3%; assess before reuse")
rows = []
for label in ["alumina", "steel", "steel_touch", "alumina_touch"]:
    cases = {}
    for kind in ["wide-h010", "narrow-h010", "narrow-h0050"]:
        path = job_path(label, kind)
        r = read(path)
        if not (
            abs(r["power_relative_defect"]) < 1e-4
            and r["true_relative_residual"] <= plan["relative_tolerance"]
            and r["boundary_current_defect"] <= 1e-5
            and abs(r["return_current_A"] / r["current_A"] + 1) < 2.1e-5
        ):
            raise ValueError("Electrical closure/residual: " + str(path))
        cases[kind] = {
            "source": str(path),
            "report": r,
            "R_ohm_cropped": r["terminal_voltage_V"] / r["current_A"],
            "liquid_Joule_fraction": r["liquid_joule_power_W"] / r["joule_power_W"],
        }
    path = job_path(label, "narrow-h0050")
    g = read(path, "grid.json")
    h = g["cell_m"]
    if abs(h / og["cell_m"] - 1) > 1e-6:
        raise ValueError("Observer spacing mismatch")
    offset = (np.array(og["origin_m"]) - g["origin_m"]) / h
    if np.max(abs(offset - np.rint(offset))) > 1e-3:
        raise ValueError("Observer grid misalignment")
    x, y, z = np.rint(offset).astype(int)
    nx, ny, nz = og["dims"]
    if min(x, y, z) < 0 or any(
        start + count > size
        for start, count, size in zip((x, y, z), (nx, ny, nz), g["dims"])
    ):
        raise ValueError("Observer lies outside conduction grid")
    full, ids = fields(path, g)
    roi = full[z : z + nz, y : y + ny, x : x + nx]
    if not np.array_equal(ids[z : z + nz, y : y + ny, x : x + nx], oi):
        raise ValueError("Observer phase mismatch: " + label)
    j = np.array(roi[..., 4:7], dtype=float)
    downward_force = float(-np.cross(j, b)[..., 2][liquid].sum() * h**3)
    terminal_current = cases["narrow-h0050"]["report"]["current_A"]
    # Explicitly a cell-centred diagnostic, not a reconstructed FV face flux.
    plane_x = g["dims"][0] // 2
    plane_ids = ids[:, :, plane_x]
    plane_jx = np.array(full[:, :, plane_x, 4], dtype=float)
    phase_currents = {
        str(int(mid)): float(plane_jx[plane_ids == mid].sum() * h * h)
        for mid in np.unique(plane_ids)
        if mid != 0
    }
    plane_total = sum(phase_currents.values())
    cases["narrow-h0050"].update(
        {
            "outlet_region_downward_force_N": downward_force,
            "outlet_region_force_N_per_terminal_A": downward_force / terminal_current,
            "outlet_liquid_volume_mm3": float(liquid.sum() * h**3 * 1e9),
            "outlet_liquid_By_mean_T": float(b[..., 1][liquid].mean()),
            "outlet_liquid_By_range_T": [
                float(b[..., 1][liquid].min()),
                float(b[..., 1][liquid].max()),
            ],
            "cell_centred_midplane_current_by_material_A": phase_currents,
            "cell_centred_midplane_sum_over_terminal_current": plane_total
            / terminal_current,
            "cell_centred_midplane_liquid_fraction": (
                phase_currents.get("4", 0) / plane_total if plane_total else None
            ),
        }
    )
    offset = (np.array(reuse["origin_m"]) - g["origin_m"]) / h
    if (
        np.max(abs(offset - np.rint(offset))) > 1e-3
        or abs(reuse["cell_m"] / h - 1) > 1e-6
    ):
        raise ValueError("Whole-pool field alignment")
    x, y, z = np.rint(offset).astype(int)
    nx, ny, nz = reuse["dims"]
    if min(x, y, z) < 0 or any(
        start + count > size
        for start, count, size in zip((x, y, z), (nx, ny, nz), g["dims"])
    ):
        raise ValueError("Whole-pool observer outside conduction grid")
    pool_ids = ids[z : z + nz, y : y + ny, x : x + nx]
    pool = pool_ids == 4
    if np.count_nonzero(pool) != np.count_nonzero(ids == 4):
        raise ValueError("Reused field does not cover all initial liquid")
    pool_j = np.array(full[z : z + nz, y : y + ny, x : x + nx, 4:7], dtype=float)
    pool_force = float(-np.cross(pool_j, reused_b)[..., 2][pool].sum() * h**3)
    cases["narrow-h0050"].update(
        {
            "whole_initial_liquid_downward_force_N_reused_q3": pool_force,
            "whole_initial_liquid_force_N_per_terminal_A_reused_q3": pool_force
            / terminal_current,
            "whole_initial_liquid_volume_mm3": float(pool.sum() * h**3 * 1e9),
        }
    )
    rw = cases["wide-h010"]["R_ohm_cropped"]
    rn = cases["narrow-h010"]["R_ohm_cropped"]
    rf = cases["narrow-h0050"]["R_ohm_cropped"]
    rows.append(
        {
            "label": label,
            "cases": cases,
            "narrow_minus_wide_relative_R_at100um": rn / rw - 1,
            "fine_minus_coarse_relative_R_narrow": rf / rn - 1,
        }
    )
record = {
    "status": "Completed stationary electrical and outlet applied-field screens; no slit jetting result",
    "plan": plan,
    "cases": rows,
    "reused_whole_pool_field": reuse,
    "q3_vs_q4_outlet_liquid_B_relative_L2": quadrature_overlap_error,
    "limits": [
        "The outlet_region force integrates only z=0..0.5mm. It is not total pool force, outlet pressure or a droplet prediction.",
        "The separate whole-initial-liquid force uses the existingq3appliedB after identical-source checks; outletq4force remains separate. Outlet quadrature agreement does not prove whole-pool convergence. Neither force is outlet pressure or a droplet prediction.",
        "Midplane currents integrate cell-centred J and are approximate phase shares, not exact FV face currents. Their sum/terminal-current ratio is included.",
        "Resistance includes only conductor lengths inside the cropped electrical box. Outside leads, real contacts and driver losses must be added separately.",
        "Ideal joints and fixed material conductivities; no hot aluminium/copper comparison, thermal evolution, wetting, oxide, feed or lifetime qualification.",
        "Only the initially filled bore/slot intersection is conducting liquid. A moving meniscus can wet more of the long slit and change the current path.",
        "Wide/narrow and100/50um comparisons concern the electrical screen. They do not establish fluid breakup convergence.",
    ],
}
args.output.parent.mkdir(parents=True, exist_ok=True)
with args.output.open("x") as out:
    json.dump(record, out, indent=2)
    out.write("\n")
print(args.output)
