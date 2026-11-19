"""Calculate terminal compliance from completed native transient linkage records.

This is lumped circuit accounting, not another field or fluid solver.
"""

import csv
import hashlib
import json
import math
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    root = Path("artifacts/hardware-research")
    plan_path = root / "unipolar-coil-v1-plan.json"
    plan = json.loads(plan_path.read_text())
    for job in plan["jobs"]:
        if not (Path(job["output"]) / "run-status.json").exists():
            raise SystemExit("Pending native coil run; no comparison written")
    inductance_path = root / "unipolar-coil-v1-air-inductance.json"
    air = json.loads(inductance_path.read_text())
    air_l = air["winding"][-1]["inductance_h"]
    if air["circle_check"]["relative_error"] > 0.005:
        raise ValueError("Independent air-inductance check failed")
    results = []
    for job in plan["jobs"]:
        directory = Path(job["output"])
        status = json.loads((directory / "run-status.json").read_text())
        if (
            status["status"] != "complete"
            or status["accepted_steps"] != 1200
            or abs(status["time_s"] - 600e-6) > 1e-10
        ):
            raise ValueError("Native coil run incomplete")
        project_path = Path(job["project"])
        if digest(project_path) != job["project_sha256"]:
            raise ValueError("Prepared input changed")
        project = json.loads(project_path.read_text())
        wire = project["electromagnetics"]["wires"][0]
        points = project["cad_curves"][wire["cad_curve"]]["points_mm"]
        length = sum(math.dist(a, b) for a, b in zip(points, points[1:])) * 1e-3
        radius = wire["core_radius_mm"] * 1e-3
        # Explicit round-wire/hot-conductivity assumption, rather than silently
        # reusing an older estimate without checking geometry/temperature.
        winding_r = length / (25e6 * math.pi * radius**2)
        series_r, series_l = 0.002, 0.2e-6
        rows = list(csv.DictReader((directory / "diagnostics.csv").open()))
        if len(rows) != 1201:
            raise ValueError("Missing native diagnostics")
        i2dt = work = winding_heat = source_work_midpoint = 0.0
        volts = []
        for a, b in zip(rows, rows[1:]):
            t0, t1 = float(a["time_s"]), float(b["time_s"])
            i0, i1 = float(a["coil_current_A"]), float(b["coil_current_A"])
            old, new = float(a["reaction_linkage_Wb"]), float(b["reaction_linkage_Wb"])
            if (
                not all(math.isfinite(x) for x in [t0, t1, i0, i1, old, new])
                or t1 <= t0
                or min(i0, i1) < -1e-6
            ):
                raise ValueError("Invalid unipolar native samples")
            dt, delta = t1 - t0, i1 - i0
            dose = dt * (i0 * i0 + i0 * i1 + i1 * i1) / 3
            voltage = (
                (winding_r + series_r) * (i0 + i1) / 2
                + (air_l + series_l) * delta / dt
                + (new - old) / dt
            )
            i2dt += dose
            winding_heat += winding_r * dose
            source_work_midpoint += voltage * (i0 + i1) / 2 * dt
            work -= new * delta
            volts.append(voltage)
        final = rows[-1]
        heat = float(final["accumulated_joule_J"])
        be = sum(float(row["reaction_BE_J"]) for row in rows)
        final_field = float(final["reaction_magnetic_J"])
        defect = (work - heat - be - final_field) / max(
            abs(work), abs(heat + be + final_field), 1e-15
        )
        expected_dose = (
            job["peak_A"] ** 2 * (job["rise_us"] + job["fall_us"]) * 1e-6 / 3
        )
        if abs(i2dt / expected_dose - 1) > 1e-6:
            raise ValueError(
                "Accepted waveform dose differs from the triangular command"
            )
        results.append(
            {
                "geometry": job["geometry"],
                "shape": job["shape"],
                "source": str(directory),
                "diagnostics_sha256": digest(directory / "diagnostics.csv"),
                "I2dt_A2s": i2dt,
                "rms_A_at_assumed200Hz": math.sqrt(i2dt * 200),
                "round_wire_assumption": {
                    "radius_m": radius,
                    "length_m_including_CAD_return": length,
                    "conductivity_S_m": 25e6,
                    "resistance_ohm": winding_r,
                },
                "series_assumption": {
                    "resistance_ohm": series_r,
                    "inductance_H": series_l,
                },
                "loaded_terminal_interval_mean_voltage_range_V": [
                    min(volts),
                    max(volts),
                ],
                "winding_heat_J": winding_heat,
                "metal_heat_J": heat,
                "winding_plus_metal_heat_W_at_assumed200Hz": (winding_heat + heat)
                * 200,
                "midpoint_terminal_net_work_J": source_work_midpoint,
                "backward_Euler_metal_source_work_J": work,
                "metal_work_balance_relative_defect": defect,
                "metal_work_balance_gate_1e_minus3_passed": abs(defect) <= 1e-3,
                "reaction_backward_Euler_loss_J": be,
                "final_reaction_field_energy_J": final_field,
                "radial_force_moment_over_ROI_volume_Pa_range": [
                    min(float(r["radial_pressure_proxy_Pa"]) for r in rows),
                    max(float(r["radial_pressure_proxy_Pa"]) for r in rows),
                ],
                "liquid_axial_force_N_range": [
                    min(float(r["roi_force_z_N"]) for r in rows),
                    max(float(r["roi_force_z_N"]) for r in rows),
                ],
                "scope": "Static liquid force moment is not nozzle pressure or a droplet criterion. Rate200Hz is arithmetic only, not demonstrated repetition.",
            }
        )
    record = {
        "status": "Completed native stationary unipolar coil comparisons; no jetting claim",
        "plan_sha256": digest(plan_path),
        "air_inductance": air,
        "method": "Interval V=R*meanI+(L_air+L_series)*deltaI/dt+delta(native reaction linkage)/dt. Exact linear-interval I2dt. Separate backward-Euler metal source-work identity from midpoint lumped-circuit net work.",
        "cases": results,
        "limits": [
            "Prescribed current; no switching, current-loop control, PWM ripple, capacitor or loaded circuit simulation.",
            "AirL retains softened round-wire assumptions. Core radius used for circuit area is an explicit wire assumption; actual winding cross-section, hot resistance and lead routing need specification.",
            "Interval voltages exclude switch-node overshoot. Negative voltage at positive current requires controlled recovery or a suitable clamp.",
            "Metal reaction uses stationary initial conductivity, no thermal/motional evolution. No equality of fields/pressure in the moving meniscus is established.",
            "200um cold screen is not spatial convergence. Inspect work-balance and numerical-loss sensitivity before selecting a flow waveform.",
        ],
    }
    output = root / "unipolar-coil-v1-comparison.json"
    output.write_text(json.dumps(record, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
