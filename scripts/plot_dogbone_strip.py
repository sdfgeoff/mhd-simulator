"""Actual CAD sections and exploded view of a full-length replaceable strip."""

from pathlib import Path
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Patch
import numpy as np
from mpl_toolkits.mplot3d.art3d import Poly3DCollection
from draw_hardware_layouts import load, section, label, COLORS, category

parts, _ = load("m6-dogbone-strip-shortneck-v1/assembly")
fig = plt.figure(figsize=(12, 10))
fig.subplots_adjust(
    left=0.07, right=0.98, bottom=0.2, top=0.86, wspace=0.2, hspace=0.33
)
a = fig.add_subplot(221)
section(a, parts[:4] + parts[6:], "xz")
a.set(
    xlim=(-10, 10),
    ylim=(-2.5, 5),
    title="Central XZ section: metal seat and bolted contacts",
)
label(a, "Flat metal seat; preload still unresolved", (0, 0.5), (-9, 4.5))
label(a, "M2 × 3 envelope", (-5.5, -1), (-9, -2.2))
label(a, "Folded copper", (8.5, 2.5), (3.5, 4.5))
b = fig.add_subplot(222)
section(b, [parts[9]], "xy", height=0.25)
b.set(
    xlim=(-9, 9),
    ylim=(-2.5, 2.5),
    title="Replaceable strip: 16 × 4 × 0.5 mm, 0.8 mm waist",
)
label(b, "0.4 mm orifice", (0, 0.2), (-8, 2.0))
label(b, "2.2 mm clearance holes", (5.5, 0.8), (1, -2.2))
c = fig.add_subplot(223)
section(c, parts, "yz", height=5.5)
c.set(xlim=(-4, 4), ylim=(-2.5, 4.5), title="Contact section: 4.6 mm pole gap")
label(c, "4 mm AF nut; 1.6 mm high", (1, 2.2), (-3.8, 4))
label(c, "0.3 mm nominal Cu-to-pole gap", (2.15, 0.75), (-3.8, -2))
d = fig.add_subplot(224, projection="3d")
for i in [2, 3, 9, 10, 11]:
    name, faces, _ = parts[i]
    faces = faces.copy()
    if i in [2, 3]:
        faces[:, :, 2] += 1.6
    if i in [10, 11]:
        faces[:, :, 2] -= 1.8
    # Show local contacts; supply tails clipped from presentation only.
    keep = (np.abs(faces[:, :, 0]).max(axis=1) < 11) & (faces[:, :, 2].max(axis=1) < 6)
    kind = category(name)
    color = COLORS["steel"] if i in [9, 10, 11] else COLORS[kind]
    d.add_collection3d(
        Poly3DCollection(
            faces[keep],
            facecolors=color,
            edgecolors=None,
            antialiased=False,
            shade=True,
        )
    )
d.set(
    xlim=(-10, 10),
    ylim=(-4, 4),
    zlim=(-4, 6),
    xlabel="x (mm)",
    ylabel="y (mm)",
    zlabel="z (mm)",
    title="Exploded contacts and strip; tails clipped",
)
d.set_box_aspect([20, 8, 10])
d.view_init(25, -60)
fig.suptitle(
    "Replaceable stainless orifice strip with ordinary M2 contact fasteners\nActual review CAD; threaded seat preload and heater-anchor clamps remain open",
    fontsize=14,
)
fig.legend(
    handles=[
        Patch(facecolor=COLORS[k], label=t)
        for k, t in [
            ("steel", "Stainless"),
            ("copper", "Copper"),
            ("liquid", "Solder"),
            ("magnet", "SmCo poles"),
            ("mica", "Mica"),
        ]
    ],
    loc="lower center",
    bbox_to_anchor=(0.5, 0.11),
    ncol=5,
    frameon=False,
)
fig.text(
    0.5,
    0.035,
    "Nominal geometry only: no unintended intersections. Thread/socket details, torque, tool access and thermal movement are absent.\nThe contact screws clamp strip to copper; they do not supply the nozzle-seat preload.\nWider poles require repositioned carriers; the previous carrier assembly has not been silently reused.",
    ha="center",
    fontsize=10,
)
fig.savefig(
    Path("artifacts/hardware-research/diagrams/m6-dogbone-strip-layout.png"), dpi=170
)
