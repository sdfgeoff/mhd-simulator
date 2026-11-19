"""Assemble completed native heat-path controls and boundary sensitivity."""

import json
from pathlib import Path

root = Path("artifacts/hardware-research")
files = [
    "thermal-hot-contact-v2-no-anchor-h010.json",
    "thermal-hot-contact-v2-weak-anchor-h010.json",
    "thermal-hot-contact-v2-medium-anchor-h010.json",
    "thermal-hot-contact-v2-tol2e6-h010.json",
    "thermal-hot-contact-wide-v1-weak-anchor-h010.json",
]
rows = []
for file in files:
    report = json.loads((root / file).read_text())
    if (
        abs(report["power_identity_relative_error"]) > 2e-4
        or report["heat_return_relative_error"] > 2e-4
    ):
        raise ValueError("Thermal closure failed")
    # Old native output reused the pool/leads method description in anchors mode.
    # Correct only the report prose, preserving raw accepted numerical values.
    report["method"] = (
        "Existing native GPU scalar FV operator with conductivity k; entire aluminium heater ID8 fixed+0.5K, outermost copper planes fixed-0.5K; steel/liquid/copper/mica unknown. No new field solver."
    )
    report["limits"][0] = (
        "Steady anchors-mode screen: entire aluminium heater ID8 fixed-temperature reservoir; steel/liquid/copper/mica solved; effective sheet k represents contact-quality sensitivity."
    )
    rows.append({"source": str(root / file), "native_report": report})
record = {
    "status": "Completed stationary heated-contact thermal experiment; not a qualified operating head",
    "cases": rows,
    "interpretation": "At250C heater/25C outer wire ends, narrow plates without mica coupling give179C outlet; nominal mica gives245C. Effective k0.1 (sheet plus interface resistance) gives228C for30mm² plates and237C for60mm² plates. Wider heated anchors improve thermal margin in this model.",
    "geometry": "Two folded copper fingers and6x10x0.4mm anchor plates for preferred thermal fixture;0.1mm insulation; each50mm,1mm² straight Cu supply extension ends at imposed25C. FullCADcollisionchecked; source accepts fixed-k heat reservoir.",
    "metadata_correction": "Earlier raw anchor reports had a pool/leads method sentence; canonical copied method text corrected above. Actual code fixed ID8 heater, confirmed by hot_boundary_cells and input source. Accepted scalar/flux numbers are unchanged.",
    "rejected_solve": {
        "case": "narrow anchors k0.528",
        "requested_relative_tolerance": 1e-6,
        "iterations": 4000,
        "reported_true_relative_residual": 1.3808963e-6,
        "accepted": False,
        "accepted_request": 2e-6,
        "note": "A single-precision long-wire system; accepted cases separately satisfy flux/energy closure2e-4. This does not establish mesh convergence.",
    },
    "unit_checks": "Uniform k15/30 rod conductance/linear potentials; layered167/0.528/394WmK rod checks analytic harmonic-face series resistance. Boundary flux error23.8ppm, dissipation error0.3ppm; scalar potentials within2e-5. Initial flux tolerance20ppm was unrealistically below measured f32 boundary-cancellation error; final boundary50ppm with separate tighter energy check.",
    "limits": [
        "100um thermal grid; 0.1mm mica is one cell with harmonic-face series resistance verified analytically. No mesh-converged whole fixture result.",
        "Effective insulation conductivity0.1 is an imposed contact-quality sensitivity, not a measured clamp. Real clamping pressure, sheet integrity and electrical isolation must be qualified.",
        "No feeding, exterior losses, pulse-average Joule heat, phase change, temperature-dependent properties or magnet temperature. The heater has unlimited modeled power.",
        "Larger hot anchors add heat paths to magnets; bare-body gas-gap estimate is not a thermal qualification of this fixture.",
    ],
    "source": {
        "url": "https://info.boydcorp.com/hubfs/Thermal/Air-Cooling/Boyd-Board-Level-Heatsinks-Catalog.pdf",
        "page_index": 100,
        "verified": "2026-11-19",
        "facts": "Maker mica table gives k0.528WmK at25C and maximum operating temperature550C. Hot conductivity and clamped-contact resistance not established by that table.",
    },
}
with Path("hardware/m6-hot-contact-thermal-screening.json").open("x") as out:
    json.dump(record, out, indent=2)
    out.write("\n")
for row in rows:
    r = row["native_report"]
    print(row["source"], r["temperature_cases"][0]["liquid_min_C"])
