"""Independent parallel-gap thermal sensitivity; no gas/solid PDE duplication.

This is a geometry-based conductance estimate, not a magnet temperature prediction.
"""

import json, math
from pathlib import Path
import numpy as np
from draw_hardware_layouts import load


def clip_z(poly, bound, above):
    out = []
    for a, b in zip(poly, poly[1:] + poly[:1]):
        ina = (a[2] >= bound) if above else (a[2] <= bound)
        inb = (b[2] >= bound) if above else (b[2] <= bound)
        if ina:
            out.append(a)
        if ina != inb:
            out.append(a + (b - a) * (bound - a[2]) / (b[2] - a[2]))
    return out


def facing_area(triangles, bottom, top):
    area = 0.0
    for triangle in triangles:
        if not (
            np.allclose(triangle[:, 1], 1.4, atol=1e-5)
            or np.allclose(triangle[:, 1], -1.4, atol=1e-5)
        ):
            continue
        poly = clip_z(list(triangle), bottom, True)
        if len(poly) > 2:
            poly = clip_z(poly, top, False)
        for i in range(1, len(poly) - 1):
            area += (
                np.linalg.norm(np.cross(poly[i] - poly[0], poly[i + 1] - poly[0])) / 2
            )
    return float(area)


rows = []
for family, gap, clearance in [
    ("m6-stub-v1", 3.0, 0.45),
    ("m6-stub-clearance-v1", 3.4, 1.0),
]:
    parts, _ = load(family + "/stub4")
    nozzle = parts[0][1]
    y = gap / 2
    r = 3.0
    cap = r * r * math.acos(y / r) - y * math.sqrt(r * r - y * y)
    heater_area = 2 * (23 * (8 - y) - cap)
    nozzle_area = facing_area(nozzle, 4 - clearance - 5, 4 - clearance)
    k = 0.04
    heater_G = k * heater_area * 1e-6 / (clearance * 0.001)
    nozzle_G = k * nozzle_area * 1e-6 / ((gap - 2.8) / 2 * 0.001)
    # An intentionally conservative radiation view-factor envelope: all top and
    # inward faces see the 250C source; outer faces are not used to offset it.
    hot_radiation_area = 2 * (25 * 10 + 25 * 5) * 1e-6
    radiation_blackbody = (
        5.670374419e-8
        * hot_radiation_area
        * ((250 + 273.15) ** 4 - (100 + 273.15) ** 4)
    )
    rows.append(
        {
            "case": family,
            "pole_gap_mm": gap,
            "heater_gap_mm": clearance,
            "heater_projected_overlap_mm2": heater_area,
            "nozzle_flat_face_overlap_mm2": nozzle_area,
            "gas_k_assumed_W_m_K": k,
            "heater_gap_G_W_K": heater_G,
            "nozzle_gap_G_W_K": nozzle_G,
            "total_parallel_gap_G_W_K": heater_G + nozzle_G,
            "conduction_W_if_head250C_magnets100C": 150 * (heater_G + nozzle_G),
            "blackbody_radiation_upper_envelope_W_same_temperatures": radiation_blackbody,
            "cold_mount_G_needed_W_K_to_hold100C_at25C_ambient_if_it_removes_all_of_estimated_heat": (
                150 * (heater_G + nozzle_G) + radiation_blackbody
            )
            / 75,
        }
    )
record = {
    "status": "Analytical heat-path sensitivity only; not a magnet operating-temperature prediction",
    "cases": rows,
    "conductance_ratio_clearance_to_original": rows[1]["total_parallel_gap_G_W_K"]
    / rows[0]["total_parallel_gap_G_W_K"],
    "method": "Gas-gap conduction k*A/g, actual CAD flat-side area clipped to magnet height and analytic projected heater underside overlap. Assumed constant gas k=0.04W/mK. Radiation blackbody envelope treats all magnet top/inward faces as facing the hot source with unit view factor/emissivity.",
    "limits": [
        "Gas conduction is a parallel-plate approximation: no fringing, natural/forced convection, transient or full CAD radiation view factors. The blackbody term is an illustrative radiation upper envelope for those faces, not a full heat-load bound.",
        "Cooling required at an assumed magnet100C/head250C; these are imposed comparison temperatures. No claim that a passive mount reaches100C or that all real heat paths are included.",
        "Current-induced magnet/metal-mount eddy losses, contact bridges, electrical insulation, changing nozzle temperature and assembly tolerances are absent.",
        "Gas conductivity0.025–0.05W/mK would scale conductive heat to0.625–1.25 times these values. Vacuum is not assumed.",
    ],
    "magnet_sources": [
        {
            "url": "https://www.first4magnets.com/product/25-x-5-x-5mm-thick-samarium-cobalt-magnet-32kg-pull-19663",
            "verified": "2026-11-19",
            "facts": "Stock25x5x5mm Sm2Co17; listed±0.1mm tolerance and300C maximum operating temperature. No exact grade hot demagnetization curve obtained.",
        },
        {
            "url": "https://www.arnoldmagnetics.com/products/recoma-samarium-cobalt-magnets/",
            "verified": "2026-11-19",
            "facts": "Manufacturer grade table shows different maximum temperatures and reversible Br coefficients; generic SmCo label does not establish the stock product hot load line.",
        },
    ],
}
with Path("hardware/m6-stub-magnet-heat-paths.json").open("x") as out:
    json.dump(record, out, indent=2)
    out.write("\n")
print(json.dumps(rows, indent=2))
