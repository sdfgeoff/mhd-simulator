"""Plot actual native current samples through the chamber above the slit."""

import json
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.colors import LogNorm
import numpy as np

data = json.loads(Path("hardware/split-slit-electrical-screening.json").read_text())
panels = []
for label, title in [
    ("alumina", "Insulating chamber"),
    ("steel", "Stainless chamber"),
    ("steel_touch", "Stainless + holder contact"),
]:
    row = next(r for r in data["cases"] if r["label"] == label)
    path = Path(row["cases"]["narrow-h0050"]["source"])
    g = json.loads((path / "grid.json").read_text())
    nx, ny, nz = g["dims"]
    h = g["cell_m"]
    f = np.memmap(path / "em.bin", dtype="<f4", mode="r", shape=(nz, ny, nx, 16))
    ids = np.memmap(
        path / "material-ids.bin", dtype="<u4", mode="r", shape=(nz, ny, nx)
    )
    x = (g["origin_m"][0] + (np.arange(nx) + 0.5) * h) * 1000
    y = (g["origin_m"][1] + (np.arange(ny) + 0.5) * h) * 1000
    z = (g["origin_m"][2] + (np.arange(nz) + 0.5) * h) * 1000
    ix = np.where(abs(x) < 1.3)[0]
    iy = np.where(abs(y) < 1.3)[0]
    iz = np.argsort(abs(z - 0.75))[:2]
    region = np.ix_(iz, iy, ix)
    phase = np.array(ids[region][0])
    if not np.array_equal(phase, ids[region][1]):
        raise ValueError("Material changes between section samples")
    j = np.array(f[region][..., 4:7], dtype=float).mean(axis=0)
    panels.append((title, x[ix], y[iy], j, phase))
fig, axes = plt.subplots(1, 3, figsize=(13, 5), constrained_layout=True)
maximum = max(np.linalg.norm(p[3], axis=-1).max() / 1e6 for p in panels)
for ax, (title, x, y, j, phase) in zip(axes, panels):
    magnitude = np.linalg.norm(j, axis=-1) / 1e6
    picture = ax.imshow(
        np.ma.masked_where(magnitude <= 0.05, magnitude),
        origin="lower",
        extent=[x[0] - 0.025, x[-1] + 0.025, y[0] - 0.025, y[-1] + 0.025],
        cmap="magma",
        norm=LogNorm(0.05, maximum),
        interpolation="nearest",
    )
    for mid, color in [
        (7, "#c49d38"),
        (14, "#6e858e"),
        (16, "#d47828"),
        (17, "#d47828"),
        (4, "#1f93cc"),
    ]:
        if np.any(phase == mid) and np.any(phase != mid):
            ax.contour(
                x,
                y,
                (phase == mid).astype(float),
                levels=[0.5],
                colors=[color],
                linewidths=1.0,
            )
    step = 3
    X, Y = np.meshgrid(x, y)
    norm = np.linalg.norm(j[..., :2], axis=-1)
    good = (magnitude > 0.1) & (norm > 0)
    u = np.divide(j[..., 0], norm, out=np.zeros_like(norm), where=good) * 0.1
    v = np.divide(j[..., 1], norm, out=np.zeros_like(norm), where=good) * 0.1
    ax.quiver(
        X[::step, ::step],
        Y[::step, ::step],
        np.ma.masked_where(~good, u)[::step, ::step],
        np.ma.masked_where(~good, v)[::step, ::step],
        angles="xy",
        scale_units="xy",
        scale=1,
        color="#2c6c7a",
        width=0.004,
    )
    ax.set(title=title, xlabel="x (mm)", ylabel="y (mm)", aspect="equal")
fig.suptitle(
    "30 A stationary current: section above the split plates\nz = 0.75 mm, mean of adjacent native 50 µm samples",
    fontsize=14,
)
fig.colorbar(picture, ax=axes, label="Current density (A/mm²)", shrink=0.75)
fig.supxlabel(
    "Arrows show in-plane direction; colour shows magnitude. Blue: liquid boundary; grey/gold: steel/alumina. No flow or jetting result.",
    fontsize=9,
)
out = Path("artifacts/hardware-research/diagrams/split-slit-current-paths-h0050.png")
fig.savefig(out, dpi=160)
print(out)
