"""Fit a lumped series-C drive to computed cold coil reaction linkage.

No field solution is implemented here. Coil air L is an independent input;
reaction flux and current come from accepted GPU hardware_probe samples.
"""

import argparse
import csv
import json
from pathlib import Path
import numpy as np


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("run", type=Path)
    for name in [
        "air-l-uh",
        "coil-r-ohm",
        "frequency-hz",
        "fit-start-us",
        "fit-end-us",
    ]:
        p.add_argument("--" + name, type=float, required=True)
    p.add_argument("--series-r-ohm", type=float, default=0.0)
    p.add_argument("--series-l-uh", type=float, default=0.0)
    p.add_argument(
        "--through-us",
        type=float,
        help="Explicitly limit voltage evaluation to an observed prefix",
    )
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    scalars = [v for v in vars(a).values() if isinstance(v, float)]
    if (
        not all(np.isfinite(scalars))
        or min(a.air_l_uh, a.frequency_hz) <= 0
        or min(a.coil_r_ohm, a.series_r_ohm, a.series_l_uh, a.fit_start_us) < 0
        or a.fit_end_us <= a.fit_start_us
        or (a.through_us is not None and a.through_us <= 0)
    ):
        raise ValueError("Invalid circuit parameters or fit window")
    rows = list(csv.DictReader((a.run / "diagnostics.csv").open()))
    input_through = float(rows[-1]["time_s"])
    # Discrete reciprocity is separate from the interval-mean circuit fit. The
    # backward-Euler work uses new linkage, not a midpoint linkage. Zero-current
    # tails require no extra source work, even in older records without linkage.
    work = 0.0
    old_current = 0.0
    for row in rows:
        new_current = float(row["coil_current_A"])
        change = new_current - old_current
        if change != 0:
            work -= float(row["reaction_linkage_Wb"]) * change
        old_current = new_current
    heat = float(rows[-1]["accumulated_joule_J"])
    numerical = sum(float(row["reaction_BE_J"]) for row in rows)
    reaction_energy = float(rows[-1]["reaction_magnetic_J"])
    accounted = heat + numerical + reaction_energy
    energy_check = {
        "through_us": input_through * 1e6,
        "backward_euler_reciprocal_source_work_j": work,
        "metal_joule_heat_j": heat,
        "reaction_numerical_loss_j": numerical,
        "final_reaction_energy_j": reaction_energy,
        "relative_work_defect": (work - accounted) / max(abs(work), abs(accounted), 1e-30),
        "numerical_loss_over_joule_heat": numerical / max(heat, 1e-30),
        "meaning": "Accepted backward-Euler field energy check, not physical inverter input energy. Its numerical loss must decrease under time refinement.",
    }
    if a.through_us is not None:
        rows = [v for v in rows if float(v["time_s"]) <= a.through_us * 1e-6 + 1e-12]
    t = np.array([float(v["time_s"]) for v in rows])
    current = np.array([float(v["coil_current_A"]) for v in rows])
    reaction = np.array(
        [
            (
                0.0
                if i == 0
                and t[i] == 0
                and current[i] == 0
                and not v["reaction_linkage_Wb"]
                else float(v["reaction_linkage_Wb"])
            )
            for i, v in enumerate(rows)
        ]
    )
    if not all(np.all(np.isfinite(v)) for v in [t, current, reaction]):
        raise ValueError("Nonfinite accepted winding observations")
    if t[-1] < a.fit_end_us * 1e-6 or np.any(np.diff(t) <= 0):
        raise ValueError("Requested complete fit window is unavailable")
    dt = np.diff(t)
    midpoint = (t[:-1] + t[1:]) / 2
    i_mid = (current[:-1] + current[1:]) / 2
    di = np.diff(current) / dt
    inductance = (a.air_l_uh + a.series_l_uh) * 1e-6
    r_coil = a.coil_r_ohm + a.series_r_ohm
    terminal = r_coil * i_mid + inductance * di + np.diff(reaction) / dt
    selected = (midpoint >= a.fit_start_us * 1e-6) & (midpoint < a.fit_end_us * 1e-6)
    matrix = np.stack([i_mid[selected], di[selected]], axis=1)
    resistance, loaded_l = np.linalg.lstsq(matrix, terminal[selected], rcond=None)[0]
    if loaded_l <= 0 or resistance <= 0:
        raise ValueError(
            "Non-passive fit; check air inductance, numerical resolution and fit window"
        )
    omega = 2 * np.pi * a.frequency_hz
    capacitance = 1 / (omega * omega * loaded_l)
    # Capacitor begins uncharged. The actual ramped waveform determines Vc.
    charge = np.concatenate([[0.0], np.cumsum(i_mid * dt)])
    # Exact interval-mean capacitor voltage for linearly interpolated current.
    capacitor_mid_v = (
        charge[:-1] + dt * (2 * current[:-1] + current[1:]) / 6
    ) / capacitance
    drive = terminal + capacitor_mid_v
    residual = terminal[selected] - matrix @ [resistance, loaded_l]
    report = {
        "method": "Interval V=R*I_mid + L_air*dI/dt + d(lambda_reaction)/dt. Fit V=R_eff*I_mid+L_eff*dI/dt over the stated steady carrier window. Series C cancels fitted L at the stated frequency; capacitor voltage integrates the actual current from zero initial charge.",
        "inputs": {
            k: str(v) if isinstance(v, Path) else v
            for k, v in vars(a).items()
            if k != "output"
        },
        "input_observed_through_us": input_through * 1e6,
        "voltage_evaluated_through_us": float(t[-1] * 1e6),
        "fitted_total_resistance_ohm": float(resistance),
        "fitted_loaded_inductance_uh": float(loaded_l * 1e6),
        "series_capacitance_uf": float(capacitance * 1e6),
        "fit_residual_rms_v": float(np.sqrt(np.mean(residual**2))),
        "observed_coil_and_series_voltage_peak_v": float(np.max(np.abs(terminal))),
        "observed_capacitor_voltage_peak_v": float(np.max(np.abs(capacitor_mid_v))),
        "observed_drive_voltage_peak_v": float(np.max(np.abs(drive))),
        "fit_average_input_w": float(np.mean(terminal[selected] * i_mid[selected])),
        "discrete_field_energy_check": energy_check,
        "limits": [
            "Cold prescribed-current screen, not a circuit solution or switching implementation. Reported maxima are interval-mean voltages, not switch-node peaks.",
            "Check waveform completion, time/grid refinement and energy/numerical losses before using component ratings.",
            "Air L retains round-wire low-frequency/internal-inductance assumptions; AC resistance, proximity, real leads, capacitor ESR/ripple and control dynamics are unresolved.",
            "No droplet or repetition-rate claim follows from electrical compliance.",
        ],
    }
    with a.output.open("x") as f:
        json.dump(report, f, indent=2)
        f.write("\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
