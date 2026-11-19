"""Fourier synthesis of the existing closed-form conducting-cylinder limit.

No finite-difference PDE, nozzle, circuit or fluid solver. The external field is
prescribed independently of a realizable coil. This isolates pulse/shielding
physics and cannot establish jetting or a required coil current.
"""

import argparse
import json
from pathlib import Path

import numpy as np
from cylinder_induction_screen import MU, NODES, WEIGHTS, field_phasors, response


def waveforms(t):
    shapes = {}
    for name, rise, fall in [
        ("100/100 µs triangle", 100e-6, 100e-6),
        ("50/250 µs triangle", 50e-6, 250e-6),
    ]:
        shapes[name] = np.maximum(0, np.minimum(t / rise, (rise + fall - t) / fall))
    shapes["50 kHz, 300 µs burst"] = np.maximum(
        0, np.minimum(np.minimum(t / 40e-6, (300e-6 - t) / 40e-6), 1)
    ) * np.sin(2 * np.pi * 50e3 * t)
    return shapes


def transfer(dt, duration, a, b, wall_sigma):
    count = round(duration / dt)
    frequency = np.fft.rfftfreq(count, dt)
    r = a * (NODES + 1) / 2
    weights = a * WEIGHTS / 2
    sample_r = np.concatenate((r, [0, a]))
    outer_r = (a + b) / 2 + NODES * (b - a) / 2
    outer_weights = WEIGHTS * (b - a) / 2
    bc, dc = [], []
    ds = []
    boundary_error = interface_error = 0.0
    for f in frequency:
        core, shell = field_phasors(f, a, b, 2.2e6, wall_sigma, 1.0)
        field, derivative = core(sample_r)
        bc.append(field)
        dc.append(derivative)
        if shell is not None:
            _, derivative = shell(np.concatenate((outer_r, [b])))
            ds.append(derivative)
            inside, inside_derivative = core(a)
            outside, outside_derivative = shell(a)
            interface_error = max(interface_error, float(abs(inside - outside)))
            if f > 0:
                interface_error = max(
                    interface_error,
                    float(
                        abs(inside_derivative / 2.2e6 - outside_derivative / wall_sigma)
                        / max(abs(inside_derivative / 2.2e6), 1e-30)
                    ),
                )
            boundary_error = max(boundary_error, float(abs(shell(b)[0] - 1)))
        else:
            boundary_error = max(boundary_error, float(abs(core(a)[0] - 1)))
    if max(boundary_error, interface_error) > 1e-8:
        raise AssertionError("Analytical mode boundary/interface check failed")
    return (
        np.array(bc),
        np.array(dc),
        np.array(ds) if ds else None,
        r,
        weights,
        outer_r,
        outer_weights,
        b,
        {
            "maximum_unit_boundary_B_error": boundary_error,
            "maximum_interface_relative_error": interface_error,
        },
    )


