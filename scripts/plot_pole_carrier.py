"""Actual assembled/exploded carrier CAD and sectioned clearance views."""

from pathlib import Path
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from mpl_toolkits.mplot3d.art3d import Poly3DCollection
from matplotlib.patches import Patch
from draw_hardware_layouts import COLORS, category, load, section, label

parts, _ = load("m6-carrier-review-wide-stops/assembly")


def clipped(tri):
    poly = list(tri)
    for sign in [-1, 1]:
        output = []
        for a, b in zip(poly, poly[1:] + poly[:1]):
            ina = sign * a[0] <= 20
            inb = sign * b[0] <= 20
            if ina:
                output.append(a)
            if ina != inb:
                output.append(a + (b - a) * (sign * 20 - a[0]) / (b[0] - a[0]))
        poly = output
    return [[poly[0], poly[i], poly[i + 1]] for i in range(1, len(poly) - 1)]


def view(ax, only_carrier=False):
    for name, faces, _ in parts:
        kind = category(name)
        if only_carrier and kind not in ["carrier", "screw", "magnet"]:
            continue
        faces = np.asarray(faces).copy()
        if only_carrier:
            if kind in ["carrier", "magnet"]:
                faces[:, :, 1] += np.sign(faces[:, :, 1].mean()) * 5
            else:
                faces[:, :, 1] += np.sign(faces[:, :, 1].mean()) * 5
                faces[:, :, 2] += 3
            if "keeper" in name and "screw" not in name:
                faces[:, :, 2] += 3
        elif (
            "carrier -1" in name
            or "keeper -1" in name
            or "screw envelope -1" in name
            or name == "Bias magnet 1"
        ):
            # Body order uses negative-Y pole first; hide the front side in this view.
            continue
        polys = [p for t in faces for p in clipped(t)]
        if polys:
            collection = Poly3DCollection(
                polys,
                facecolors=COLORS[kind],
                linewidths=0,
                shade=True,
                antialiased=False,
            )
            collection.set_edgecolor("none")
            ax.add_collection3d(collection)
    if not only_carrier:
        t = np.linspace(0, 2 * np.pi, 129)
        ax.plot(
            14 * np.cos(t),
            14 * np.sin(t),
            np.full_like(t, 22.0),
            ls="--",
            color="#557071",
            lw=0.8,
        )
        ax.text(0, -15, 22.2, "Ø28 upper keepout", fontsize=7)
    ax.set(
        xlim=(-21, 21),
        ylim=(-24, 24),
        zlim=(-4, 23),
        xlabel="x (mm)",
        ylabel="y (mm)",
        zlabel="z (mm)",
    )
    ax.set_box_aspect((42, 48, 27))
    ax.view_init(elev=25, azim=-55)
    ax.tick_params(labelsize=7)


fig = plt.figure(figsize=(14, 11))
fig.subplots_adjust(
    left=0.055, right=0.98, top=0.90, bottom=0.12, hspace=0.27, wspace=0.22
)
a = fig.add_subplot(221, projection="3d")
view(a)
a.set_title("Assembly cutaway: front carrier and pole hidden", fontsize=11)
b = fig.add_subplot(222, projection="3d")
view(b, True)
b.set_title("Pole carriers, bars and lifted keepers", fontsize=11)
c = fig.add_subplot(223)
section(c, parts, "yz")
c.set(xlim=(-20, 20), ylim=(-3.3, 6.5), title="Central YZ section at x = 0 mm")
label(c, "0.5 mm floor sets\n2.7 mm projection", (8, -2.7), (-19, 5.2))
label(c, "Open droplet corridor", (0, -2.25), (-19, 2.7))
c.axhline(0, color="#27735b", ls=":", lw=0.8)
c.text(6, 0.4, "N", ha="center", fontsize=13, color="#243f5b")
c.text(-6, 0.4, "S", ha="center", fontsize=13, color="#243f5b")
d = fig.add_subplot(224)
section(d, parts, "yz", height=15.2)
d.set(xlim=(-19, 19), ylim=(-3.3, 23), title="End section at x = 15.2 mm")
label(d, "Cold-carriage mounting\nM3 pilot envelopes", (14.3, 20), (-18, 20.8))
label(d, "Copper supply foil\nclears keeper by 0.5 mm", (1.25, 3.7), (-18, 11))
label(d, "M2 keeper screw\nhead/shaft envelope", (4.2, 4.5), (-18, 6.8))
fig.suptitle(
    "Independent aluminium carriers retain the poles without loading the nozzle\nActual nominal CAD: 37 × 36.6 mm carrier envelope; 2.7 mm lowest projection",
    fontsize=15,
)
fig.legend(
    handles=[
        Patch(facecolor=COLORS[k], label=t)
        for k, t in [
            ("carrier", "Aluminium carrier / keeper"),
            ("magnet", "SmCo bars"),
            ("heater", "Heater"),
            ("copper", "Copper"),
            ("mica", "Mica"),
            ("steel", "Stainless nozzle"),
        ]
    ],
    loc="lower center",
    bbox_to_anchor=(0.5, 0.055),
    ncol=6,
    frameon=False,
)
fig.text(
    0.5,
    0.033,
    "Supply foils clipped in the 3D view. Projection excludes assembly and target-surface clearance allowances.",
    ha="center",
    fontsize=9,
)
fig.text(
    0.5,
    0.012,
    "Review CAD only: thread roots, tool reliefs, tolerances, preload, thermal distortion and machine mounting are not qualified. Carrier excluded from physics.",
    ha="center",
    fontsize=9,
)
fig.savefig(
    Path("artifacts/hardware-research/diagrams/m6-pole-carrier-assembly.png"), dpi=160
)
