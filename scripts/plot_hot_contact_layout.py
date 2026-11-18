"""Actual CAD contact anchor sections; no clip or thread-fit qualification."""

from pathlib import Path
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Patch
from draw_hardware_layouts import COLORS, load, section, label

parts, _ = load("m6-hot-contact-wide-v1/hot-contact")
fig, axes = plt.subplots(
    1, 3, figsize=(15, 7), gridspec_kw={"width_ratios": [1.5, 1, 1.1]}
)
fig.subplots_adjust(top=0.83, bottom=0.19, left=0.065, right=0.98, wspace=0.27)
section(axes[0], parts, "xz")
axes[0].set(
    xlim=(-13, 13), ylim=(-0.8, 16), title="XZ section: folded copper contact fingers"
)
label(axes[0], "Heater", (9, 10), (-12, 14))
label(axes[0], "Electrical insulation\nunder each hot plate", (7, 3.95), (-12, 10))
label(axes[0], "Warm copper fingers\nfeed the tip thermally", (4.5, 1.8), (-12, 6.5))
label(
    axes[0], "50 mm supply extensions\nto imposed colder ends", (11.8, 3.7), (-12, -0.5)
)
section(axes[1], parts, "xz")
axes[1].set(xlim=(3.4, 10.6), ylim=(2.9, 5.7), title="Anchor detail: XZ section")
label(axes[1], "0.1 mm mica layer", (6, 3.95), (3.5, 5.3))
label(axes[1], "6 × 10 × 0.4 mm\ncopper anchor plate", (7, 3.7), (6.8, 3.05))
section(axes[2], parts, "yz", height=7.0)
axes[2].set(xlim=(-12, 12), ylim=(-3, 10), title="YZ section at x = 7 mm")
label(axes[2], "Hot copper plate", (3, 3.7), (-11, 8.7))
label(axes[2], "0.5 mm gap to magnets", (4, 3.25), (-11, 6.6))
label(axes[2], "SmCo bars", (7, 0.5), (-11, -2.5))
fig.suptitle(
    "Heat the contact fingers while electrically isolating them\nActual research CAD sections; added hot plates also heat the magnet surroundings",
    fontsize=15,
)
fig.legend(
    handles=[
        Patch(facecolor=COLORS[k], label=t)
        for k, t in [
            ("heater", "Heated block"),
            ("steel", "Stainless nozzle"),
            ("liquid", "Solder"),
            ("copper", "Copper"),
            ("mica", "Mica"),
        ]
    ],
    loc="lower center",
    bbox_to_anchor=(0.5, 0.10),
    ncol=5,
    frameon=False,
)
fig.text(
    0.5,
    0.065,
    "Insulated clamping hardware is not yet detailed. Thin mica conducts heat but must prevent each finger shorting to the block.",
    ha="center",
    fontsize=10,
)
fig.text(
    0.5,
    0.03,
    "Steady thermal experiment: fixed heater, constant properties, ideal contacts; feeding, Joule heat and external losses are absent.",
    ha="center",
    fontsize=9,
)
fig.savefig(
    Path("artifacts/hardware-research/diagrams/m6-hot-contact-layout.png"), dpi=180
)
