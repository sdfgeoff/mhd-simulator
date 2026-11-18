"""Plot headless nozzle snapshots: python scripts/plot_jet.py OUTPUT_DIRECTORY.

Optional postprocessing dependencies: numpy and matplotlib.
"""
import json
import sys
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.patches import Polygon


def main():
    directory = Path(sys.argv[1] if len(sys.argv) > 1 else "artifacts/jet-experiment")
    grid = json.loads((directory / "grid.json").read_text())
    dims = grid["dims"]
    spacing_mm = grid["cell_m"] * 1000
    origin_mm = np.array(grid["origin_m"]) * 1000
    files = sorted(directory.glob("alpha-*.bin"))
    if not files:
        raise SystemExit(f"No alpha snapshots in {directory}")
    count = min(4, len(files))
    indices = np.linspace(0, len(files) - 1, count).round().astype(int)
    selected = [files[i] for i in indices]
    fig, axes = plt.subplots(
        1, count, figsize=(4 * count, 6), squeeze=False, constrained_layout=True
    )
    profile = np.array([[.75, -1], [.75, .15], [.15, .15], [.15, 0], [.6, 0], [.6, -1]])
    for ax, filename in zip(axes[0], selected):
        step = int(filename.stem.split("-")[1])
        alpha = np.fromfile(filename, dtype="<f4").reshape(dims[::-1])
        middle = dims[1] // 2
        section = alpha[:, middle, :]
        if dims[1] % 2 == 0:
            section = .5 * (section + alpha[:, middle - 1, :])
        ax.imshow(
            section, origin="lower", interpolation="nearest", vmin=0, vmax=1,
            extent=[origin_mm[0], origin_mm[0] + dims[0] * spacing_mm,
                    origin_mm[2], origin_mm[2] + dims[2] * spacing_mm],
            cmap="Blues",
        )
        for sign in [-1, 1]:
            wall = profile.copy()
            wall[:, 0] *= sign
            ax.add_patch(Polygon(
                wall, closed=True, facecolor="#cccccc", edgecolor="#666666",
                hatch="///", linewidth=.5,
            ))
        ax.axhline(.15, color="#cc5500", lw=.7, ls=":")
        ax.set(
            xlabel="x (mm)", ylabel="z (mm)", ylim=(origin_mm[2], origin_mm[2] + dims[2] * spacing_mm),
            title=f"{step * grid['dt_s'] * 1e6:.1f} µs",
        )
        ax.set_aspect("equal")
    forcing = "Unforced" if grid["force_peak_n_m3"] == 0 else "Prescribed force pulse"
    fig.suptitle(
        f"{forcing} · {spacing_mm * 1000:.0f} µm cells — central liquid fraction\n"
        "Grey: ideal CAD wall (simulation uses voxel walls)"
    )
    fig.savefig(directory / "sections.png", dpi=160, bbox_inches="tight", pad_inches=.15)


if __name__ == "__main__":
    main()
