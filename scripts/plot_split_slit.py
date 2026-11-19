"""Draw sections through the generated split-electrode CAD, without a desktop."""

import argparse
import json
import math
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Patch, PathPatch
from matplotlib.path import Path as PlotPath

from draw_hardware_layouts import COLORS, category, load

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("case", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
manifest = json.loads((args.case / "manifest.json").read_text())
gap = manifest["cases"][0]["slit_gap_mm"]
short = manifest["cases"][0]["tube_OD_ID_length_mm"][2] < 9.5
reservoir_id = manifest["cases"][0].get("reservoir_id_mm", 1.0)
COLORS = {**COLORS, "alumina": "#f3e7c7"}


def section(ax, name, plane="xz", height=0):
    parts, _ = load(str(args.case / (name + ".gltf")))
    for body, _, solid in parts:
        shape = (solid.rotate([-90, 0, 0]) if plane == "xz" else solid).slice(height)
        vertices, codes = [], []
        for poly in shape.to_polygons():
            if len(poly) < 3:
                continue
            vertices.extend(poly)
            vertices.append(poly[0])
            codes.extend(
                [PlotPath.MOVETO]
                + [PlotPath.LINETO] * (len(poly) - 1)
                + [PlotPath.CLOSEPOLY]
            )
        if not vertices:
            continue
        kind = "alumina" if "Alumina chamber" in body else category(body)
        ax.add_patch(
            PathPatch(
                PlotPath(vertices, codes),
                facecolor=COLORS[kind],
                edgecolor="#596673",
                lw=0.6,
                hatch="///" if kind == "steel" else None,
            )
        )
    ax.set_aspect("equal")
    ax.set_facecolor("#f7fafc")
    ax.spines[["right", "top"]].set_visible(False)
    ax.set_xlabel("x (mm)")
    ax.set_ylabel("z (mm)" if plane == "xz" else "y (mm)")
    ax.tick_params(labelsize=8)


def note(ax, text, point, location):
    ax.annotate(
        text,
        xy=point,
        xytext=location,
        fontsize=9,
        arrowprops={"arrowstyle": "-", "color": "#475569", "lw": 0.8},
        va="center",
    )


fig, axes = plt.subplots(2, 2, figsize=(13, 10))
fig.subplots_adjust(
    left=0.07, right=0.97, bottom=0.14, top=0.89, hspace=0.35, wspace=0.28
)
a, b, c, d = axes.flat
section(a, "alumina")
a.set(
    xlim=(-6.3, 6.3),
    ylim=(-0.5, 10.5),
    title=(
        "A. Central section: short ceramic insert"
        if short
        else "A. Central section: straight ceramic tube"
    ),
)
note(a, "M6 metal holder\n(thread envelope)", (2.2, 8), (-5.8, 9))
note(
    a,
    "OD 2 / ID 1 mm tube",
    (0.8, 1.5) if short else (0.8, 5.5),
    (1.5, 2.0) if short else (1.5, 6),
)
if short:
    note(a, f"Ø{reservoir_id:g} mm reservoir\nin stainless", (0.7, 6.5), (1.3, 6.3))
note(a, "Two separate\n0.5 mm copper plates", (2, 0.25), (-5.8, 1.8))
a.axhline(0, color="#4e7789", lw=0.6, ls=":")
section(b, "alumina")
b.set(
    xlim=(-1.5, 1.5),
    ylim=(-0.3, 3.6 if short else 2.2),
    title=(
        "B. Outlet and insert seat"
        if short
        else "B. Outlet detail: alumina touches both plates"
    ),
)
note(b, "Liquid solder", (0, 0.9), (-1.4, 1.7))
note(b, "0.5 mm metal-holder clearance", (1.15, 0.75), (-1.4, 3.35 if short else 2.05))
if short:
    note(b, "0.1 mm locating ledge", (0.95, 2.5), (-1.4, 2.95))
b.annotate(
    "",
    xy=(-gap / 2, -0.12),
    xytext=(gap / 2, -0.12),
    arrowprops={"arrowstyle": "<->", "lw": 0.8},
)
b.text(0.2, -0.16, f"{gap:g} mm gap", fontsize=9)
section(c, "alumina", "xy", 0.25)
c.set(
    xlim=(-1.5, 1.5),
    ylim=(-1.5, 1.5),
    title="C. Inside plates, z = 0.25 mm: slit in plan",
)
c.plot(
    [0.5 * math.cos(t * 0.02 * math.pi) for t in range(101)],
    [0.5 * math.sin(t * 0.02 * math.pi) for t in range(101)],
    ls="--",
    lw=0.8,
    color="#536879",
)
note(c, "Dashed: 1 mm chamber bore", (0.35, 0.35), (-1.4, 1.15))
note(c, "Wetted slit ~1 mm long", (0, 0.2), (-1.4, -0.9))
c.annotate(
    "I",
    xy=(0.8, 0),
    xytext=(-0.8, 0),
    fontsize=12,
    arrowprops={"arrowstyle": "->", "color": "#253c4c"},
)
c.annotate(
    "B",
    xy=(0.95, -0.7),
    xytext=(0.95, 0.5),
    fontsize=12,
    arrowprops={"arrowstyle": "->", "color": "#253c4c"},
)
section(d, "alumina_touch")
d.set(
    xlim=(-3.3, 3.3),
    ylim=(-0.3, 2.2),
    title="D. Deliberate bypass comparison: metal bridges",
)
note(
    d,
    "Conducting holder joins both plates\neven with an alumina tube",
    (2, 1),
    (-3.1, 1.9),
)
note(d, "Bridge across the clearance", (2, 0.75), (-3.1, -0.12))
fig.suptitle(
    (
        "Short ceramic insert and metal reservoir: same split-plate outlet\nActual review CAD; copper electrodes remain separate"
        if short
        else "Split-plate slit nozzle: no drilled outlet and a straight chamber tube\nActual comparison CAD; copper electrodes remain separate"
    ),
    fontsize=15,
)
fig.legend(
    handles=[
        Patch(facecolor=COLORS[k], label=t)
        for k, t in [
            ("alumina", "Alumina"),
            ("liquid", "Solder"),
            ("copper", "Copper"),
            ("steel", "Stainless holder"),
        ]
    ],
    loc="lower center",
    bbox_to_anchor=(0.5, 0.075),
    ncol=4,
    frameon=False,
)
fig.text(
    0.5,
    0.047,
    "Nominal geometry only: seal, ceramic preload, plate alignment, thermal contact and wetting remain unresolved.",
    ha="center",
    fontsize=10,
)
fig.text(
    0.5,
    0.024,
    "Magnets, heater and long terminal extensions omitted from these sections. No slit jetting result yet.",
    ha="center",
    fontsize=10,
)
args.output.parent.mkdir(parents=True, exist_ok=True)
fig.savefig(args.output, dpi=160)
print(args.output)
