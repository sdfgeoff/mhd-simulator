"""Headless annotated diagrams from the actual generated research glTF geometry.

Run with uv run --with manifold3d --with matplotlib python scripts/draw_hardware_layouts.py
"""

import json
from pathlib import Path

import manifold3d as cad
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.collections import LineCollection
from matplotlib.patches import Circle, Patch, PathPatch
from matplotlib.path import Path as MplPath
from mpl_toolkits.mplot3d.art3d import Poly3DCollection, Line3DCollection

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "artifacts/hardware-research/diagrams"
COLORS = {
    "liquid": "#268eb8",
    "steel": "#d5dce2",
    "copper": "#cd833c",
    "magnet": "#9cbbd3",
    "heater": "#abb1b8",
    "wire": "#586575",
}


def category(name):
    name = name.lower()
    if "liquid" in name:
        return "liquid"
    if "copper" in name or "coil" in name:
        return "copper"
    if "magnet" in name:
        return "magnet"
    if "wire" in name:
        return "wire"
    if "heater" in name:
        return "heater"
    return "steel"


def load(name):
    path = ROOT / "artifacts/hardware-research/cases" / (name + ".gltf")
    g = json.loads(path.read_text())
    buffers = [(path.parent / b["uri"]).read_bytes() for b in g["buffers"]]
    parts, curves = [], []
    for mesh in g["meshes"]:
        primitive = mesh["primitives"][0]
        a = g["accessors"][primitive["attributes"]["POSITION"]]
        view = g["bufferViews"][a["bufferView"]]
        start = view.get("byteOffset", 0) + a.get("byteOffset", 0)
        points = (
            np.frombuffer(
                buffers[view["buffer"]], dtype="<f4", count=a["count"] * 3, offset=start
            ).reshape(-1, 3)
            * 1000
        )
        if primitive["mode"] == 3:
            curves.append(points)
            continue
        faces = points.reshape(-1, 3, 3)
        vertices, indices = np.unique(points, axis=0, return_inverse=True)
        solid = cad.Manifold(
            cad.Mesh(
                vertices.astype(np.float32), indices.reshape(-1, 3).astype(np.uint32)
            )
        )
        if not solid.volume() > 0:
            raise ValueError(f"Invalid closed CAD body: {mesh['name']}")
        parts.append((mesh["name"], faces, solid))
    return parts, curves


def section(ax, parts, plane="xz", height=0):
    for name, _, solid in parts:
        oriented = solid.rotate([-90, 0, 0]) if plane == "xz" else solid
        if plane == "yz":
            oriented = solid.transform([[0, 1, 0, 0], [0, 0, 1, 0], [1, 0, 0, 0]])
        shape = oriented.slice(height)
        vertices, codes = [], []
        for polygon in shape.to_polygons():
            if len(polygon) < 3:
                continue
            vertices.extend(polygon)
            vertices.append(polygon[0])
            codes.extend(
                [MplPath.MOVETO]
                + [MplPath.LINETO] * (len(polygon) - 1)
                + [MplPath.CLOSEPOLY]
            )
        if not vertices:
            continue
        kind = category(name)
        ax.add_patch(
            PathPatch(
                MplPath(vertices, codes),
                facecolor=COLORS[kind],
                edgecolor="#596673",
                lw=0.65,
                hatch="///" if kind in ["steel", "heater"] else None,
            )
        )
    ax.set_aspect("equal")
    ax.set_facecolor("#f7fafc")
    ax.spines[["right", "top"]].set_visible(False)
    ax.tick_params(labelsize=9)
    ax.set_xlabel("y (mm)" if plane == "yz" else "x (mm)", fontsize=10)
    ax.set_ylabel("z (mm)" if plane in ["xz", "yz"] else "y (mm)", fontsize=10)


