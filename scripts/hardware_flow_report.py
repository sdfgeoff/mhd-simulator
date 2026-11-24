"""Measure accepted native RunState exports; no additional fluid solver.

Usage: python scripts/hardware_flow_report.py RUN_DIRECTORY [--plot]
Detached regions use six-neighbour connectivity at several alpha thresholds; this is
a diagnostic, not a grid-independent pinch-off certification.
"""

import argparse
import csv
import json
import hashlib
import tempfile
import zipfile
import scipy
from pathlib import Path

import numpy as np
from scipy import ndimage
from native_field_io import read_native_fields


def components(alpha, fields, h, origin, threshold, exit_z=0.):
    labels, count = ndimage.label(alpha >= threshold)
    sizes = np.bincount(labels.ravel(), weights=alpha.ravel(), minlength=count + 1)
    sizes[0] = 0
    main_id = int(sizes.argmax())
    result = []
    shape = np.array(alpha.shape)
    candidates = np.flatnonzero(sizes >= 1)
    if len(candidates) <= 1:
        return result
    boxes = ndimage.find_objects(labels, max_label=count)
    for label in candidates:
        if label == main_id:
            continue
        box = boxes[label - 1]
        indices = np.array(np.nonzero(labels[box] == label))
        indices += np.array([axis.start for axis in box])[:, None]
        positions = origin[:, None] + (indices[::-1] + 0.5) * h
        if not np.any(positions[2] < exit_z):
            continue
        weights = alpha[tuple(indices)]
        volume = float(sizes[label] * h**3)
        centre = np.sum(positions * weights, axis=1) / weights.sum()
        velocity = (
            np.sum(fields[..., :3][tuple(indices)] * weights[:, None], axis=0)
            / weights.sum()
        )
        result.append(
            {
                "volume_nl": volume * 1e12,
                "equivalent_diameter_mm": (6 * volume / np.pi) ** (1 / 3) * 1000,
                "cells": int(indices.shape[1]),
                "centre_mm": (centre * 1000).tolist(),
                "velocity_m_s": velocity.tolist(),
                "touches_domain_boundary": bool(
                    np.any(indices == 0) or np.any(indices == shape[:, None] - 1)
                ),
            }
        )
    return result


