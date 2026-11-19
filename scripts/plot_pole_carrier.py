"""Actual assembled/exploded carrier CAD and sectioned clearance views."""

import argparse, json
from pathlib import Path
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from mpl_toolkits.mplot3d.art3d import Poly3DCollection
from matplotlib.patches import Patch
from draw_hardware_layouts import COLORS, category, load, section, label

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--case", default="m6-carrier-review-wide-stops")
parser.add_argument(
    "--output",
    type=Path,
    default=Path("artifacts/hardware-research/diagrams/m6-pole-carrier-assembly.png"),
)
parser.add_argument("--contact-section", action="store_true")
args = parser.parse_args()
root = Path("artifacts/hardware-research/cases") / args.case
manifest = json.loads((root / "manifest.json").read_text())
parts, _ = load(str(root / "assembly.gltf"))
base_count = manifest.get("base_body_count", 9)
outlet = manifest.get("outlet_z_mm", 0)
projection = manifest["modeled_standoff_below_outlet_mm"]
size = manifest["new_part_bounds_mm"]["size"]


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
    for index, (name, faces, _) in enumerate(parts):
        kind = category(name)
        if only_carrier and not (
            kind == "magnet" or (index >= base_count and kind in ["carrier", "screw"])
        ):
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
label(
    c,
    f"0.5 mm floor sets\n{projection:.1f} mm projection",
    (8, manifest["new_part_bounds_mm"]["min"][2]),
    (-19, 5.2),
)
label(c, "Open droplet corridor", (0, -2.25), (-19, 2.7))
c.axhline(outlet, color="#27735b", ls=":", lw=0.8)
c.text(6, 0.4, "N", ha="center", fontsize=13, color="#243f5b")
c.text(-6, 0.4, "S", ha="center", fontsize=13, color="#243f5b")
d = fig.add_subplot(224)
if args.contact_section:
    section(d, parts, "yz", height=5.5)
    d.set(xlim=(-15, 15), ylim=(-3.3, 6.5), title="Contact section at x = 5.5 mm")
    label(d, "0.2 mm head-to-floor gap", (2.0, -2.25), (5, 4.6))
    label(d, "M2 terminal bolt", (0, -1.3), (-14, -2.5))
    label(d, "Stainless strip / Cu contact", (1, 0.5), (-14, 4.6))
    d.axhline(outlet, color="#27735b", ls=":", lw=0.8)
else:
    section(d, parts, "yz", height=15.2)
    d.set(xlim=(-19, 19), ylim=(-3.3, 23), title="End section at x = 15.2 mm")
    label(d, "Cold-carriage mounting\nM3 pilot envelopes", (14.3, 20), (-18, 20.8))
    label(d, "Copper supply foil\nclears keeper by 0.5 mm", (1.25, 3.7), (-18, 11))
    label(d, "M2 keeper screw\nhead/shaft envelope", (4.2, 4.5), (-18, 6.8))
fig.suptitle(
    f"Independent aluminium carriers retain the poles without loading the nozzle\nActual nominal CAD: {size[0]:.0f} × {size[1]:.1f} mm carrier envelope; {projection:.1f} mm lowest projection",
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
            ("steel", "Stainless nozzle / strip"),
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
fig.savefig(args.output, dpi=160)