def coils(ax, curves, radius=0.3):
    points = []
    for curve in curves:
        for a, b in zip(curve, curve[1:]):
            if a[1] * b[1] <= 0 and abs(a[1] - b[1]) > 1e-9:
                t = a[1] / (a[1] - b[1])
                p = a + t * (b - a)
                if not any(np.linalg.norm(p - q) < 0.05 for q in points):
                    points.append(p)
    for p in points:
        ax.add_patch(
            Circle(
                (p[0], p[2]),
                radius,
                facecolor=COLORS["copper"],
                edgecolor="#8a5428",
                lw=0.5,
            )
        )


def label(ax, text, point, location, color="#293d50"):
    ax.annotate(
        text,
        xy=point,
        xytext=location,
        fontsize=9,
        color=color,
        arrowprops={"arrowstyle": "-", "color": color, "lw": 0.8},
        bbox={"facecolor": "white", "edgecolor": "none", "alpha": 0.9, "pad": 1.5},
    )


def clip_front(faces):
    result = []
    for triangle in faces:
        polygon = list(triangle)
        clipped = []
        for a, b in zip(polygon, polygon[1:] + polygon[:1]):
            ina, inb = a[1] >= 0, b[1] >= 0
            if ina:
                clipped.append(a)
            if ina != inb:
                clipped.append(a + (b - a) * (-a[1] / (b[1] - a[1])))
        for i in range(1, len(clipped) - 1):
            result.append([clipped[0], clipped[i], clipped[i + 1]])
    return result


def comparison():
    fig, axes = plt.subplots(2, 2, figsize=(12, 11), layout="constrained")
    specs = [
        ("v6-32-stock-block", "1  Stock V6 + induction coil", (-13, 13), (-1, 20)),
        ("foil-60-l015", "2  Custom reservoir + induction coil", (-9, 9), (-1, 12)),
        (
            "cross-channel-retracted-flight5-h010",
            "3  Thin-wall cross-channel + permanent field",
            (-6, 6),
            (-1, 7),
        ),
        (
            "v6-direct-dc-h010",
            "4  V6 + direct current + permanent field",
            (-6, 6),
            (-1, 14),
        ),
    ]
    for ax, (name, title, xlim, ylim) in zip(axes.flat, specs):
        parts, curves = load(name)
        section(ax, parts)
        coils(ax, curves)
        ax.set(xlim=xlim, ylim=ylim, title=title)
        ax.axhline(0, color="#b8c5ce", ls=":", lw=0.6)
    a, b, c, d = axes.flat
    label(a, "Aluminium heater block", (9, 13), (-12, 18))
    label(a, "12-turn coil, 200 A trials", (5.5, 3.5), (-12, 0))
    label(a, "Stainless V6\n3.2 mm inlet", (2, 7), (5.5, 7))
    label(b, "32-turn coil, 200 A trials", (5.5, 6), (-8, 11))
    label(b, "6 mm liquid chamber", (0, 5), (-8, 1.8))
    label(b, "0.5 mm hole / 0.15 mm land", (0.25, 0), (0.5, -0.6))
    label(c, "Retracted 0.8 mm feed wire", (0, 5.5), (-5.8, 6.6))
    label(c, "2 mm ID stainless channel", (1.2, 1.2), (-5.8, 3.6))
    label(c, "0.5 mm downward orifice", (0, 0), (0.8, -0.65))
    c.text(-4.7, 0.6, "+", fontsize=15, color="#8a5428")
    c.text(4.3, 0.6, "−", fontsize=15, color="#8a5428")
    c.annotate(
        "I",
        xy=(2, 1.25),
        xytext=(-2, 1.25),
        color="#9b541f",
        fontsize=11,
        arrowprops={"arrowstyle": "->", "lw": 2, "color": "#9b541f"},
    )
    c.text(0.8, 2.4, "⊙ B out of page", fontsize=10, color="#32678b")
    label(d, "Copper jaws on hex flats", (-4, 3.5), (-5.6, 12.9))
    label(d, "Stock 0.4 mm V6 tip", (0.2, 0), (-5.6, -0.6))
    d.text(-5.6, 7.5, "⊙ B out\nof page", fontsize=9, color="#32678b")
    d.text(3.2, 9.5, "Heater must\nnot bridge\ncontacts", fontsize=8, color="#6e4d35")
    fig.suptitle(
        "Solder jetter layouts under test\nActual central CAD sections • dimensions in mm • heaters/mounts not fully designed",
        fontsize=17,
        weight="bold",
    )
    fig.legend(
        handles=[
            Patch(facecolor=COLORS[k], label=t)
            for k, t in [
                ("steel", "Stainless"),
                ("liquid", "Liquid solder"),
                ("copper", "Copper contact / coil"),
                ("heater", "Aluminium heater"),
            ]
        ],
        loc="outside lower center",
        ncol=4,
        frameon=False,
    )
    fig.savefig(OUT / "hardware-layout-comparison.png", dpi=180)


