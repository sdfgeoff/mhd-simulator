"""Plot exact CAD sections of the proposed heater interruption; no native field."""
import json
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.collections import LineCollection
from prepare_lower_coil_package import accessor_bytes, section_lines

root = Path("artifacts/hardware-research")
paths = [Path("hardware/designs/lowered-v6-coil-review/geometry.gltf"),
         Path("hardware/designs/lowered-v6-heater-slot-review/geometry.gltf")]
fig, axes = plt.subplots(1, 2, figsize=(11, 5.5), constrained_layout=True)
for ax, path, label in zip(axes, paths, ["Completed unslotted control", "Proposed radial-slot sensitivity"]):
    doc = json.loads(path.read_text())
    binary = (path.parent / doc["buffers"][0]["uri"]).read_bytes()
    mesh = next(m for m in doc["meshes"] if m["name"].startswith("Drawing-based heater"))
    data = accessor_bytes(doc, binary, mesh["primitives"][0]["attributes"]["POSITION"])
    points = np.frombuffer(data, dtype="<f4").reshape(-1, 3, 3).astype(float) * 1000
    # Reuse the exact y=0 section routine after changing axes: z=6mm -> y=0.
    transformed = points[:, :, [0, 2, 1]].copy()
    transformed[:, :, 1] -= 6
    ax.add_collection(LineCollection(section_lines(transformed), colors="#555555", linewidths=1.5))
    ax.set(xlim=(-16, 9), ylim=(-9, 9), xlabel="x (mm)", ylabel="y (mm)", title=label)
    ax.set_aspect("equal")
    ax.grid(alpha=.2)
    ax.annotate("Smooth M6 bore\n(thread omitted)", xy=(-2.1, -2.1), xytext=(-11, -6.5),
                arrowprops={"arrowstyle": "->"}, fontsize=9)
axes[1].annotate("0.8 mm through-slot\nto outer face", xy=(4, .4), xytext=(-11, 6.5),
                 arrowprops={"arrowstyle": "->", "color": "#b44b25"}, color="#b44b25", fontsize=10)
fig.suptitle("Interrupt the heater current path with one accessible cut\nExact CAD section at z = 6 mm; nozzle, liquid and coil remain unchanged", fontsize=13)
fig.supxlabel("Prepared geometry only: no EM benefit, thread strength or thermal performance established.\nMetal bridges through threads/inserts/fasteners remain an assembly concern.", fontsize=10)
output = root / "diagrams/lowered-coil-heater-slot-cad.png"
fig.savefig(output, dpi=160)
print(output)