def history(boundary, trans, dt, wall_sigma):
    bc, dc, ds, r, weights, outer_r, outer_weights, b, _ = trans
    spectral = np.fft.rfft(boundary)
    field = np.fft.irfft(bc * spectral[:, None], n=len(boundary), axis=0)
    current = -np.fft.irfft(dc * spectral[:, None], n=len(boundary), axis=0) / MU
    pressure = -np.sum(current[:, :-2] * field[:, :-2] * weights, axis=1)
    identity = (field[:, -1] ** 2 - field[:, -2] ** 2) / (2 * MU)
    error = float(np.max(abs(pressure - identity)) / max(np.max(abs(identity)), 1e-30))
    if error > 1e-7:
        raise AssertionError(
            "Instantaneous radial force/field identity failed: " + str(error)
        )
    liquid_heat = np.sum(
        current[:, :-2] ** 2 / 2.2e6 * (weights * 2 * np.pi * r), axis=1
    )
    shell_heat = np.zeros_like(liquid_heat)
    if ds is not None:
        shell_current = (
            -np.fft.irfft(ds * spectral[:, None], n=len(boundary), axis=0) / MU
        )
        shell_heat = np.sum(
            shell_current[:, :-1] ** 2
            / wall_sigma
            * (outer_weights * 2 * np.pi * outer_r),
            axis=1,
        )
        outer_current, outer_sigma = shell_current[:, -1], wall_sigma
    else:
        outer_current, outer_sigma = current[:, -1], 2.2e6
    inward_poynting = -2 * np.pi * b * outer_current * boundary / (outer_sigma * MU)
    net_input = float(inward_poynting.sum() * dt)
    heat = float((liquid_heat + shell_heat).sum() * dt)
    balance_error = abs(net_input - heat) / max(abs(net_input), abs(heat), 1e-30)
    if balance_error > 1e-7:
        raise AssertionError(
            "Periodic Poynting/Joule balance failed: " + str(balance_error)
        )
    return pressure, field[:, -2], liquid_heat, shell_heat, error, balance_error


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--plot-path", type=Path)
    parser.add_argument("--check-refinement", action="store_true")
    args = parser.parse_args()
    dt, duration, onset, amplitude = 0.5e-6, 2e-3, 250e-6, 0.25
    t = np.arange(round(duration / dt)) * dt - onset
    results, plotted, refinement = [], [], []
    for label, a, b, sigma in [
        ("6 mm bore, 0.5 mm steel wall", 3e-3, 3.5e-3, 1.3e6),
        ("3.2 mm bore, 1.9 mm steel wall", 1.6e-3, 3.5e-3, 1.3e6),
        ("6 mm bore, insulating wall", 3e-3, 3e-3, 0.0),
    ]:
        trans = transfer(dt, duration, a, b, sigma)
        # Exact harmonic benchmark against the pre-existing analytical AC result.
        steady = amplitude * np.sin(2 * np.pi * 50e3 * np.arange(len(t)) * dt)
        p, *_ = history(steady, trans, dt, sigma)
        expected = response(50e3, a, b, 2.2e6, sigma, amplitude)[
            "radial_axis_to_wall_pressure_scale_Pa"
        ]
        steady_error = float(abs(np.mean(p) / expected - 1))
        if steady_error > 1e-7:
            raise AssertionError("Steady analytical AC benchmark failed")
        cases, curves = [], []
        for name, waveform in waveforms(t).items():
            p, axis_b, liquid_heat, shell_heat, error, power_error = history(
                amplitude * waveform, trans, dt, sigma
            )
            curves.append((name, p))
            cases.append(
                {
                    "waveform": name,
                    "inward_radial_pressure_difference_peak_Pa": float(p.max()),
                    "outward_radial_pressure_difference_peak_Pa": float(p.min()),
                    "positive_radial_pressure_impulse_Pa_s": float(
                        np.maximum(p, 0).sum() * dt
                    ),
                    "negative_radial_pressure_impulse_Pa_s": float(
                        np.minimum(p, 0).sum() * dt
                    ),
                    "liquid_heat_J_per_m": float(liquid_heat.sum() * dt),
                    "wall_heat_J_per_m": float(shell_heat.sum() * dt),
                    "liquid_axis_abs_B_peak_T": float(np.max(abs(axis_b))),
                    "force_field_identity_relative_error": error,
                    "periodic_Poynting_Joule_balance_relative_error": power_error,
                    "pre_onset_axis_abs_B_peak_T_before_minus5us": float(
                        np.max(abs(axis_b[t < -5e-6]))
                    ),
                }
            )
        results.append(
            {
                "geometry_limit": label,
                "liquid_radius_m": a,
                "wall_outer_radius_m": b,
                "wall_sigma_S_m": sigma,
                "steady_AC_relative_error": steady_error,
                "analytical_mode_checks": trans[-1],
                "cases": cases,
            }
        )
        plotted.append((label, curves))
        if args.check_refinement:
            refined = []
            for fine_dt, window in [(dt / 2, duration), (dt, duration * 2)]:
                fine_t = np.arange(round(window / fine_dt)) * fine_dt - onset
                fine_trans = transfer(fine_dt, window, a, b, sigma)
                refined.append(
                    {
                        name: history(amplitude * w, fine_trans, fine_dt, sigma)
                        for name, w in waveforms(fine_t).items()
                    }
                )
            checks = []
            for row, (name, base_pressure) in zip(cases, curves):
                fine = refined[0][name]
                window = refined[1][name]
                peak_error = float(fine[0].max() / base_pressure.max() - 1)
                heat_error = float(
                    (fine[2].sum() + fine[3].sum())
                    * (dt / 2)
                    / (row["liquid_heat_J_per_m"] + row["wall_heat_J_per_m"])
                    - 1
                )
                window_error = float(
                    np.max(abs(window[0][: len(t)] - base_pressure))
                    / np.max(abs(base_pressure))
                )
                if (
                    abs(peak_error) > 0.01
                    or abs(heat_error) > 0.01
                    or window_error > 1e-6
                ):
                    raise AssertionError("Analytical time/window refinement failed")
                checks.append(
                    {
                        "waveform": name,
                        "dt_halving_inward_peak_relative_change": peak_error,
                        "dt_halving_total_heat_relative_change": heat_error,
                        "window_doubling_pressure_max_relative_difference": window_error,
                    }
                )
            refinement.append({"geometry_limit": label, "checks": checks})
    record = {
        "status": "Analytical infinite-cylinder pulse/shielding comparison only; no nozzle, circuit or jetting result",
        "background": "https://web.mit.edu/6.013_book/www/chapter10/10.3.html",
        "derivation": "Fourier synthesize exact Bessel transfer functions with exp(+i omega t). Jphi=-dBz/dr/mu. Integrate -Jphi*Bz over radius; verify instantaneous result=(Bwall²-Baxis²)/(2mu). Steady sinusoidal mean also matches the existing phasor solution.",
        "parameters": {
            "dt_s": dt,
            "periodic_synthesis_window_s": duration,
            "pulse_onset_s": onset,
            "prescribed_external_peak_B_T": amplitude,
            "liquid_sigma_S_m": 2.2e6,
            "permeability_H_m": MU,
            "radial_Gauss_nodes": len(NODES),
        },
        "results": results,
        "time_window_refinement": refinement,
        "limits": [
            "Field at the outer cylinder is prescribed. Equal0.25T here does not mean equal coil current or voltage in different geometries.",
            "Infinite axial length, concentric geometry, constant conductivities, unit relative permeability. No finite coil, nozzle, free surface or axial force.",
            "Radial pressure difference is an analytical interior diagnostic, not solved outlet pressure or an ejection threshold.",
            "Joule heat is per metre of the uniform infinite cylinder; do not multiply by an arbitrary coil length to claim a finite nozzle heat result.",
            "Fourier synthesis is periodic; finite sampling can produce small pre-ringing. Inspect time/window refinement before interpreting sharp-edge maxima.",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(record, indent=2) + "\n")
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, axes = plt.subplots(2, 3, figsize=(14, 7.2), constrained_layout=True)
    show = (t >= -25e-6) & (t <= 500e-6)
    for column, (label, curves) in enumerate(plotted):
        for name, curve in curves:
            row = 1 if "burst" in name else 0
            ax = axes[row, column]
            ax.plot(t[show] * 1e6, curve[show], label=name, lw=1.1)
        for row in range(2):
            ax = axes[row, column]
            ax.axhline(0, color="grey", lw=0.7)
            ax.set(
                xlabel="Time from onset (µs)",
                ylabel="Radial pressure difference (Pa)",
                ylim=(-3000, 6500) if row == 0 else (-9000, 26000),
            )
            ax.grid(alpha=0.2)
        axes[0, column].set_title(label)
    axes[0, 0].legend(fontsize=8)
    axes[1, 0].legend(fontsize=8)
    fig.suptitle(
        "Infinite-cylinder analytical response at the same prescribed 0.25 T external peak\nPositive: inward squeeze; negative: outward pullback. No finite nozzle, driver or droplet prediction.",
        fontsize=12,
    )
    plot_path = args.plot_path or args.output.with_suffix(".png")
    plot_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(plot_path, dpi=170)
    print(args.output)
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