def detail():
    parts, _ = load("cross-channel-retracted-flight5-h010")
    fig = plt.figure(figsize=(16, 7), layout="constrained")
    ax = fig.add_subplot(131)
    section(ax, parts, "xy", 1.15)
    ax.set(xlim=(-6.5, 6.5), ylim=(-13, 13), title="Plan through the pump channel")
    ax.text(0, 2.6, "N", ha="center", fontsize=16, color="#234f75", weight="bold")
    ax.text(0, -3.4, "S", ha="center", fontsize=16, color="#234f75", weight="bold")
    ax.annotate(
        "B",
        xy=(0.2, -1.65),
        xytext=(0.2, 1.6),
        fontsize=12,
        color="#32678b",
        arrowprops={"arrowstyle": "->", "color": "#32678b", "lw": 2},
    )
    ax.annotate(
        "I",
        xy=(2.8, 0),
        xytext=(-2.8, 0),
        fontsize=12,
        color="#9b541f",
        arrowprops={"arrowstyle": "->", "color": "#9b541f", "lw": 2},
    )
    ax.text(0, 9, "10 mm SmCo cube", ha="center", fontsize=9)
    ax.text(0, -10.5, "10 mm SmCo cube", ha="center", fontsize=9)
    label(ax, "4 mm pole gap", (3, 2), (1, 5.2))
    ax.text(
        0,
        -5,
        "⊗ Force / jet goes\ninto this page (−z)",
        ha="center",
        fontsize=10,
        color="#27735b",
    )
    ax = fig.add_subplot(132)
    section(ax, parts)
    ax.set(xlim=(-5.8, 5.8), ylim=(-1.6, 7), title="Section through the liquid path")
    label(ax, "Open feed neck\n1.2 mm bore", (0.6, 4.4), (-5.5, 5.7))
    label(ax, "Feed wire withdrawn\nduring the pulse", (0, 5.4), (1.1, 5.6))
    label(
        ax,
        "Stainless end cap\nCurrent enters the liquid\nthrough the metal",
        (-2.85, 1.15),
        (-5.5, 3.1),
    )
    label(ax, "0.5 mm orifice\n0.15 mm centre land", (0, 0), (0.7, -1.2))
    ax.annotate(
        "F = J × B",
        xy=(0, -1.15),
        xytext=(0, 0.65),
        ha="center",
        fontsize=10,
        color="#27735b",
        arrowprops={"arrowstyle": "->", "color": "#27735b", "lw": 2},
    )
    ax.text(-4.7, 0.3, "+", fontsize=15, color="#9b541f")
    ax.text(4.2, 0.3, "−", fontsize=15, color="#9b541f")
    ax = fig.add_subplot(133, projection="3d")
    for name, faces, _ in parts:
        kind = category(name)
        if kind == "steel":
            faces = clip_front(faces)
        if len(faces) == 0:
            continue
        collection = Poly3DCollection(
            faces,
            facecolors=COLORS[kind],
            edgecolors="none",
            linewidths=0.4,
            alpha=0.10 if kind == "magnet" else 0.94,
            zsort="average",
        )
        ax.add_collection3d(collection)
        if kind == "magnet":
            v = np.unique(np.asarray(faces).reshape(-1, 3), axis=0)
            edges = [
                (a, b)
                for i, a in enumerate(v)
                for b in v[i + 1 :]
                if np.count_nonzero(np.abs(a - b) > 1e-5) == 1
            ]
            ax.add_collection3d(
                Line3DCollection(edges, colors="#6c8596", linewidths=0.7)
            )
    ax.set(
        xlim=(-6, 6),
        ylim=(-13, 13),
        zlim=(-3.9, 7),
        xlabel="x (mm)",
        ylabel="y (mm)",
        zlabel="z (mm)",
        title="CAD cutaway",
    )
    ax.set_box_aspect((12, 26, 10.9))
    ax.view_init(elev=24, azim=-53)
    ax.tick_params(labelsize=8)
    ax.grid(False)
    fig.suptitle(
        "Thin stainless cross-channel concept\nCurrent runs along the tube • magnetic field crosses it • liquid is driven down through the side orifice",
        fontsize=16,
        weight="bold",
    )
    fig.text(
        0.50,
        0.015,
        "Current-controlled 100–150 A trials; 24/48 V is the driver supply. Magnet grade, contacts, seals and heater still need qualification.",
        ha="center",
        fontsize=10,
        color="#475767",
    )
    fig.savefig(OUT / "cross-channel-setup.png", dpi=180)


