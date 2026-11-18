"""Headless CAD sections of the assembly-clearance / drilled-bore candidate."""

from pathlib import Path
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Patch
from draw_hardware_layouts import COLORS, load, section, label
from make_hardware import heater
from make_v6_conduction import as_solid

parts, _ = load("m6-stub-drilled-v1/stub4")
fig, axes = plt.subplots(
    1, 3, figsize=(14, 7), gridspec_kw={"width_ratios": [1.3, 1, 1]}
)
fig.subplots_adjust(left=0.05, right=0.98, bottom=0.18, top=0.82, wspace=0.28)
section(axes[0], parts + [("Reference heater", [], as_solid(heater(4, 15.5)))], "yz")
axes[0].set(xlim=(-12, 12), ylim=(-3, 16), title="Assembly: YZ section")
label(axes[0], "1 mm heater gap", (7, 3.5), (-11, 9))
label(axes[0], "0.3 mm side gap", (-1.55, 2), (-11, 5.5))
label(axes[0], "2 mm magnet projection", (8, -2), (-11, -2.6))
axes[0].axhline(0, color="#27735b", ls=":", lw=0.9)
section(axes[1], [p for p in parts if "magnet" not in p[0].lower()], "xz")
axes[1].set(xlim=(-4, 4), ylim=(-0.6, 10.6), title="Nozzle: XZ section")
label(axes[1], "Top annular sealing face", (2.1, 10), (-3.9, 10.3))
label(axes[1], "M6 × 1, 6 mm envelope\nthread helix omitted", (2.8, 7.5), (-3.9, 8.8))
label(axes[1], "3.2 mm inlet bore", (1.6, 5.5), (-3.9, 6.5))
label(axes[1], "2 mm lower bore", (0.95, 2), (-3.9, 3.8))
label(axes[1], "External contacts", (2, 0.3), (-3.9, -0.3))
section(axes[2], [p for p in parts if "magnet" not in p[0].lower()], "yz")
axes[2].set(xlim=(-1.8, 1.8), ylim=(-0.2, 2.6), title="Outlet detail: YZ section")
label(axes[2], "118° lower drill point", (0.65, 0.87), (-1.7, 2.3))
label(axes[2], "0.7 mm square boss\n0.15 mm nominal wall", (0.35, 0.25), (-1.7, 1.8))
label(axes[2], "0.4 mm bore\n0.6 mm land", (0.2, 0.1), (-1.7, -0.05))
fig.suptitle(
    "More assembly clearance, with standard 118° drill-point geometry\nShort stainless M6 nozzle concept — dimensions in mm",
    fontsize=15,
)
fig.legend(
    handles=[
        Patch(facecolor=COLORS[k], label=t)
        for k, t in [
            ("heater", "Reference heater"),
            ("steel", "Stainless nozzle"),
            ("liquid", "Solder"),
            ("magnet", "SmCo bars"),
            ("copper", "Copper contacts"),
        ]
    ],
    loc="lower center",
    bbox_to_anchor=(0.5, 0.09),
    ncol=5,
    frameon=False,
)
fig.text(
    0.5,
    0.065,
    "Lower 2 mm drilling reaches 9.52 mm from the top to its nominal point; the 0.4 mm outlet is drilled from below.",
    ha="center",
    fontsize=10,
)
fig.text(
    0.5,
    0.03,
    "Research CAD, not a shop drawing: tool radii, tolerances, thread roots, contact clamps and cold magnet carriers remain to be detailed.",
    ha="center",
    fontsize=9,
)
fig.savefig(
    Path("artifacts/hardware-research/diagrams/m6-practical-clearance-layout.png"),
    dpi=180,
)
