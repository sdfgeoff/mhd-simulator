"""Calculate terminal compliance from completed native transient linkage records.

This is lumped circuit accounting, not another field or fluid solver.
"""

import csv
import argparse
import hashlib
import json
import math
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def reaction_sample(row):
    value = row.get("reaction_linkage_Wb")
    if value not in [None, ""]:
        return float(value)
    if (
        int(row["step"]) == 0
        and float(row["time_s"]) == 0
        and float(row["coil_current_A"]) == 0
        and float(row["reaction_magnetic_J"]) == 0
    ):
        # The native initially-zero state has no accepted-step linkage packet.
        return 0.0
    raise ValueError("Missing native reaction linkage after the initial zero state")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "--available",
        action="store_true",
        help="Explicitly analyze completed cases while the batch continues",
    )
    args = parser.parse_args()
    root = Path("artifacts/hardware-research")
    plan_path = args.plan or root / "unipolar-coil-v2-plan.json"
    plan = json.loads(plan_path.read_text())
    jobs = plan["jobs"]
    other_status = []
    if args.available:
        jobs = []
        for job in plan["jobs"]:
            status_path = Path(job["output"]) / "run-status.json"
            status = (
                json.loads(status_path.read_text())
                if status_path.exists()
                else {"status": "pending"}
            )
            if status["status"] == "complete":
                jobs.append(job)
            else:
                other_status.append({"output": job["output"], "native_status": status})
        if not jobs:
            raise SystemExit("No completed native cases; no comparison written")
    for job in jobs:
        if not (Path(job["output"]) / "run-status.json").exists():
            raise SystemExit("Pending native coil run; no comparison written")
    inductance_path = root / "unipolar-coil-v1-air-inductance.json"
    air = json.loads(inductance_path.read_text())
    air_project = json.loads(Path(air["project"]).read_text())
    air_wire = air_project["electromagnetics"]["wires"][0]
    air_points = air_project["cad_curves"][air_wire["cad_curve"]]["points_mm"]
    air_l = air["winding"][-1]["inductance_h"]
    if air["circle_check"]["relative_error"] > 0.005:
        raise ValueError("Independent air-inductance check failed")
    results = []
    for job in jobs:
        directory = Path(job["output"])
        status = json.loads((directory / "run-status.json").read_text())
        if (
            status["status"] != "complete"
            or status["accepted_steps"] != job.get("requested_steps", 1200)
            or abs(status["time_s"] - job.get("end_time_s", 600e-6)) > 1e-10
        ):
            raise ValueError("Native coil run incomplete")
        project_path = Path(job["project"])
        if digest(project_path) != job["project_sha256"]:
            raise ValueError("Prepared input changed")
        project = json.loads(project_path.read_text())
        if project["run"]["fluid_enabled"] or project["run"]["thermal_enabled"]:
            raise ValueError("Current scaling requires fixed stationary properties")
        wire = project["electromagnetics"]["wires"][0]
        points = project["cad_curves"][wire["cad_curve"]]["points_mm"]
        if points != air_points or wire["core_radius_mm"] != air_wire["core_radius_mm"]:
            raise ValueError(
                "Air-inductance source winding differs from the native case"
            )
        length = sum(math.dist(a, b) for a, b in zip(points, points[1:])) * 1e-3
        radius = wire["core_radius_mm"] * 1e-3
        # Explicit round-wire/hot-conductivity assumption, rather than silently
        # reusing an older estimate without checking geometry/temperature.
        winding_r = length / (25e6 * math.pi * radius**2)
        series_r, series_l = 0.002, 0.2e-6
        rows = list(csv.DictReader((directory / "diagnostics.csv").open()))
        if len(rows) != job.get("requested_steps", 1200) + 1:
            raise ValueError("Missing native diagnostics")
        i2dt = work = winding_heat = source_work_midpoint = 0.0
        volts = []
        for a, b in zip(rows, rows[1:]):
            t0, t1 = float(a["time_s"]), float(b["time_s"])
            i0, i1 = float(a["coil_current_A"]), float(b["coil_current_A"])
            old, new = reaction_sample(a), reaction_sample(b)
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
        if any(not math.isfinite(v) or v < 0 for v in [heat, be, final_field]):
            raise ValueError("Invalid stationary heat/field/numerical-loss energy")
        decay_fraction = final_field / max(heat, 1e-30)
        if decay_fraction > 1e-8:
            raise ValueError("Stationary screen retains appreciable reaction energy; extend decay")
        defect = (work - heat - be - final_field) / max(
            abs(work), abs(heat + be + final_field), 1e-15
        )
        if not math.isfinite(defect) or abs(defect) > 1e-3:
            raise ValueError("Native stationary metal-work balance failed")
        expected_dose = (
            job["peak_A"] ** 2 * (job["rise_us"] + job["fall_us"]) * 1e-6 / 3
        )
        if abs(i2dt / expected_dose - 1) > 1e-6:
            raise ValueError(
                "Accepted waveform dose differs from the triangular command"
            )
        radial_range = [
            min(float(r["radial_pressure_proxy_Pa"]) for r in rows),
            max(float(r["radial_pressure_proxy_Pa"]) for r in rows),
        ]
        axial_range = [
            min(float(r["roi_force_z_N"]) for r in rows),
            max(float(r["roi_force_z_N"]) for r in rows),
        ]

        def integrate_samples(key, lower, upper):
            # Piecewise-linear accepted native samples, clipped at phase edges.
            integral = 0.0
            for a, b in zip(rows, rows[1:]):
                t0, t1 = float(a["time_s"]), float(b["time_s"])
                lo, hi = max(t0, lower), min(t1, upper)
                if hi <= lo:
                    continue
                f0, f1 = float(a[key]), float(b[key])
                value = lambda t: f0 + (f1 - f0) * (t - t0) / (t1 - t0)
                integral += (hi - lo) * (value(lo) + value(hi)) / 2
            return integral

        rise_end = job["rise_us"] * 1e-6
        fall_end = (job["rise_us"] + job["fall_us"]) * 1e-6
        phases = [("rise", 0, rise_end), ("fall", rise_end, fall_end),
                  ("zero_current_decay", fall_end, float(rows[-1]["time_s"]))]
        impulses = {
            label: {
                "axial_force_impulse_N_s_positive_up": integrate_samples("roi_force_z_N", lo, hi),
                "radial_moment_integral_Pa_s": integrate_samples("radial_pressure_proxy_Pa", lo, hi),
            }
            for label, lo, hi in phases
        }
        total_impulse = integrate_samples("roi_force_z_N", 0, float(rows[-1]["time_s"]))
        if abs(sum(p["axial_force_impulse_N_s_positive_up"] for p in impulses.values()) - total_impulse) > 1e-14:
            raise ValueError("Impulse phase partition does not close")

        def projection(scale):
            return {
                "peak_A": job["peak_A"] * scale,
                "rms_A_at_assumed200Hz": math.sqrt(i2dt * 200) * scale,
                "loaded_terminal_interval_mean_voltage_range_V": [
                    min(volts) * scale,
                    max(volts) * scale,
                ],
                "winding_metal_and_series_resistive_heat_W_at_assumed200Hz": (
                    winding_heat + heat + series_r * i2dt
                )
                * 200
                * scale**2,
                "radial_force_moment_over_ROI_volume_Pa_range": [
                    v * scale**2 for v in radial_range
                ],
                "liquid_axial_force_N_range": [v * scale**2 for v in axial_range],
            }

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
                "final_reaction_energy_over_generated_heat": decay_fraction,
                "final_reaction_energy_fraction_gate_1e_minus8_passed": True,
                "radial_force_moment_over_ROI_volume_Pa_range": radial_range,
                "liquid_axial_force_N_range": axial_range,
                "native_force_integrals": {
                    "phases": impulses,
                    "total_axial_impulse_N_s_positive_up": total_impulse,
                    "scope": "Trapezoidal integration of stationary liquid ROI forces. Neither a wall-pressure solution nor the impulse delivered to an expelled drop. Rise/fall cancellation can change when the interface moves.",
                },
                "stationary_current_projections": {
                    "at_200A_working_target": projection(200 / job["peak_A"]),
                    "capped_at_tested_peak_and30Arms200Hz": projection(
                        min(1.0, 30 / math.sqrt(i2dt * 200))
                    ),
                    "scope": "Same waveform, geometry and fixed conductivity/permeability: field/linkage/voltage scale with I; force/Joule heat with I². Reuses the native stationary solution, not another field solver. No thermal, moving-interface or droplet scaling is inferred.",
                },
                "scope": "Static liquid force moment is not nozzle pressure or a droplet criterion. Rate200Hz is arithmetic only, not demonstrated repetition.",
            }
        )
    record = {
        "status": (
            "Partial native stationary unipolar coil results; no jetting claim"
            if len(jobs) < len(plan["jobs"])
            else "Completed native stationary unipolar coil comparisons; no jetting claim"
        ),
        "expected_cases": len(plan["jobs"]),
        "completed_cases": len(jobs),
        "other_case_status": other_status,
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
    output = args.output or root / (
        plan_path.stem.removesuffix("-plan")
        + ("-comparison-partial.json" if args.available else "-comparison.json")
    )
    output.write_text(json.dumps(record, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