def v6_detail():
    parts, _ = load("v6-direct-disc20-h010")
    fig, axes = plt.subplots(1, 3, figsize=(15, 8))
    fig.subplots_adjust(left=0.055, right=0.985, bottom=0.19, top=0.85, wspace=0.25)
    ax = axes[0]
    section(ax, parts, "xy", 3.5)
    ax.set(xlim=(-11, 11), ylim=(-16, 16), title="Plan at contact height")
    for text, xy, start, color in [
        ("I", (3, 0), (-3, 0), "#9b541f"),
        ("B", (0, -4), (0, 4), "#32678b"),
    ]:
        ax.annotate(
            text,
            xy=xy,
            xytext=start,
            fontsize=13,
            color=color,
            arrowprops={"arrowstyle": "->", "color": color, "lw": 2},
        )
    ax.hlines([-9.75, 9.75], -9.9, 9.9, colors="#6c8596", linestyles=":", linewidths=1)
    ax.text(0, 10, "Two 20 × 5 mm discs", ha="center", fontsize=9)
    ax.text(0, -11, "Two 20 × 5 mm discs", ha="center", fontsize=9)
    ax.text(-10, -3, "Downward force\ninto this page", fontsize=9, color="#27735b")
    ax = axes[1]
    section(ax, parts)
    ax.set(xlim=(-6, 6), ylim=(-2, 14), title="Section through copper jaws")
    label(ax, "Copper contacts", (4.4, 3.4), (1.2, 6))
    label(ax, "Stainless V6", (2.5, 8), (-5.5, 11.7))
    label(ax, "Open solder pool", (0, 10.5), (-5.5, 13))
    label(ax, "0.4 mm orifice", (0.2, 0), (-5.5, -1.2))
    ax = axes[2]
    section(ax, parts, "yz")
    ax.set(xlim=(-16, 16), ylim=(-7, 16), title="Section through magnet axes")
    ax.annotate(
        "B",
        xy=(-4, 4.5),
        xytext=(4, 4.5),
        color="#32678b",
        fontsize=13,
        arrowprops={"arrowstyle": "->", "color": "#32678b", "lw": 2},
    )
    ax.annotate(
        "Jet / force",
        xy=(0, -4),
        xytext=(0, -1.5),
        ha="center",
        fontsize=10,
        color="#27735b",
        arrowprops={"arrowstyle": "->", "color": "#27735b", "lw": 2},
    )
    ax.vlines([-9.75, 9.75], -5.5, 14.5, colors="#6c8596", linestyles=":", linewidths=1)
    ax.text(0, 15, "9.5 mm pole gap", ha="center", fontsize=10)
    handles = [
        Patch(facecolor=COLORS[k], edgecolor="#596673", label=v)
        for k, v in [
            ("steel", "Stainless steel"),
            ("liquid", "Solder"),
            ("copper", "Copper"),
            ("magnet", "SmCo"),
        ]
    ]
    fig.legend(handles=handles, loc="lower center", ncol=4, bbox_to_anchor=(0.5, 0.045))
    fig.suptitle(
        "V6 direct-current concept — larger catalogue magnet stacks\nActual generated CAD sections; externally clamped current contacts, metal nozzle retained",
        fontsize=16,
        weight="bold",
    )
    fig.text(
        0.5,
        0.015,
        "180 A current-controlled trial • hot Br assumed 0.95 T • heater, mounting, contact preparation and driver remain to be designed",
        ha="center",
        fontsize=10,
    )
    OUT.mkdir(parents=True, exist_ok=True)
    fig.savefig(OUT / "v6-disc-stack-setup.png", dpi=180)


