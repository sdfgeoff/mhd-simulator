"""Plot actual CAD sections and accepted native bias-field screening values."""
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

from draw_hardware_layouts import load, section
from make_hardware import heater
from make_v6_conduction import as_solid


def main():
    root = Path("artifacts/hardware-research")
    rows = {}
    for name in ["pole-clearance-v3-results.json", "pole-clearance-v4-results.json"]:
        rows.update({r["label"]: r for r in json.loads((root / name).read_text())["cases"]})
    choices = [("original-disc", "Current 10 × 5 mm discs"),
               ("raised-disc-machined", "Raised discs; shoulder clearance flats"),
               ("stock-bar-stack", "Short poles: two 25 × 5 × 5 mm bars per side")]
    fig, axes = plt.subplots(1, 3, figsize=(15, 9))
    fig.subplots_adjust(left=.055, right=.985, top=.81, bottom=.17, wspace=.28)
    obstacle = as_solid(heater(6.5, 18.))
    for ax, (key, title) in zip(axes, choices):
        row = rows[key]
        assert row["status"] == "complete bias-field screen" and not row["collisions"]
        assert row["reference_heater_overlap_mm3"] < 1e-6
        name = Path(row["project"]).name.removesuffix(".mhd.json")
        parts, _ = load(name)
        section(ax, parts + [("Reference heater", [], obstacle)], "yz")
        projection = row["projection_below_exit_mm"]
        ax.axhline(0, color="#c2762c", linestyle=":", linewidth=1.2)
        ax.annotate("", xy=(0, -projection), xytext=(0, 0),
                    arrowprops={"arrowstyle": "<->", "color": "#27735b", "lw": 1.5})
        if projection > 1:
            ax.text(.4, -projection / 2, f"{projection:.1f} mm", color="#27735b", fontsize=11)
        else:
            ax.annotate(f"{projection:.1f} mm projection", xy=(0, -projection / 2), xytext=(-10, -3),
                        fontsize=10, color="#27735b", arrowprops={"arrowstyle": "-", "color": "#27735b"})
        ax.set(xlim=(-13, 13), ylim=(-9, 19),
               title=f"{title}\nMean outlet-land |By| ≈ {abs(row['mean_B_T'][1]):.3f} T")
        ax.tick_params(labelsize=9)
    fig.suptitle("Reducing projection below the nozzle without losing the outlet field\nActual generated CAD; the short-pole option is wider, with nominal shoulder clearance", fontsize=16, weight="bold")
    fig.text(.5, .105, "Orange: nozzle exit. Grey heater is a clearance reference only. Raised options require 4 mm shoulder flats.", ha="center", fontsize=11)
    fig.text(.5, .065, "GPU field-only screen: same 0.95 T magnet remanence, µr = 1; mean over the 0–0.6 mm outlet land. No jetting claim.", ha="center", fontsize=10)
    fig.text(.5, .025, "Short poles: 25 mm in x, 10 mm total magnetic depth in y, 5 mm high in z. Seams, hot grade and tolerances remain unqualified.", ha="center", fontsize=10)
    (root / "diagrams").mkdir(exist_ok=True)
    fig.savefig(root / "diagrams/pole-clearance-comparison.png", dpi=180)
    plt.close(fig)


if __name__ == "__main__":
    main()
