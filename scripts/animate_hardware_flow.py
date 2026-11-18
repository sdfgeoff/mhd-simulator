"""Animate accepted native slices, without interpolating or inventing frames."""

import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.animation import FuncAnimation, PillowWriter
import numpy as np


def read_section(path):
    m = json.loads(path.read_text())
    if m["format_version"] != 1 or not m["moving_interface"]:
        raise ValueError("Expected accepted version1 moving-interface fields")
    nx, ny, nz = m["dims"]
    f = np.memmap(path.parent/"fluid.bin", dtype="<f4", mode="r").reshape(nz, ny, nx, 16)
    section = np.mean(f[:, (ny-1)//2:ny//2+1, :, 14], axis=1)
    solid = 1-np.mean(f[:, (ny-1)//2:ny//2+1, :, 12], axis=1)
    return m, section, solid


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("directory", type=Path)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--fps", type=int, default=8)
    a = p.parse_args()
    if a.output.exists() or not 1 <= a.fps <= 30:
        p.error("Choose a new output and1..30 frames per second")
    paths = sorted(a.directory.glob("frame-*/metadata.json"))
    if len(paths) < 2:
        p.error("At least two accepted frames are required")
    frames = [read_section(path) for path in paths]
    first = frames[0][0]
    for m, _, _ in frames:
        if (m["dims"], m["cell_m"], m["origin_m"]) != (first["dims"], first["cell_m"], first["origin_m"]):
            raise ValueError("Animation frames must share a grid")
    occupied = [np.nonzero(section >= .5)[0] for _, section, _ in frames]
    lower = min(-1.8, min((first["origin_m"][2]+(indices.min()+.5)*first["cell_m"])*1000 for indices in occupied if len(indices))-.4)
    h = first["cell_m"]*1000
    x, _, z = np.array(first["origin_m"])*1000
    extent = [x, x+first["dims"][0]*h, z, z+first["dims"][2]*h]
    fig, ax = plt.subplots(figsize=(4.7, 6.5), layout="constrained")
    picture = ax.imshow(frames[0][1], extent=extent, origin="lower", interpolation="nearest", vmin=0, vmax=1, cmap="Blues")
    rgba = np.empty((*frames[0][2].shape, 4))
    rgba[..., :3] = .55
    rgba[..., 3] = np.clip(frames[0][2], 0, 1)
    ax.imshow(rgba, extent=extent, origin="lower", interpolation="nearest")
    ax.axhline(0, color="darkorange", ls=":", lw=1)
    ax.set(xlim=(-.8, .8), ylim=(lower, 1.8), xlabel="x (mm)", ylabel="z (mm)", aspect="equal")
    title = ax.set_title("")
    fig.colorbar(picture, ax=ax, label="Liquid fraction", shrink=.65)
    fig.suptitle(f"Accepted native tip sections · {h*1000:.0f} µm grid\nGrey: solid mask; orange: nominal exit", fontsize=11)
    fig.supxlabel("Saved frames only · playback is not real time", fontsize=9)

    def update(i):
        m, section, _ = frames[i]
        picture.set_data(section)
        title.set_text(f"t = {m['time_s']*1e6:.0f} µs · step {m['step']}")
        return picture, title

    movie = FuncAnimation(fig, update, frames=len(frames), interval=1000/a.fps, blit=False)
    a.output.parent.mkdir(parents=True, exist_ok=True)
    movie.save(a.output, writer=PillowWriter(fps=a.fps), dpi=110)
    print(json.dumps({"output": str(a.output), "native_frames": len(frames), "physical_time_us": [frames[0][0]["time_s"]*1e6, frames[-1][0]["time_s"]*1e6], "playback_fps": a.fps}))


if __name__ == "__main__":
    main()
