"""Read native initial gas topology; this is a diagnostic, not a fluid solver."""

import argparse, json
from pathlib import Path
import numpy as np
from scipy import ndimage

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("runs", nargs="+", type=Path)
p.add_argument("--output", type=Path, required=True)
a = p.parse_args()
rows = []
for run in a.runs:
    frame = run / "frame-00000000"
    m = json.loads((frame / "metadata.json").read_text())
    nx, ny, nz = m["dims"]
    h = m["cell_m"]
    origin = np.array(m["origin_m"])
    f = np.memmap(frame / "fluid.bin", mode="r", dtype="<f4").reshape(nz, ny, nx, 16)
    mobile = f[..., 12] > 0.5
    labels, n = ndimage.label(mobile & (f[..., 14] < 0.01))
    _, fluid_n = ndimage.label(mobile)

    def label_at(xyz):
        x, y, z = np.floor((np.array(xyz) * 0.001 - origin) / h).astype(int)
        return int(labels[z, y, x])

    top = label_at([0, 0, 9])
    bottom = label_at([0, 0, -3])
    rows.append(
        {
            "run": str(run),
            "gas_components": n,
            "mobile_components": fluid_n,
            "upper_bore_gas_label": top,
            "below_nozzle_gas_label": bottom,
            "air_connected": bool(top != 0 and top == bottom),
        }
    )
a.output.write_text(
    json.dumps(
        {
            "method": "Read-only six-neighbour labels from native initial packet: mobile flag>0.5 andalpha<0.01. Probes at(0,0,9) and(0,0,-3)mm. No field solver.",
            "cases": rows,
        },
        indent=2,
    )
    + "\n"
)
print(json.dumps(rows, indent=2))
