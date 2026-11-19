"""Read native current/field results for the matched strip material comparison."""

import argparse
import json
from pathlib import Path
import numpy as np

root = Path("artifacts/hardware-research")
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--cases", default="m6-strip-tip-v1")
parser.add_argument(
    "--output", type=Path, default=Path("hardware/m6-strip-tip-screening.json")
)
args = parser.parse_args()
manifest = json.loads((root / "cases" / args.cases / "manifest.json").read_text())
rows = []
for row in manifest["cases"]:
    path = root / ("m6-strip-" + row["label"] + "-current30-h0050")
    report = json.loads((path / "report.json").read_text())
    grid = json.loads((path / "grid.json").read_text())
    nx, ny, nz = grid["dims"]
    h = grid["cell_m"]
    f = np.memmap(path / "em.bin", mode="r", dtype="<f4").reshape(nz, ny, nx, 16)
    ids = np.memmap(path / "material-ids.bin", mode="r", dtype="<u4").reshape(
        nz, ny, nx
    )
    z = grid["origin_m"][2] + (np.arange(nz) + 0.5) * h
    roi = (z[:, None, None] >= 0) & (z[:, None, None] < 0.0006)
    force = -float(f[..., 10][roi & (ids == 4)].astype(float).sum() * h**3)
    # Recompute current crossing x=0 between adjacent cell centres. Same metric
    # as native report but also includes the copper centre's separate ID20.
    liquid = float(
        f[:, :, nx // 2, 4][ids[:, :, nx // 2] == 4].astype(float).sum() * h * h
    )
    strip = float(
        f[:, :, nx // 2, 4][
            (ids[:, :, nx // 2] == (20 if row["strip_material"] == "copper" else 14))
            & (z[:, None] < row["strip_thickness_mm"] * 0.001)
        ]
        .astype(float)
        .sum()
        * h
        * h
    )
    if (
        abs(report["power_relative_defect"]) > 1e-4
        or report["true_relative_residual"] > 1.1e-6
    ):
        raise ValueError("Unaccepted charge solve")
    rows.append(
        {
            **row,
            "native_report": report,
            "outlet_force_N_at30A": force,
            "midplane_adjacent_cell_liquid_current_A": liquid,
            "midplane_adjacent_cell_strip_current_A": strip,
            "terminal_R_ohm": report["terminal_voltage_V"] / report["current_A"],
        }
    )
for row in rows:
    reference = next(
        a
        for a in rows
        if a["strip_width_mm"] == row["strip_width_mm"]
        and a["strip_material"] == "steel"
    )
    row["force_ratio_to_same_width_steel"] = (
        row["outlet_force_N_at30A"] / reference["outlet_force_N_at30A"]
    )
record = {
    "status": "Completed matched 50um native current and applied field comparison; not a seal or jetting qualification",
    "cases": rows,
    "scope": manifest["scope"],
    "interpretation": "Same-width steel/copper pairs differ only in central strip material. Copper extensions retain the same geometry and fixed conductivity. Do not infer repetitive limits or electrode lifetime from this cold screen.",
}
args.output.write_text(json.dumps(record, indent=2) + "\n")
for row in rows:
    print(
        row["label"],
        {
            k: row[k]
            for k in [
                "outlet_force_N_at30A",
                "midplane_adjacent_cell_liquid_current_A",
                "midplane_adjacent_cell_strip_current_A",
                "terminal_R_ohm",
                "force_ratio_to_same_width_steel",
            ]
        },
    )
