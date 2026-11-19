"""Inspect native slit startup, including spreading hidden by one section plane."""

import argparse
import hashlib
import json
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from native_field_io import read_native_fields


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--plot", required=True, type=Path)
    args = parser.parse_args()
    rows, sections = [], []
    paths = sorted(args.directory.glob("frame-*/metadata.json"))
    if not paths:
        raise RuntimeError("No accepted native frames")
    for path in paths:
        m = json.loads(path.read_text())
        if not m["moving_interface"]:
            raise RuntimeError("Moving-interface export required")
        nx, ny, nz = m["dims"]
        h = m["cell_m"]
        origin = np.asarray(m["origin_m"])
        f = read_native_fields(path.parent / "fluid.bin", (nz, ny, nx, 16))
        alpha = f[..., 14]
        x, y, z = [o + (np.arange(n) + 0.5) * h for o, n in zip(origin, [nx, ny, nz])]
        slot = (z >= 0) & (z <= 0.5e-3)
        outside = np.sqrt(x[None, :] ** 2 + y[:, None] ** 2) >= 0.55e-3
        w = np.clip(alpha.astype(float), 0, 1)
        v2 = np.sum(np.asarray(f[..., :3], dtype=float) ** 2, axis=-1)
        land, land_v2 = w[slot], v2[slot]
        row = {
            "step": m["step"],
            "time_us": m["time_s"] * 1e6,
            "metadata_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "slot_outside_radius055mm_nl": float(
                np.sum(alpha[slot][:, outside], dtype=float) * h**3 * 1e12
            ),
            "liquid_weighted_RMS_speed_m_s": float(np.sqrt(np.sum(w * v2) / w.sum())),
            "land_liquid_weighted_RMS_speed_m_s": (
                float(np.sqrt(np.sum(land * land_v2) / land.sum()))
                if land.sum() > 0
                else None
            ),
            "kinetic_proxy_J_rho7000": float(0.5 * 7000 * np.sum(w * v2) * h**3),
        }
        rows.append(row)
        ix = np.argsort(abs(x))[:2]
        iy = np.argsort(abs(y))[:2]
        # Mean of the two native planes straddling the requested centreline.
        sections.append(
            (
                m,
                alpha[:, iy, :].mean(axis=1),
                1 - f[:, iy, :, 12].mean(axis=1),
                alpha[:, :, ix].mean(axis=2),
                1 - f[:, :, ix, 12].mean(axis=2),
            )
        )
    result = {
        "status": "Completed native slit diagnostics; not equilibrium or retention qualification",
        "source": str(args.directory),
        "native_run_status": json.loads(
            (args.directory / "run-status.json").read_text()
        ),
        "method": "Accepted native alpha and cell-centred velocity. Land z0..0.5mm; outside-radius volume uses cell centres r>=0.55mm to avoid initial bore-edge samples. Native signed alpha is retained for volume; RMS and kinetic proxy use alpha clipped0..1 for weights. The kinetic proxy assumes rho7000 and is not the MAC solver energy ledger.",
        "frames": rows,
        "limits": [
            "Two100umcells across0.2mm gap; no converged contact-line/spurious-current assessment.",
            "No calibrated wetting/contact angle, oxide or thermal evolution.",
            "Persisting motion and increasing cell-velocity kinetic proxy do not establish stationary retention.",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    selected = [
        0,
        min(range(len(rows)), key=lambda i: abs(rows[i]["time_us"] - 1000)),
        len(rows) - 1,
    ]
    fig, axes = plt.subplots(2, 3, figsize=(11, 7), constrained_layout=True)
    for column, index in enumerate(selected):
        m, across, solid_across, along, solid_along = sections[index]
        ox, oy, oz = np.array(m["origin_m"]) * 1000
        nx, ny, nz = m["dims"]
        h = m["cell_m"] * 1000
        for row, (section, solid, label, low, count) in enumerate(
            [
                (across, solid_across, "x (mm), across slit", ox, nx),
                (along, solid_along, "y (mm), along slit", oy, ny),
            ]
        ):
            ax = axes[row, column]
            extent = [low, low + count * h, oz, oz + nz * h]
            im = ax.imshow(
                np.clip(section, 0, 1),
                extent=extent,
                origin="lower",
                interpolation="nearest",
                cmap="Blues",
                vmin=0,
                vmax=1,
            )
            rgba = np.empty((*solid.shape, 4))
            rgba[..., :3] = 0.42
            rgba[..., 3] = np.clip(solid, 0, 1)
            ax.imshow(rgba, extent=extent, origin="lower", interpolation="nearest")
            ax.axhline(0, color="#d47918", lw=0.8)
            ax.set(
                xlim=(-1.2, 1.2),
                ylim=(-0.3, 1.3),
                xlabel=label,
                ylabel="z (mm)",
                title=f"{m['time_s'] * 1000:.3f} ms",
                aspect="equal",
            )
    fig.colorbar(im, ax=axes, label="Native liquid fraction", shrink=0.7)
    fig.suptitle(
        "Unpowered slit: liquid spreads under the wall, then retracts\nTwo orthogonal sections through the same native 100 µm run",
        fontsize=14,
    )
    fig.supxlabel(
        "Blue: liquid; grey: impermeable solid. Mean of adjacent centre planes. Coarse startup test; wetting and retention unqualified.",
        fontsize=9,
    )
    args.plot.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.plot, dpi=180)
    print(args.output)
    print(args.plot)


if __name__ == "__main__":
    main()
