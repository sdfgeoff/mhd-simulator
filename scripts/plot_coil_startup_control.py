"""Plot completed coarse coil startup proxies; no experimental-retention claim."""
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
root = Path("artifacts/hardware-research")
fig, axes = plt.subplots(1, 2, figsize=(11, 5.5))
fig.subplots_adjust(left=.08, right=.97, top=.80, bottom=.25, wspace=.28)
for current, color in [(0, "#64748b"), (250, "#176b9c")]:
    directory = root / f"coil-low-fill-flow-v1-{current}A-h020"
    status = json.loads((directory / "run-status.json").read_text())
    analysis = json.loads((directory / "flow-analysis.json").read_text())
    if status["status"] != "complete" or abs(status["time_s"]-.0021) > 1e-12:
        raise ValueError("Require completed2.1ms startup controls")
    rows = analysis["frames"]
    if any(regions for row in rows for regions in row["connectivity_thresholds"].values()):
        raise ValueError("This no-resolved-drop figure requires no detached regions")
    times = [r["time_us"]/1000 for r in rows]
    initial = rows[0]["exit_plane_alpha_area_mm2"]
    axes[0].plot(times, [r["exit_plane_alpha_area_mm2"]/initial for r in rows],
                 "o-", color=color, label=f"{current} A peak")
    axes[1].plot(times, [r["exit_plane_mean_uz_m_s"] for r in rows],
                 "o-", color=color, label=f"{current} A peak")
for ax in axes:
    ax.axvspan(0, .110, color="#d6a854", alpha=.22, label="Driven pulse window")
    ax.set(xlabel="Time (ms)", xlim=(0, 2.1))
    ax.grid(alpha=.18)
    ax.spines[["top", "right"]].set_visible(False)
axes[0].set(ylabel="Liquid section proxy / initial proxy", ylim=(0, 1.05))
axes[1].set(ylabel="Upward cell-centred velocity (m/s)")
axes[0].legend(frameon=False, fontsize=9)
fig.suptitle("Single-pulse coil: no resolved drop in this coarse startup control", fontsize=14)
fig.text(.5, .865, "Same normal unified solver · 250 A, 10 µs rise / 100 µs fall · lower free level", ha="center", fontsize=11)
fig.text(.5, .10, "200 µm cells: 2.5 across the outlet; the 0.15 mm land is subcell. Samples are from the first cell plane at z = 0.1 mm.\n"
         "These are section/velocity proxies, not exact face fluxes. Both unpowered and driven menisci remain in motion.\n"
         "This result neither establishes experimental retention nor rules out single-coil jetting.", ha="center", fontsize=9)
args.output.parent.mkdir(parents=True, exist_ok=True)
fig.savefig(args.output, dpi=160)
print(args.output)
