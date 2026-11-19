"""Actual CAD comparison of the thin sharp boss and a thicker radiused root."""

from pathlib import Path
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Patch
from draw_hardware_layouts import COLORS, load, section, label

fig, axes = plt.subplots(1, 2, figsize=(11, 6))
fig.subplots_adjust(left=0.07, right=0.98, top=0.78, bottom=0.20, wspace=0.25)
for ax, source, title in zip(
    axes,
    ["m6-stub-drilled-v1/stub4", "m6-rooted08-v1/stub4"],
    ["Earlier 0.7 mm sharp boss", "0.8 mm boss with R0.1 root"],
):
    parts, _ = load(source)
    section(ax, parts[:4], "xz")
    ax.set(xlim=(-1.25, 1.25), ylim=(-0.1, 1.55), title=title)
    label(ax, "0.4 mm bore", (0.18, 0.2), (-1.2, -0.05))
    if "rooted" in source:
        label(ax, "R0.1 root", (0.43, 0.56), (-1.2, 1.4))
        label(ax, "0.2 mm nominal wall", (0.3, 0.25), (-1.2, 1.1))
        label(
            ax,
            "0.35 mm contact height\n0.05 mm below fillet start",
            (0.6, 0.43),
            (0.05, 1.35),
        )
    else:
        label(ax, "Sharp root idealization", (0.35, 0.6), (-1.2, 1.4))
        label(ax, "0.15 mm nominal wall", (0.275, 0.25), (-1.2, 1.1))
fig.suptitle(
    "Trade a modest current increase for a thicker tip and an explicit tool radius\nSame 0.4 mm outlet / 0.6 mm land; actual generated sections",
    fontsize=14,
)
fig.legend(
    handles=[
        Patch(facecolor=COLORS[k], label=t)
        for k, t in [
            ("steel", "Stainless"),
            ("liquid", "Solder"),
            ("copper", "Copper contacts"),
        ]
    ],
    loc="lower center",
    bbox_to_anchor=(0.5, 0.08),
    ncol=3,
    frameon=False,
)
fig.text(
    0.5,
    0.025,
    "Geometry screen: clamping force, concentricity, surface finish and machining tolerances remain to be specified.",
    ha="center",
    fontsize=10,
)
fig.savefig(
    Path("artifacts/hardware-research/diagrams/m6-rooted08-tip-comparison.png"), dpi=180
)
