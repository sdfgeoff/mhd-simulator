"""Verify initial native gas connectivity above and below the unsealed slit pool."""

import hashlib
import json
from pathlib import Path

import numpy as np
from scipy import ndimage
from native_field_io import read_native_fields


def main():
    run = Path("artifacts/hardware-research/split-slit-steel-zero-xends-h010")
    path = run / "frame-00000000/metadata.json"
    m = json.loads(path.read_text())
    nx, ny, nz = m["dims"]
    f = read_native_fields(path.parent / "fluid.bin", (nz, ny, nx, 16))
    mobile = f[..., 12] > 0.5
    gas, components = ndimage.label(mobile & (f[..., 14] < 0.01))
    _, mobile_components = ndimage.label(mobile)
    probes = []
    for point in [[0, 0, 9], [0, 0, -3]]:
        cell = np.floor((np.array(point) * 1e-3 - m["origin_m"]) / m["cell_m"]).astype(
            int
        )
        if np.any(cell < 0) or np.any(cell >= [nx, ny, nz]):
            raise ValueError("Gas probe outside the native grid")
        probes.append(
            {
                "point_mm": point,
                "cell_xyz": cell.tolist(),
                "gas_label": int(gas[tuple(cell[::-1])]),
            }
        )
    connected = (
        probes[0]["gas_label"] != 0 and probes[0]["gas_label"] == probes[1]["gas_label"]
    )
    if not connected:
        raise ValueError(
            "Upper and lower gas probes are not connected in initial native mask"
        )
    output = {
        "method": "Read-only six-neighbour labels from the initial native packet: mobile flag>0.5 and alpha<0.01. No field solver.",
        "run": str(run),
        "metadata_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "gas_components": components,
        "mobile_components": mobile_components,
        "probes": probes,
        "air_connected": connected,
        "limits": [
            "Initial voxel connectivity is not a gas-flow resistance or domain-convergence check.",
            "No proof of continued ventilation after liquid motion or physical wetting.",
        ],
    }
    path = Path("hardware/split-slit-gas-connectivity-screening.json")
    path.write_text(json.dumps(output, indent=2) + "\n")
    print(path)


if __name__ == "__main__":
    main()
