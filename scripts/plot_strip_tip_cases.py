"""Plot native GPU current slices through the actual strip CAD."""

import json
from pathlib import Path
import numpy as np
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.colors import LogNorm

root = Path("artifacts/hardware-research")
record = json.loads(Path("hardware/m6-strip-tip-screening.json").read_text())
fig, axes = plt.subplots(2, 2, figsize=(11, 10), sharex=True, sharey=True)
fig.subplots_adjust(
    left=0.08, right=0.86, bottom=0.22, top=0.86, hspace=0.38, wspace=0.18
)
for ax, row in zip(axes.flat, record["cases"]):
    path = root / ("m6-strip-" + row["label"] + "-current30-h0050")
    g = json.loads((path / "grid.json").read_text())
    nx, ny, nz = g["dims"]
    h = g["cell_m"]
    origin = np.array(g["origin_m"])
    z = round((0.00025 - origin[2]) / h - 0.5)
    fields = np.memmap(path / "em.bin", dtype="<f4", mode="r").reshape(nz, ny, nx, 16)
    ids = np.memmap(path / "material-ids.bin", dtype="<u4", mode="r").reshape(
        nz, ny, nx
    )
    x = (origin[0] + (np.arange(nx) + 0.5) * h) * 1000
    y = (origin[1] + (np.arange(ny) + 0.5) * h) * 1000
    xx, yy = np.meshgrid(x, y)
    j = fields[z, :, :, 4:7].astype(float)
    mag = np.linalg.norm(j, axis=-1)
    mask = np.isin(ids[z], [4, 14, 16, 17, 20])
    values = np.ma.masked_where(~mask, mag * 1e-6)
    im = ax.pcolormesh(
        xx,
        yy,
        values,
        cmap="viridis",
        norm=LogNorm(1, 1000),
        shading="nearest",
        rasterized=True,
    )
    # Thin material boundary marks the solder-filled drilled opening.
    ax.contour(
        xx, yy, (ids[z] == 4).astype(float), levels=[0.5], colors="white", linewidths=1
    )
    select = (abs(xx) < 1.45) & (abs(yy) < 1.5) & mask
    take = np.zeros_like(select)
    take[::4, ::4] = True
    take &= select
    ax.quiver(
        xx[take],
        yy[take],
        j[:, :, 0][take] / np.maximum(mag[take], 1),
        j[:, :, 1][take] / np.maximum(mag[take], 1),
        color="white",
        scale=23,
        width=0.0035,
    )
    ax.set(
        aspect="equal",
        xlim=(-1.5, 1.5),
        ylim=(-1.6, 1.6),
        xlabel="x (mm)",
        ylabel="y (mm)",
        title=f"{row['strip_material'].title()}, {row['strip_width_mm']} mm wide\nOutlet force {row['outlet_force_N_at30A'] * 1000:.3f} mN at 30 A",
    )
    ax.set_facecolor("#eeeeee")
cax = fig.add_axes([0.89, 0.25, 0.02, 0.48])
fig.colorbar(im, cax=cax, label="Native current density |J| (A/mm²)")
fig.suptitle(
    "Continuous copper strips bypass much more current around the solder\nMatched 0.5 mm strips, 0.4 mm hole; native 50 µm GPU fields",
    fontsize=14,
)
fig.text(
    0.5,
    0.045,
    "Top slices at z = 0.25 mm. White hole boundary contains molten solder; arrows show current direction.\nSame-width pairs change only the central strip material. Fixed properties and ideal contacts.\nElectrical screen only: sealing, clamping, heat balance, material wear and jetting are not qualified.",
    ha="center",
    fontsize=10,
)
fig.savefig(root / "diagrams/m6-strip-tip-current-comparison.png", dpi=170)