def analyze_frame(path, m, exit_z):
    nx, ny, nz = m["dims"]
    h = m["cell_m"]
    origin = np.array(m["origin_m"])
    f = read_native_fields(path.parent / "fluid.bin", (nz, ny, nx, 16))
    alpha = f[..., 14]
    z = origin[2] + (np.arange(nz) + 0.5) * h
    exit_planes = np.flatnonzero(z >= exit_z)
    exit_plane = int(exit_planes[0]) if len(exit_planes) else None
    exit_weight = (
        float(alpha[exit_plane].sum(dtype=np.float64))
        if exit_plane is not None
        else 0.0
    )
    exit_uz = (
        float(
            np.sum(alpha[exit_plane] * f[exit_plane, :, :, 2], dtype=np.float64)
            / exit_weight
        )
        if exit_weight > 0
        else None
    )
    exit_pressure = (
        float(
            np.sum(alpha[exit_plane] * f[exit_plane, :, :, 4], dtype=np.float64)
            / exit_weight
        )
        if exit_weight > 0
        else None
    )
    below = np.clip((exit_z - (z - h / 2)) / h, 0, 1)
    below_guard = np.clip((exit_z-h - (z - h / 2)) / h, 0, 1)
    thresholds = {
        str(t): components(alpha, f, h, origin, t, exit_z) for t in [0.01, 0.25, 0.5, 0.75]
    }
    detached = thresholds["0.5"]
    occupied_z = z[np.any(alpha >= 0.5, axis=(1, 2))]
    boundary_maxima = {}
    for axis, label in enumerate(["z", "y", "x"]):
        for index, side in [(0, "min"), (-1, "max")]:
            boundary_maxima[f"{label}_{side}"] = float(np.take(alpha, index, axis=axis).max())
    row = {
        "step": m["step"],
        "time_us": m["time_s"] * 1e6,
        "first_cell_plane_at_or_above_exit_z_mm": (
            float(z[exit_plane] * 1000) if exit_plane is not None else None
        ),
        "exit_plane_alpha_area_mm2": exit_weight * h**2 * 1e6,
        "exit_plane_mean_uz_m_s": exit_uz,
        "exit_plane_mean_pressure_pa": exit_pressure,
        "exit_plane_downward_flux_proxy_nl_s": (
            -exit_uz * exit_weight * h**2 * 1e12 if exit_uz is not None else None
        ),
        "liquid_volume_nl": float(alpha.sum(dtype=np.float64) * h**3 * 1e12),
        "minimum_liquid_fraction": float(alpha.min()),
        "maximum_liquid_fraction": float(alpha.max()),
        "below_tip_maximum_liquid_fraction_at_cell_centres": float(alpha[z < exit_z].max()) if np.any(z < exit_z) else None,
        "boundary_maximum_liquid_fraction": boundary_maxima,
        "liquid_touches_domain_boundary_alpha01": any(v >= .01 for v in boundary_maxima.values()),
        "below_tip_nl": float(
            np.einsum("zyx,z->", alpha, below, dtype=np.float64) * h**3 * 1e12
        ),
        "below_one_cell_guard_nl": float(
            np.einsum("zyx,z->", alpha, below_guard, dtype=np.float64) * h**3 * 1e12
        ),
        "front_z_mm_alpha_half": (
            float(occupied_z.min() * 1000) if len(occupied_z) else None
        ),
        "detached_below_tip": detached,
        "connectivity_thresholds": thresholds,
    }
    section = np.mean(alpha[:, (ny - 1) // 2 : ny // 2 + 1, :], axis=1)
    solid = 1 - np.mean(f[:, (ny - 1) // 2 : ny // 2 + 1, :, 12], axis=1)
    return row, section, solid


def cached_frame(path, metadata, exit_z, cache_dir, version):
    field = path.parent / "fluid.bin"
    if not field.exists():
        field = field.with_suffix(".bin.zst")
    stat = field.stat()
    identity = (version, np.__version__, scipy.__version__, metadata, exit_z, field.name,
                stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns)
    signature = hashlib.sha256(json.dumps(identity, sort_keys=True).encode()).hexdigest()
    cache = cache_dir / (path.parent.name + ".npz")
    if cache.exists():
        try:
            with np.load(cache, allow_pickle=False) as saved:
                if saved["signature"].item() == signature:
                    return json.loads(saved["row"].item()), saved["section"], saved["solid"]
        except (ValueError, KeyError, EOFError, zipfile.BadZipFile):
            pass
    row, section, solid = analyze_frame(path, metadata, exit_z)
    with tempfile.NamedTemporaryFile(dir=cache_dir, delete=False) as staged:
        try:
            np.savez(staged, signature=signature, row=json.dumps(row), section=section, solid=solid)
            staged.close()
            Path(staged.name).replace(cache)
        finally:
            Path(staged.name).unlink(missing_ok=True)
    return row, section, solid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--plot", action="store_true")
    parser.add_argument("--pool-view", action="store_true", help="Include the initial pool and free surface in the section plot")
    parser.add_argument("--exit-z-mm",type=float,default=0.,help="Nominal CAD exit coordinate, default0mm")
    args = parser.parse_args()
    exit_z=args.exit_z_mm*.001
    if not np.isfinite(exit_z):raise ValueError("Finite exit coordinate required")
    records = sorted(args.directory.glob("frame-*/metadata.json"))
    if not records:
        raise SystemExit("No accepted native exports")
    rows, sections = [], []
    cache_dir = args.directory / ".flow-report-cache"
    cache_dir.mkdir(exist_ok=True)
    version = hashlib.sha256(Path(__file__).read_bytes() + Path(__file__).with_name("native_field_io.py").read_bytes()).hexdigest()
    initial_aperture = None
    for path in records:
        m = json.loads(path.read_text())
        if m["format_version"] != 1 or not m["moving_interface"]:
            raise ValueError("Expected version-1 moving-interface export")
        row, section, solid = cached_frame(path, m, exit_z, cache_dir, version)
        if initial_aperture is None and row["first_cell_plane_at_or_above_exit_z_mm"] is not None:
            initial_aperture = {
                "plane_z_mm": row["first_cell_plane_at_or_above_exit_z_mm"],
                "alpha_area_mm2": row["exit_plane_alpha_area_mm2"],
                "meaning": "Initial liquid-fraction area at the first cell-centre plane at or above the nominal exit; a voxel-aperture proxy, not the CAD area.",
            }
        row["below_tip_change_nl"] = row["below_tip_nl"] - (
            rows[0]["below_tip_nl"] if rows else row["below_tip_nl"]
        )
        row["relative_liquid_volume_change"] = row["liquid_volume_nl"] / (
            rows[0]["liquid_volume_nl"] if rows else row["liquid_volume_nl"]
        ) - 1
        rows.append(row)
        sections.append((m, section, solid))
    (args.directory / "flow-analysis.json").write_text(
        json.dumps(
            {
                "method": "Accepted native exports; six-neighbour components at several alpha thresholds. Component volumes include alpha only above that threshold. Below-tip volume includes all alpha. Exit flux is a cell-centred proxy, not a native MAC-face flux. No convergence or wetting claim.",
                "initial_aperture_proxy": initial_aperture,
                "component_minimum_alpha_weighted_cells": 1,
                "component_minimum_volume_nl": sections[0][0]["cell_m"]**3*1e12,
                "frames": rows,
            },
            indent=2,
        )
    )
    with (args.directory / "flow-analysis.csv").open("w") as out:
        scalar_fields = [k for k, v in rows[0].items() if not isinstance(v, (list, dict))]
        writer = csv.DictWriter(
            out,
            fieldnames=scalar_fields,
        )
        writer.writeheader()
        writer.writerows(
            {k: row[k] for k in scalar_fields}
            for row in rows
        )
    print(json.dumps(rows[-1], indent=2))
    if args.plot:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt

        indices = np.unique(np.linspace(0, len(sections) - 1, 4).round().astype(int))
        fig, axes = plt.subplots(
            1,
            len(indices),
            figsize=(3.5 * len(indices), 7 if args.pool_view else 5),
            squeeze=False,
            constrained_layout=True,
        )
        lower_z = min(
            -1.8,
            min(r["front_z_mm_alpha_half"] for r in rows if r["front_z_mm_alpha_half"] is not None) - 0.4,
        )
        xlimits, upper_z = (-0.8, 0.8), 1.8
        if args.pool_view:
            initial, section, _ = sections[0]
            occupied = np.nonzero(section >= .5)
            if len(occupied[0]):
                h = initial["cell_m"] * 1000
                origin = np.array(initial["origin_m"]) * 1000
                xlimits = (origin[0] + occupied[1].min()*h - .3, origin[0] + (occupied[1].max()+1)*h + .3)
                upper_z = origin[2] + (occupied[0].max()+1)*h + .3
            for m, section, _ in sections:
                occupied = np.nonzero(section >= .5)
                if len(occupied[0]):
                    upper_z = max(upper_z, (m["origin_m"][2] + (occupied[0].max()+1)*m["cell_m"])*1000 + .3)
        for ax, index in zip(axes[0], indices):
            m, section, solid = sections[index]
            h = m["cell_m"] * 1000
            x, _, z = np.array(m["origin_m"]) * 1000
            picture = ax.imshow(
                section,
                origin="lower",
                interpolation="nearest",
                vmin=0,
                vmax=1,
                extent=[x, x + section.shape[1] * h, z, z + section.shape[0] * h],
                cmap="Blues",
            )
            rgba = np.empty((*solid.shape, 4))
            rgba[..., :3] = .55
            rgba[..., 3] = np.clip(solid, 0, 1)
            ax.imshow(rgba, origin="lower", interpolation="nearest", extent=picture.get_extent())
            ax.axhline(args.exit_z_mm, color="darkorange", lw=0.8, ls=":")
            ax.set(
                xlim=xlimits,
                ylim=(lower_z, upper_z),
                xlabel="x (mm)",
                ylabel="z (mm)",
                title=f"{m['time_s']*1e6:.1f} µs",
            )
            ax.set_aspect("equal")
        title = (
            "Current-driven MHD research trial (inductionless)"
            if (args.directory / "current-diagnostics.csv").exists()
            else "Computed EM + fluid/interface"
        )
        fig.suptitle(
            title
            + f" · central liquid fraction · {sections[0][0]['cell_m']*1e6:.0f} µm grid"
            + "\nOrange: nominal nozzle exit; grey: native impermeable solid mask; breakup requires refinement"
        )
        fig.colorbar(picture, ax=axes[0].tolist(), label="Liquid fraction", shrink=0.6)
        fig.savefig(args.directory / ("pool-sections.png" if args.pool_view else "tip-sections.png"), dpi=160, bbox_inches="tight")


if __name__ == "__main__":
    main()
