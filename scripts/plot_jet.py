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
    native = not (directory / "grid.json").exists()
    if native:
        records = [(p, json.loads(p.read_text())) for p in directory.glob("frame-*/metadata.json")]
        records.sort(key=lambda item: item[1]["step"])
        if not records:
            raise SystemExit(f"No completed field exports in {directory}")
        grid = dict(records[0][1])
        if any(m.get("format_version") != 1 or not m.get("moving_interface") for _, m in records):
            raise SystemExit("Nozzle plots require version-1 moving-interface field exports")
        if any(any(m[k] != grid[k] for k in ["dims", "origin_m", "cell_m", "byte_order"]) for _, m in records):
            raise SystemExit("Field exports have different grids or byte orders")
        files = []
        for path, m in records:
            packet = next((f for f in m["files"] if f["physics"] == "fluid"), None)
            if packet is None or packet["f32_components_per_cell"] != 16:
                raise SystemExit("Expected a 16-component fluid packet")
            files.append(path.parent / packet["file"])
        times = [m["time_s"] for _, m in records]
        grid["dt_s"] = grid["run"]["dt_s"]
        grid["force_peak_n_m3"] = (grid["run"].get("prescribed_jet") or {}).get("peak_n_m3", 0)
    else:
        grid = json.loads((directory / "grid.json").read_text())
        files = sorted(directory.glob("alpha-*.bin"))
        times = [int(p.stem.split("-")[1]) * grid["dt_s"] for p in files]
    dims = grid["dims"]
    spacing_mm = grid["cell_m"] * 1000
    origin_mm = np.array(grid["origin_m"]) * 1000
    if not files:
        raise SystemExit(f"No fraction snapshots in {directory}")
    count = min(4, len(files))
    indices = np.linspace(0, len(files) - 1, count).round().astype(int)
    fig, axes = plt.subplots(
        1, count, figsize=(4 * count, 6), squeeze=False, constrained_layout=True
    )
    profile = np.array([[.75, -1], [.75, .15], [.15, .15], [.15, 0], [.6, 0], [.6, -1]])
    for ax, index in zip(axes[0], indices):
        filename = files[index]
        dtype = ">f4" if grid.get("byte_order") == "big" else "<f4"
        data = np.fromfile(filename, dtype=dtype)
        alpha = (data.reshape(-1, 16)[:, 14] if native else data).reshape(dims[::-1])
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
            title=f"{times[index] * 1e6:.1f} µs",
        )
        ax.set_aspect("equal")
    forcing = ("Computed EM / fluid run" if grid.get("forcing_kind") == "computed_em"
               else "Prescribed force pulse" if grid["force_peak_n_m3"] > 0
               else "Fluid/interface run" if native else "Unforced")
    fig.suptitle(
        f"{forcing} · {spacing_mm * 1000:.0f} µm cells — central liquid fraction\n"
        f"Δt {grid['dt_s'] * 1e6:.6f} µs · x minimum {origin_mm[0]:.4f} mm\n"
        "Grey: ideal CAD wall (simulation uses voxel walls)"
    )
    fig.savefig(directory / "sections.png", dpi=160, bbox_inches="tight", pad_inches=.15)


if __name__ == "__main__":
    main()
