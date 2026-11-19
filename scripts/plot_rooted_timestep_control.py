"""Plot completed native timestep controls at a common physical time."""

import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from native_field_io import read_native_fields

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--study", choices=["timestep", "retract"], default="timestep")
parser.add_argument("--reference-retract", type=float, default=.25)
parser.add_argument("--candidate-retract", type=float, default=.5)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
root = Path("artifacts/hardware-research")
cases = [
    ("m6-rooted08-flow110-r25-motionabs1e-06A-h010-dt3", "100 µm · 3 µs", "v2 + 1 µA motion allowance"),
    ("m6-rooted08-flow110-r25-motionabs1e-06A-h010-dt1p5", "100 µm · 1.5 µs", "v2 + 1 µA motion allowance"),
    ("m6-rooted08-flow110-r25-chargefix-h0050", "50 µm · 1.5 µs", "earlier v1, relative-only gate"),
]
if args.study == "retract":
    cases = [(f"m6-rooted08-flow110-r{r*100:g}-motionabs1e-06A-h010-dt1p5",
              f"Reverse: {110*r:g} A", "100 µm · 1.5 µs")
             for r in [args.reference_retract, args.candidate_retract]]
fig, axes = plt.subplots(1, len(cases), figsize=(10 if len(cases) == 3 else 7.5, 7.8))
fig.subplots_adjust(top=.84, bottom=.28, left=.07, right=.88, wspace=.32)
for ax, (name, label, version) in zip(axes, cases):
    directory = root / name
    status = json.loads((directory / "run-status.json").read_text())
    if status["status"] != "complete" or abs(status["time_s"] - .0036) > 1e-12:
        raise ValueError("Require completed 3.6 ms controls")
    metadata = directory / f"frame-{status['accepted_steps']:08}/metadata.json"
    m = json.loads(metadata.read_text())
    if abs(m["time_s"] - status["time_s"]) > 1e-12 or not m["moving_interface"]:
        raise ValueError("Require accepted final moving-interface exports")
    nx, ny, nz = m["dims"]
    f = read_native_fields(metadata.parent / "fluid.bin", (nz, ny, nx, 16))
    alpha = np.mean(f[:, (ny-1)//2:ny//2+1, :, 14], axis=1)
    solid = 1 - np.mean(f[:, (ny-1)//2:ny//2+1, :, 12], axis=1)
    h = m["cell_m"] * 1000
    origin = np.array(m["origin_m"]) * 1000
    extent = [origin[0], origin[0]+nx*h, origin[2], origin[2]+nz*h]
    picture = ax.imshow(alpha, origin="lower", extent=extent, cmap="Blues",
                        vmin=0, vmax=1, interpolation="nearest")
    rgba = np.full((*solid.shape, 4), .55)
    rgba[..., 3] = np.clip(solid, 0, 1)
    ax.imshow(rgba, origin="lower", extent=extent, interpolation="nearest")
    ax.axhline(0, color="darkorange", ls=":", lw=.8)
    ax.set(xlim=(-1.15, 1.15), ylim=(-4.6, .8), xlabel="x (mm)",
           ylabel="z (mm)", title=f"{label}\n{version}")
    ax.set_aspect("equal")
    analysis = json.loads((directory / "flow-analysis.json").read_text())
    regions = analysis["frames"][-1]["connectivity_thresholds"]["0.5"]
    description = "\n".join(f"D = {r['equivalent_diameter_mm']:.3f} mm; "
                            f"down = {-r['velocity_m_s'][2]:.2f} m/s" for r in regions)
    ax.text(.5, -.17, description, transform=ax.transAxes, ha="center", va="top", fontsize=9)
    del f
fig.suptitle("Same 110 A pulse at 3.6 ms — isolate timestep before grid refinement" if args.study == "timestep" else "Stronger reverse-current pullback — accepted state at 3.6 ms", fontsize=14)
color = fig.add_axes([.91, .28, .018, .43])
fig.colorbar(picture, cax=color, label="Liquid fraction")
caption = "Left pair: only the timestep changes. Right: finer grid at matched timestep, with an earlier charge solver.\n" if args.study == "timestep" else "Same geometry and forward pulse; reverse amplitude changes.\n"
fig.text(.5, .07, caption + "Two central XZ slices averaged; grey = solid, orange = exit. Diameters use 3D α ≥ 0.5 connectivity.\n"
         "One pulse only; no claim of converged breakup or repetitive hardware performance.", ha="center", fontsize=9)
args.output.parent.mkdir(parents=True, exist_ok=True)
fig.savefig(args.output, dpi=160)
print(args.output)
