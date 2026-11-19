"""Show native initial air connectivity before interpreting any evolved jet."""

from pathlib import Path
import json
import numpy as np
from scipy import ndimage
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Patch

root = Path("artifacts/hardware-research")
fig, axes = plt.subplots(1, 2, figsize=(10, 9))
fig.subplots_adjust(left=0.08, right=0.98, bottom=0.18, top=0.86, wspace=0.25)
for ax, name, title in zip(
    axes,
    ["m6-rooted08-flow110-r25-h010", "m6-rooted08-flow110-r25-vented-h010"],
    ["Compact domain: upper air trapped", "Extended domain: continuous air path"],
):
    p = root / name / "frame-00000000"
    m = json.loads((p / "metadata.json").read_text())
    nx, ny, nz = m["dims"]
    h = m["cell_m"]
    origin = np.array(m["origin_m"])
    f = np.memmap(p / "fluid.bin", mode="r", dtype="<f4").reshape(nz, ny, nx, 16)
    mobile = f[..., 12] > 0.5
    gas = mobile & (f[..., 14] < 0.01)
    labels, n = ndimage.label(gas)
    upper = np.floor((np.array([0, 0, 0.009]) - origin) / h).astype(int)
    x, y, z = upper
    top_label = labels[z, y, x]
    image = np.zeros((nz, ny, 4))
    image[:] = [0.62, 0.65, 0.68, 1]
    section = f[:, :, nx // 2]
    mask = mobile[:, :, nx // 2]
    air = gas[:, :, nx // 2]
    ids = labels[:, :, nx // 2]
    image[mask] = [0.15, 0.56, 0.72, 1]
    image[air] = [0.9, 0.96, 0.98, 1]
    image[air & (ids == top_label)] = [0.54, 0.79, 0.59, 1]
    extent = [
        origin[1] * 1000,
        (origin[1] + ny * h) * 1000,
        origin[2] * 1000,
        (origin[2] + nz * h) * 1000,
    ]
    ax.imshow(
        image, origin="lower", extent=extent, interpolation="nearest", aspect="equal"
    )
    ax.set(title=title, xlabel="y (mm)", ylabel="z (mm)")
    if n == 2:
        ax.annotate(
            "Upper bore air",
            (0, 9),
            xytext=(-3.5, 7.2),
            arrowprops={"arrowstyle": "->"},
            fontsize=9,
        )
    else:
        ax.annotate(
            "Air passes above heater",
            (4, 16),
            xytext=(-7, 14.3),
            arrowprops={"arrowstyle": "->"},
            fontsize=9,
        )
        ax.annotate(
            "Side passage",
            (8.2, 10),
            xytext=(-7, 11.5),
            arrowprops={"arrowstyle": "->"},
            fontsize=9,
        )
fig.suptitle(
    "Flow needs an air path around the heated head\nNative initial midplane sections; green marks gas connected to the upper bore",
    fontsize=14,
)
fig.legend(
    handles=[
        Patch(facecolor=c, label=t)
        for c, t in [
            ("#8ac995", "Gas connected to upper bore"),
            ("#e6f5fa", "Other gas"),
            ("#268eb8", "Solder"),
            ("#9ea6ad", "Solid CAD"),
        ]
    ],
    loc="lower center",
    bbox_to_anchor=(0.5, 0.095),
    ncol=2,
    frameon=False,
)
fig.text(
    0.5,
    0.025,
    "Initial geometry/topology diagnostic only. These are not evolved jetting frames.\nThe compact domain remains suitable for its static electrical scope; it trapped the gas for this flow setup.",
    ha="center",
    fontsize=10,
)
fig.savefig(root / "diagrams/m6-rooted-flow-air-path.png", dpi=170)