def v6_tip_detail():
    parts, _ = load("v6-tip-disc20-compact-h010")
    fig, axes = plt.subplots(1, 3, figsize=(15, 8))
    fig.subplots_adjust(left=.055, right=.985, bottom=.19, top=.85, wspace=.28)
    ax = axes[0]
    section(ax, parts, "yz")
    ax.set(xlim=(-13,13), ylim=(-19,14), title="Magnet stacks below the shoulder")
    ax.vlines([-7.1,7.1],-18.2,1.8,colors="#6c8596",linestyles=":",linewidths=1)
    ax.text(0,-14,"4.2 mm\nflight gap",ha="center",fontsize=10)
    ax.annotate("Jet",xy=(0,-7),xytext=(0,-3),ha="center",color="#27735b",arrowprops={"arrowstyle":"->","color":"#27735b","lw":2})
    ax = axes[1]
    section(ax,parts)
    ax.set(xlim=(-1.6,1.6),ylim=(-.3,1.4),title="Tip section enlarged")
    label(ax,"Copper contacts",(1.2,.3),(.15,.85))
    label(ax,"0.4 mm bore",(.18,.05),(-1.5,-.2))
    ax.annotate("Force",xy=(0,-.25),xytext=(0,.2),ha="center",color="#27735b",arrowprops={"arrowstyle":"->","color":"#27735b","lw":2})
    ax = axes[2]
    section(ax,parts,"xy",.3)
    ax.set(xlim=(-5.5,5.5),ylim=(-3,3),title="Plan through the contact band")
    ax.annotate("I",xy=(1.2,0),xytext=(-1.2,0),color="#9b541f",fontsize=12,arrowprops={"arrowstyle":"->","color":"#9b541f","lw":2})
    ax.annotate("B",xy=(0,-1.9),xytext=(0,1.9),color="#32678b",fontsize=12,arrowprops={"arrowstyle":"->","color":"#32678b","lw":2})
    ax.text(3,2.5,"Pole face",ha="center",fontsize=9)
    ax.text(3,-2.7,"Pole face",ha="center",fontsize=9)
    handles=[Patch(facecolor=COLORS[k],edgecolor="#596673",label=v) for k,v in [("steel","Stainless steel"),("liquid","Solder"),("copper","Copper"),("magnet","SmCo")]]
    fig.legend(handles=handles,loc="lower center",ncol=4,bbox_to_anchor=(.5,.045))
    fig.suptitle("V6 tip-contact variant — concentrate current near the orifice\nActual generated CAD sections; the metal nozzle remains intact",fontsize=16,weight="bold")
    fig.text(.5,.015,"Geometry under test • ideal electrical wetting assumed • heater, contact mounting and driver omitted",ha="center",fontsize=10)
    OUT.mkdir(parents=True,exist_ok=True)
    fig.savefig(OUT/"v6-tip-contact-setup.png",dpi=180)


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    comparison()
    detail()
    print(OUT)
