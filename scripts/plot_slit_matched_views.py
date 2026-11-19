"""Orthogonal views of accepted matched slit exports, including failure status."""
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from native_field_io import read_native_fields


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--zero", type=Path, default=Path("artifacts/hardware-research/split-slit-steel-conditioned-0A-xends-h010"))
parser.add_argument("--driven", type=Path, default=Path("artifacts/hardware-research/split-slit-steel-conditioned-40A-xends-h010"))
parser.add_argument("--output", type=Path, default=Path("artifacts/hardware-research/diagrams/slit-conditioned-orthogonal-prefix.png"))
args = parser.parse_args()
status = [json.loads((p / "run-status.json").read_text()) for p in [args.zero, args.driven]]
assert status[0]["peak_A"] == 0 and status[1]["peak_A"] > 0
fig, axes = plt.subplots(2, 4, figsize=(12, 9), constrained_layout=True)
reference = None
for col, (directory, target_ms, label) in enumerate([
    (args.zero, 5.001, "Shared unpowered prefix"),
    (args.driven, 6.012, "Driven"),
    (args.zero, 9.519, "Unpowered control"),
    (args.driven, 9.519, "Driven"),
]):
    available = [(path, json.loads(path.read_text())) for path in directory.glob("frame-*/metadata.json")]
    path, m = min(available, key=lambda pair: abs(pair[1]["time_s"] * 1000 - target_ms))
    grid = (m["dims"], m["origin_m"], m["cell_m"])
    if reference is None:
        reference = grid
    if grid != reference or not m["moving_interface"]:
        raise ValueError("Require matched native moving-interface grids")
    nx, ny, nz = m["dims"]
    f = read_native_fields(path.parent / "fluid.bin", (nz, ny, nx, 16))
    h = m["cell_m"] * 1000
    origin = np.asarray(m["origin_m"]) * 1000
    for row in range(2):
        ax = axes[row, col]
        if row == 0:
            section = np.mean(f[:, (ny-1)//2:ny//2+1, :, 14], axis=1)
            solid = 1 - np.mean(f[:, (ny-1)//2:ny//2+1, :, 12], axis=1)
            horizontal, name = origin[0], "x"
        else:
            section = np.mean(f[:, :, (nx-1)//2:nx//2+1, 14], axis=2)
            solid = 1 - np.mean(f[:, :, (nx-1)//2:nx//2+1, 12], axis=2)
            horizontal, name = origin[1], "y"
        extent = [horizontal, horizontal + section.shape[1]*h, origin[2], origin[2] + nz*h]
        picture = ax.imshow(section, extent=extent, origin="lower", cmap="Blues",
                            interpolation="nearest", vmin=0, vmax=1)
        rgba = np.full((*solid.shape, 4), .55)
        rgba[..., 3] = np.clip(solid, 0, 1)
        ax.imshow(rgba, extent=extent, origin="lower", interpolation="nearest")
        ax.axhline(0, color="darkorange", linewidth=.8, linestyle=":")
        ax.set(xlim=(-1.4, 1.4), ylim=(-4.2, .8), xlabel=f"{name} (mm)", ylabel="z (mm)",
               title=f"{label}\n{m['time_s']*1000:.3f} ms · {'XZ' if row==0 else 'YZ'}")
        ax.set_aspect("equal")
    del f
caption = (f"Driven run status: {status[1]['status']} at {status[1]['time_s']*1000:.3f} ms; only accepted exports shown.\n"
           "100 µm cells give only two cells across the 0.2 mm gap; no breakup convergence or hardware qualification.")
fig.suptitle("Slit control / 40 A pulse — orthogonal native liquid-fraction sections\nOrange: nominal exit; grey: solid mask; two central slices averaged", fontsize=13)
fig.supxlabel(caption, fontsize=10)
fig.colorbar(picture, ax=axes.ravel().tolist(), shrink=.6, label="Liquid fraction")
args.output.parent.mkdir(parents=True, exist_ok=True)
fig.savefig(args.output, dpi=160)
print(args.output)
