"""Native fixed-magnetization internal H diagnostics; no nonlinear magnet model."""

import json, math
from pathlib import Path
import numpy as np

root = Path("artifacts/hardware-research")
mu0 = 4e-7 * math.pi
M = 0.95 / mu0
rows = []
fields = {}
for q in [3, 4]:
    run = root / f"m6-magnet-interior-q{q}"
    r = json.loads((run / "report.json").read_text())
    nx, ny, nz = r["dims"]
    h = r["cell_m"]
    f = np.fromfile(run / "em.bin", dtype="<f4").reshape(nz, ny, nx, 16)
    ids = np.fromfile(run / "material-ids.bin", dtype="<u4").reshape(nz, ny, nx)
    if not np.all(ids == 18):
        raise ValueError("Window must be inside one magnet")
    z, y, x = np.meshgrid(
        *[r["origin_m"][i] + (np.arange(r["dims"][i]) + 0.5) * h for i in [2, 1, 0]],
        indexing="ij",
    )
    margin = np.minimum.reduce(
        [12.5e-3 - abs(x), y - 1.7e-3, 11.7e-3 - y, z + 2e-3, 3e-3 - z]
    )
    # Magnetization points -Y. Project H=B/mu0-M onto that direction.
    hpar = -f[..., 1].astype(float) / mu0 - M
    fields[q] = hpar
    stats = {}
    for m in [0.0, 0.5, 1.0]:
        mask = margin > m * 1e-3
        stats[str(m)] = {
            "sample_count": int(mask.sum()),
            "minimum_H_parallel_kA_m": float(hpar[mask].min() / 1000),
            "maximum_H_parallel_kA_m": float(hpar[mask].max() / 1000),
        }
    rows.append({"q": q, "source": str(run), "sampled_margin_mm": stats})
# Exact centre-axis solid angle identity for uniformly magnetized cuboids.
a, b, c = 12.5, 2.5, 5.0


def omega(d):
    return 4 * math.atan(a * b / (d * math.sqrt(a * a + b * b + d * d)))


self_demag = -omega(c) / (2 * math.pi)
centre_distance = 13.4
neighbor = (omega(centre_distance - c) - omega(centre_distance + c)) / (4 * math.pi)
record = {
    "status": "Completed native fixed-M internal-field diagnostic; not a demagnetization/lifetime qualification",
    "source_Br_T": 0.95,
    "magnetization_A_m": M,
    "cases": rows,
    "q4_q3_H_max_difference_kA_m_in_core_1mm": float(
        abs(fields[4] - fields[3])[margin > 1e-3].max() / 1000
    ),
    "analytic_pair_centre_H_parallel_kA_m": (self_demag + neighbor) * M / 1000,
    "method": "Existing native bound-surface-current applied-field evaluator. H_parallel=(B/mu0-M) dot M_hat at centres inside one pole, mu_r=1. Analytical centre-axis solid-angle identity is a separate geometry check; no CPU field/PDE implementation.",
    "limits": [
        "Fixed uniform magnetization; no material recoil curve, local demagnetization feedback, hot-property update or actual grade certificate.",
        "Near-surface source quadrature and smoothing can bias fields; sampled maxima are not worst-case bounds, especially sharp corners and seams.",
        "Applied-current self-field and external return conductors are absent. Do not compare only to room-temperature Hcj or call centre margin a full lifetime qualification.",
    ],
}


def rect_omega(x, z, d):
    value = 0.0
    for sx in [-1, 1]:
        for sz in [-1, 1]:
            u = sx * 12.5 - x
            v = sz * 2.5 - z
            value += sx * sz * math.atan(u * v / (d * math.sqrt(u * u + v * v + d * d)))
    return value


def exact_h_parallel(point):
    x, y, z = point
    hy = 0.0
    for cy in [-6.7, 6.7]:
        for face, charge in [(cy - 5, M), (cy + 5, -M)]:
            d = y - face
            hy += (
                charge
                * math.copysign(1.0, d)
                * rect_omega(x, z - 0.5, abs(d))
                / (4 * math.pi)
            )
    return -hy / 1000


assert abs(exact_h_parallel([0, 6.7, 0.5]) - (self_demag + neighbor) * M / 1000) < 1e-8
checks = []
for label, flat_index in [
    ("minimum", fields[4].argmin()),
    ("maximum", fields[4].argmax()),
    ("near_center", np.ravel_multi_index((nz // 2, ny // 2, nx // 2), (nz, ny, nx))),
]:
    k, j, i = np.unravel_index(flat_index, (nz, ny, nx))
    point = [(r["origin_m"][a] + (v + 0.5) * h) * 1000 for a, v in enumerate([i, j, k])]
    checks.append(
        {
            "label": label,
            "point_mm": point,
            "native_H_parallel_kA_m": float(fields[4][k, j, i] / 1000),
            "closed_form_H_parallel_kA_m": exact_h_parallel(point),
        }
    )
record["closed_form_point_checks"] = checks
record["near_surface_internal_field_accepted_for_margin"] = False
record["reason"] = (
    "At the native positive-H extremum, closed form gives -130kA/m while native q4 gives+282kA/m. At the negative extremum it gives-302 versus-461. Near-centre error is0.23kA/m. This near-face quadrature error precludes a lifetime conclusion from sampled extrema."
)
bound_N = (2 * math.pi + omega(5)) / (4 * math.pi)
record["uniform_fixed_M_self_demag_component_bound_kA_m"] = bound_N * M / 1000
record["bound_derivation"] = (
    "Inside a uniformly magnetized cuboid, both charged end faces oppose M. Each face subtends at most2pi steradians; at least one face is>=5mm away, so that face subtends no more than the centred25x5mm rectangle at5mm. Sum/(4pi) bounds the opposing H component. The aligned opposite pole supports M within the common rectangular projection; ignoring that support retains the bound. Fixed-M, mu_r1 and ideal abutting bars only; current self-field absent."
)
record["grade_curve_reference"] = {
    "url": "https://www.horizonmagnet.com/uploads/Grade-YXG26H-Samarium-Cobalt-Magnet-Demagnetization-Curves-at-Different-Temperatures.pdf",
    "verified": "2026-11-19",
    "scope": "Visually inspected typical YXG26H curves; the250C curve has not entered its steep polarization fall by about-0.5MA/m. This is not a batch certificate for the catalogue bars or a quantitative knee measurement.",
}
record["method"] = (
    "Native bound-surface-current diagnostics plus analytical solid-angle point checks and component bounds for rectangular bars."
)
record["individual_stock_25x5x5_bar_self_demag_component_bound_kA_m"] = (
    (2 * math.pi + omega(2.5)) / (4 * math.pi) * M / 1000
)
record["individual_bar_bound_scope"] = (
    "Neglect supporting field from all other aligned bars. Applies to each actual5mm-thick stock bar, even with small seam gaps; the481kA/m bound uses an ideal continuous10mm pair. FixedM,mu_r1 only; current and transverse fields/recoil excluded."
)
record["limits"].append(
    "Use a supplier-certified grade and hot curve; a generic300C maximum does not establish recoil/irreversible-loss margin. Analytical component bound does not bound transverse H or current-induced heating."
)
with Path("hardware/m6-magnet-internal-field-screening.json").open("w") as out:
    json.dump(record, out, indent=2)
    out.write("\n")
print(json.dumps(record, indent=2))
