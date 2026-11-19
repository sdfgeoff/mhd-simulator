"""Render existing CAD for the human-requested design review; no simulation."""

from pathlib import Path
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Patch
from draw_hardware_layouts import load, section, coils, COLORS

OUT = Path("hardware/design-review")
OUT.mkdir(exist_ok=True)
SPECS = [
    ("1. Integral nozzle + external contacts", "hardware/designs/rooted08-hot-contact-m6/hot-contact.gltf", "xz", (-3, 3), (-.5, 3),
     "0.4 mm bore; 0.8 mm square stainless boss\nCopper fingers touch the outside; outlet is one-piece steel."),
    ("2. Split copper plates / slit", "hardware/designs/split-slit-review/steel.gltf", "xz", (-3, 3), (-.5, 3),
     "1 mm tube bore; 0.2 mm slit; 0.5 mm plates\nSlot continues sideways out of this section; ends are open."),
    ("3. Single coil / custom chamber", "hardware/designs/single-coil-250A/geometry.gltf", "xz", (-9, 9), (-1, 11),
     "6 mm chamber; 0.5 mm steel wall; 0.5 mm outlet\nTen-turn winding; narrow high-fill free surface around feed wire."),
    ("4. Compact coil below V6-like heater", "hardware/designs/lowered-v6-heater-slot-review/geometry.gltf", "xz", (-13, 13), (-3, 16),
     "Twelve turns; 0.8 mm radial cut in aluminium heater\nStationary EM only: this package has no moving-fluid result."),
    ("5. Immersed tungsten electrodes", "v6-flat4-pins1mm-cube-gap44-h010", "xz", (-5, 5), (-.7, 12),
     "Two 1 mm pins enter the melt in a V6-like body\nHistorical tested collar geometry; pin seals/mounts unresolved."),
    ("6. Thin-wall cross-channel", "cross-flight5-compact-h010", "xz", (-6, 6), (-1, 7),
     "2 mm internal cross-channel; 0.5 mm outlet\nSide current contacts plus permanent magnets; custom sealed capsule."),
    ("7. Replaceable stainless strip", "hardware/designs/dogbone-strip-review/assembly.gltf", "xz", (-4, 4), (-.5, 3),
     "Separate dogbone outlet strip under a flat chamber seat\nExtra hot seal, clamp and screws; branch parked."),
    ("8. Two phased induction coils", "foil-two-phase-plus90-200A-h020.mhd", "xz", (-8, 8), (-1, 12),
     "Two six-turn windings around the custom chamber\n200 A per phase / 50 kHz historical screen; no resolved ejection."),
]

for page in range(2):
    fig, axes = plt.subplots(2, 2, figsize=(12, 12))
    fig.subplots_adjust(left=.07, right=.97, bottom=.23, top=.88, hspace=.65, wspace=.25)
    for ax, (title, source, plane, xlim, ylim, note) in zip(axes.flat, SPECS[page*4:page*4+4]):
        parts, curves = load(source)
        section(ax, parts, plane)
        coils(ax, curves, radius=.6 if title.startswith(("3.", "4.")) else .5)
        ax.set(xlim=xlim, ylim=ylim, title=title)
        ax.axhline(0, color="#64748b", ls=":", lw=.6)
        ax.text(0, -.22, note, transform=ax.transAxes, va="top", fontsize=9)
    fig.suptitle("Tested design families — existing CAD cross sections", fontsize=16, y=.97)
    fig.text(.5, .935, "Initial geometry, not a predicted liquid shape. Different panel scales; dimensions in mm.", ha="center", fontsize=10)
    fig.legend(handles=[Patch(facecolor=COLORS[k], label=s) for k,s in [
        ("steel", "Stainless"), ("liquid", "Solder"), ("copper", "Copper / winding"),
        ("magnet", "Permanent magnet"), ("mica", "Mica"), ("heater", "Heater")]],
        loc="lower center", bbox_to_anchor=(.5,.055), ncol=3, frameon=False)
    fig.text(.5, .025, "Magnets outside the central section are omitted. Coil circles are nominal wire envelopes. Mounts are incomplete.", ha="center", fontsize=9)
    fig.savefig(OUT / f"design-review-sections-{page+1}.png", dpi=160)
    plt.close(fig)
