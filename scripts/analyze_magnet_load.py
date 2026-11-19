"""Native applied-field Maxwell-stress midplane integral for pole attraction."""

import json, math
from pathlib import Path
import numpy as np

root = Path("artifacts/hardware-research")
rows = []
mu0 = 4e-7 * math.pi
for x, z in [(25, 15), (40, 25)]:
    run = root / f"m6-load-plane-{x}-{z}"
    r = json.loads((run / "report.json").read_text())
    nx, ny, nz = r["dims"]
    h = r["cell_m"]
    if ny != 1 or abs(r["origin_m"][1] + 0.5 * h) > 1e-9:
        raise ValueError("Must sample midplaney0")
    B = (
        np.fromfile(run / "em.bin", dtype="<f4")
        .reshape(nz, ny, nx, 16)[..., :3]
        .astype(float)
    )
    stress = (B[..., 1] ** 2 - 0.5 * (B**2).sum(axis=-1)) / mu0
    rows.append(
        {
            "source": str(run),
            "half_extent_X_mm": x,
            "half_extent_Z_mm": z,
            "cell_mm": h * 1000,
            "pole_attraction_N": float(stress.sum() * h * h),
            "maximum_normal_B_T": float(abs(B[..., 1]).max()),
            "maximum_tangential_B_T": float(
                np.linalg.norm(B[..., [0, 2]], axis=-1).max()
            ),
        }
    )
F = rows[-1]["pole_attraction_N"]
L = 0.015
width = 0.005
thickness = 0.003
E = 69e9
I = width * thickness**3 / 12
record = {
    "status": "Completed fixed-source static pole-attraction screen; not a qualified carrier",
    "cases": rows,
    "relative_force_change_expanding_plane": rows[-1]["pole_attraction_N"]
    / rows[0]["pole_attraction_N"]
    - 1,
    "method": "Integrate Tyy=(By²-0.5|B|²)/mu0 on source-free y0 plane using nativeq4 source fields. The full infinite plane separates both pole groups; finite-plane truncation checked by expansion. Unit permeability throughout, no imposed head current.",
    "illustrative_two_end_supports_per_pole": {
        "force_each_support_N": F / 2,
        "unsupported_length_mm": L * 1000,
        "section_width_X_mm": width * 1000,
        "section_thickness_Y_mm": thickness * 1000,
        "assumed_Young_modulus_GPa": E / 1e9,
        "cantilever_tip_deflection_each_um": (F / 2) * L**3 / (3 * E * I) * 1e6,
        "two_sides_gap_closure_um": 2 * (F / 2) * L**3 / (3 * E * I) * 1e6,
        "root_bending_stress_MPa": 6 * (F / 2) * L / (width * thickness**2) / 1e6,
    },
    "limits": [
        "Fixed uniform magnetization Br0.95T, source quadratureq4; no hot recoil, eddy forces, assembly error or current self-field. Force scales roughly withBr² only within this fixed-source approximation.",
        "End-support calculation assumes independent15mm cantilevers and equal load split. It excludes screw joints, holes, stress concentration, carrier contact compliance and thermal distortion; it is a sizing estimate, not an actual-frame FEM.",
        "The nozzle tip must not act as a mechanical stop for the magnets. Carrier lips/fasteners may project below the magnets and must count toward stand-off.",
    ],
    "reference": "https://web.mit.edu/viz/guide/Chapter-11-Inductance%20and%20Magnetic%20Energy.pdf",
}
record["illustrative_design_load_N_per_pole"] = 20
record["design_load_basis"] = (
    "Chosen rounded engineering allowance above twice nominal static model; not a certified load bound."
)
record["two_sides_gap_closure_um_at_design_load"] = (
    record["illustrative_two_end_supports_per_pole"]["two_sides_gap_closure_um"]
    * 20
    / F
)
with Path("hardware/m6-magnet-carrier-load-screening.json").open("x") as out:
    json.dump(record, out, indent=2)
    out.write("\n")
print(json.dumps(record, indent=2))
