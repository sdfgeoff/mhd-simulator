"""Compare accepted cold fields for matched-domain V6 contact variants."""

import argparse
import json
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "artifacts/hardware-research"


def heat_grid_detail():
    """Show actual cold Joule fields as an adiabatic diagnostic, not temperatures."""
    fig, axes = plt.subplots(1, 3, figsize=(14, 5.5), layout="constrained")
    for index, (tag, color) in enumerate([("h010", "#3b6ea8"), ("h0050", "#c16b2b")]):
        directory = DATA / ("v6-tip-wide-cold-30A-" + tag)
        report = json.loads((directory / "report.json").read_text())
        project = json.loads((ROOT / report["project"]).read_text())
        steel = next(m for m in project["materials"] if m["id"] == 14)
        grid = json.loads((directory / "grid.json").read_text())
        nx, ny, nz = grid["dims"]
        h = grid["cell_m"]
        z = grid["origin_m"][2] + (np.arange(nz) + .5) * h
        y = grid["origin_m"][1] + (np.arange(ny) + .5) * h
        ids = np.fromfile(directory / "material-ids.bin", dtype="<u4").reshape(nz, ny, nx)
        field = np.memmap(directory / "em.bin", dtype="<f4", mode="r").reshape(nz, ny, nx, 16)
        scale = 9.075 / report["current_A"]**2 / (steel["density_kg_m3"] * steel["thermal"]["heat_capacity_j_kg_k"])
        rise = np.max(np.where(ids == 14, field[..., 11] * scale, 0), axis=2)
        iz, iy = np.flatnonzero((z >= 0) & (z < .001)), np.flatnonzero(abs(y) < .0007)
        ax = axes[index]
        image = ax.imshow(rise[np.ix_(iz, iy)], origin="lower", interpolation="nearest",
                          extent=[(y[iy[0]] - h / 2) * 1000, (y[iy[-1]] + h / 2) * 1000,
                                  (z[iz[0]] - h / 2) * 1000, (z[iz[-1]] + h / 2) * 1000],
                          vmin=0, vmax=36, cmap="magma", aspect="equal")
        ax.set(title=f"{h * 1e6:.0f} µm grid · max over x", xlabel="y (mm)", ylabel="z above exit (mm)")
        axes[2].plot(z[iz] * 1000, rise.max(axis=1)[iz], "o-", color=color,
                     label=f"{h * 1e6:.0f} µm", markersize=4)
    fig.colorbar(image, ax=list(axes[:2]), shrink=.8, label="Initial-field adiabatic rise (K)")
    axes[2].set(title="Peak across each horizontal plane", xlabel="z above exit (mm)",
                ylabel="Initial-field adiabatic rise (K)", ylim=(0, 38))
    axes[2].legend()
    axes[2].grid(alpha=.2)
    fig.suptitle("V6 wider-contact hotspot is not grid converged\n200 A / 250 µs push, −50 A return · fixed cold current paths; no heat-transfer solve", fontsize=13)
    output = DATA / "diagrams"
    output.mkdir(exist_ok=True)
    fig.savefig(output / "v6-tip-heat-grid-refinement.png", dpi=180)
    plt.close(fig)


def main():
    cases = [
        ("v6-tip-pool35-medium-cold-30A-h010", "0.4 wide × 0.4 high", "#3b6ea8"),
        ("v6-tip-wide-cold-30A-h010", "0.8 wide × 0.4 high", "#31876a"),
        ("v6-tip-pads08-cold-30A-h010", "0.8 wide × 0.8 high", "#c47735"),
    ]
    fig, axes = plt.subplots(1, 2, figsize=(12, 5.2), constrained_layout=True)
    summaries = []
    for name, label, color in cases:
        directory = DATA / name
        report = json.loads((directory / "report.json").read_text())
        project = json.loads((ROOT / report["project"]).read_text())
        steel = next(m for m in project["materials"] if m["id"] == 14)
        capacity = steel["density_kg_m3"] * steel["thermal"]["heat_capacity_j_kg_k"]
        grid = json.loads((directory / "grid.json").read_text())
        nx, ny, nz = grid["dims"]
        h = grid["cell_m"]
        z = grid["origin_m"][2] + (np.arange(nz) + .5) * h
        ids = np.fromfile(directory / "material-ids.bin", dtype="<u4").reshape(nz, ny, nx)
        field = np.memmap(directory / "em.bin", dtype="<f4", mode="r").reshape(nz, ny, nx, 16)
        liquid = ids == 4
        count = liquid.sum(axis=(1, 2))
        force = np.sum(field[..., 10] * liquid, axis=(1, 2), dtype=np.float64)
        mean_force = -np.divide(force, count, out=np.zeros(nz), where=count > 0)
        peak_heat = np.max(np.where(ids == 14, field[..., 11], 0), axis=(1, 2))
        # Exact integral for the conduction_flow 200 A/250 us/r=0.25 waveform,
        # including 20 us rise/fall ramps on both push and return.
        i2dt = 9.075
        rise = peak_heat * (i2dt / report["current_A"]**2) / capacity
        selected = (z > 0) & (z < .0014)
        axes[0].plot(z[selected] * 1000, mean_force[selected] * 1e-6, "o-", ms=3, color=color, label=label)
        axes[1].plot(z[selected] * 1000, rise[selected], "o-", ms=3, color=color, label=label)
        summaries.append({
            "artifacts": str(directory.relative_to(ROOT)),
            "pad_box_mm": label,
            "land_axial_force_area_integral_pa_at_30a": float(np.sum(mean_force[(z >= 0) & (z < .0006)]) * h),
            "initial_field_maximum_steel_rise_k": float(rise.max()),
        })
    for ax in axes:
        ax.axvspan(0, .6, color="#dfe6ec", alpha=.5)
        ax.set(xlabel="Height above outlet (mm)", xlim=(0, 1.4), ylim=(0, None))
        ax.grid(alpha=.2)
    axes[0].set(title="Where the liquid receives the push · 30 A", ylabel="Downward axial force per volume (kPa/mm)")
    axes[1].set(title="Initial-field steel heating · 200 A pulse", ylabel="No-diffusion temperature rise (K)")
    axes[0].legend(title="Nominal contact box (mm)", fontsize=9)
    axes[1].text(.97, .95, "Shaded: outlet land\n250 µs push, −50 A return", transform=axes[1].transAxes, ha="right", va="top", fontsize=9)
    fig.suptitle("V6 contact geometry changes where the force acts\nAccepted cold GPU fields · 100 µm grid · no pressure or heat-transfer solve", fontsize=13)
    output = DATA / "diagrams"
    output.mkdir(exist_ok=True)
    fig.savefig(output / "v6-contact-force-heat.png", dpi=180)
    (output / "v6-contact-force-heat.json").write_text(json.dumps({
        "method": __doc__,
        "limits": "Cold area-mean axial force only; the axial force/area integral is not solved outlet pressure. Heating retains initial current paths and properties and excludes diffusion and external contacts.",
        "cases": summaries,
    }, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--heat-grid", action="store_true")
    args = parser.parse_args()
    heat_grid_detail() if args.heat_grid else main()
