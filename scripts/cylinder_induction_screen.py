"""Closed-form axial AC diffusion in a liquid core and conducting shell.

An infinite coaxial-cylinder limit, not a nozzle/free-surface simulation or a
CPU implementation of the GPU PDE. Peak phasors use exp(+i omega t), mu=mu0.
The prescribed external B ignores coil/load backreaction and finite-length ends.
Equation: (r B')'/r = i omega mu sigma B. Match B and E_phi=-B'/(mu sigma).
Primary background: https://web.mit.edu/6.013_book/www/chapter10/10.3.html
"""

import argparse
import json
from pathlib import Path

import numpy as np
from scipy.special import iv, kv

MU = 4e-7*np.pi
NODES, WEIGHTS = np.polynomial.legendre.leggauss(128)


def response(frequency, a, b, liquid_sigma, wall_sigma, external_b):
    omega = 2*np.pi*frequency
    kl = np.sqrt(1j*omega*MU*liquid_sigma)
    if wall_sigma == 0 or b == a:
        c = external_b/iv(0, kl*a)
        wall = None
    else:
        ks = np.sqrt(1j*omega*MU*wall_sigma)
        # Divide each interface equation by its largest coefficient for units
        # and conditioning; the unknowns are liquid I0 and wall I0/K0 amplitudes.
        matrix = np.array([
            [iv(0, kl*a), -iv(0, ks*a), -kv(0, ks*a)],
            [kl*iv(1, kl*a)/liquid_sigma, -ks*iv(1, ks*a)/wall_sigma, ks*kv(1, ks*a)/wall_sigma],
            [0, iv(0, ks*b), kv(0, ks*b)],
        ], dtype=complex)
        rhs = np.array([0, 0, external_b], dtype=complex)
        scale = np.max(abs(matrix), axis=1)
        c, d, e = np.linalg.solve(matrix/scale[:, None], rhs/scale)
        wall = (ks, d, e)

    def core(r):
        return c*iv(0, kl*r), c*kl*iv(1, kl*r)

    def shell(r):
        ks, d, e = wall
        return d*iv(0, ks*r)+e*kv(0, ks*r), ks*(d*iv(1, ks*r)-e*kv(1, ks*r))

    def integrals(lo, hi, sigma, fields):
        r = (lo+hi)/2 + NODES*(hi-lo)/2
        weights = WEIGHTS*(hi-lo)/2
        bz, derivative = fields(r)
        j = -derivative/MU
        power = float(np.sum(weights*.5*abs(j)**2/sigma*2*np.pi*r))
        inward_force_integral = float(-np.sum(weights*.5*np.real(j*np.conj(bz))))
        return power, inward_force_integral

    liquid_power, pressure_integral = integrals(0, a, liquid_sigma, core)
    ba, _ = core(a)
    pressure_difference = float((abs(ba)**2-abs(c)**2)/(4*MU))
    wall_power = 0.
    if wall is not None:
        wall_power, _ = integrals(a, b, wall_sigma, shell)
        bout, derivative = shell(b)
        inward_poynting = float(np.pi*b/(MU*MU*wall_sigma)*np.real(derivative*np.conj(bout)))
        interface_b, interface_derivative = shell(a)
        liquid_derivative = core(a)[1]
        interface_error = max(abs(interface_b-ba)/external_b, abs(interface_derivative/wall_sigma-liquid_derivative/liquid_sigma)/max(abs(liquid_derivative/liquid_sigma), 1e-30))
    else:
        bout, derivative = core(a)
        inward_poynting = float(np.pi*a/(MU*MU*liquid_sigma)*np.real(derivative*np.conj(bout)))
        interface_error = 0.
    total_power = liquid_power+wall_power
    return {
        "frequency_hz": float(frequency),
        "liquid_surface_B_peak_T": float(abs(ba)),
        "liquid_axis_B_peak_T": float(abs(c)),
        "radial_axis_to_wall_pressure_scale_Pa": pressure_difference,
        "radial_force_integral_Pa": pressure_integral,
        "liquid_heat_W_per_m": liquid_power,
        "wall_heat_W_per_m": wall_power,
        "relative_poynting_heat_defect": total_power/inward_poynting-1,
        "interface_relative_error": float(interface_error),
    }


def checks():
    worst_power, worst_interface, worst_force = 0., 0., 0.
    for frequency in [1e3, 5e4, 1e6, 1e7]:
        r = response(frequency, .0002, .0007, 2.2e6, 1.3e6, .5)
        worst_power = max(worst_power, abs(r["relative_poynting_heat_defect"]))
        worst_interface = max(worst_interface, r["interface_relative_error"])
        worst_force = max(worst_force, abs(r["radial_axis_to_wall_pressure_scale_Pa"]/r["radial_force_integral_Pa"]-1))
        assert worst_power < 1e-9 and worst_interface < 1e-9 and worst_force < 1e-6
        same = response(frequency, .0002, .0007, 2.2e6, 2.2e6, .5)
        k = np.sqrt(1j*2*np.pi*frequency*MU*2.2e6)
        expected_axis = abs(.5/iv(0, k*.0007))
        assert abs(same["liquid_axis_B_peak_T"]/expected_axis-1) < 1e-10
    low = response(1., .0002, .0002, 2.2e6, 0., .5)
    expected_heat = np.pi*2.2e6*(2*np.pi)**2*.5**2*.0002**4/16
    assert abs(low["liquid_heat_W_per_m"]/expected_heat-1) < 1e-9
    print(json.dumps({"worst_power_defect": worst_power, "worst_interface_error": worst_interface, "worst_force_identity_error": worst_force}))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--check", action="store_true")
    p.add_argument("--output", type=Path)
    p.add_argument("--plot", action="store_true")
    a = p.parse_args()
    if a.check:
        checks()
    if a.output is None:
        if not a.check:
            p.error("Choose --check and/or --output")
        return
    frequencies = np.geomspace(1e3, 1e7, 241)
    cases = []
    for name, b, sigma in [
        ("0.5 mm stainless wall", .0007, 1.3e6),
        ("0.5 mm lower-conductivity wall", .0007, .5e6),
        ("0.05 mm stainless wall", .00025, 1.3e6),
        ("Ideal insulating wall", .0002, 0.),
    ]:
        rows = [response(f, .0002, b, 2.2e6, sigma, .5) for f in frequencies]
        best = max(rows, key=lambda row: row["radial_axis_to_wall_pressure_scale_Pa"])
        cases.append({"name": name, "wall_sigma_S_m": sigma, "outer_radius_m": b, "peak_pressure_scale_sample": best, "selected_frequencies": [response(f, .0002, b, 2.2e6, sigma, .5) for f in [5e4, 1e6, 5e6, 1e7]], "rows": rows})
    result = {
        "status": "Analytical infinite-cylinder screening limit only; no nozzle, droplet or feasible driver prediction",
        "assumptions": {"liquid_radius_m": .0002, "liquid_sigma_S_m": 2.2e6, "mu_H_m": MU, "prescribed_external_B_peak_T": .5, "phasor": "peak, exp(+i omega t)", "frequency_range_hz": [1e3, 1e7]},
        "source": "https://web.mit.edu/6.013_book/www/chapter10/10.3.html",
        "equations": "Bcore=C I0(kl r), Bwall=D I0(ks r)+E K0(ks r), k=sqrt(i omega mu sigma); match B and Ephi=-Bprime/(mu sigma); integrate half Re[J cross Bconj]. Radial pressure scale=(|B(a)|^2-|B(0)|^2)/(4mu).",
        "limits": [
            "Infinite length, axisymmetry, uniform external axial field and unit relative permeability; finite coil/end/nozzle forces are absent.",
            "Radial axis-to-wall pressure difference is a diagnostic, not solved outlet pressure or a jetting threshold.",
            "External0.5T is prescribed at every frequency without a realizable coil, current, resonance, driver or cooling check.",
            "Constant conductivities are exploratory hot screening values. The0.5MS/m wall is generic, not a verified supplier titanium alloy.",
            "Heat is steady-cycle W per metre of an infinitely uniform axial region; do not multiply by an arbitrary coil length to claim a finite nozzle heat result.",
            "The reported maximum is over the sampled1kHz..10MHz range. Thin-wall/insulating curves are still increasing at the upper limit; this is not a global frequency optimum.",
        ],
        "cases": cases,
    }
    with a.output.open("x") as f:
        json.dump(result, f, indent=2)
        f.write("\n")
    print(json.dumps([{k: v for k, v in c.items() if k != "rows"} for c in cases], indent=2))
    if a.plot:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
        fig, axes = plt.subplots(1, 2, figsize=(12, 5), layout="constrained")
        for case in cases:
            rows = case["rows"]
            frequency = [r["frequency_hz"] for r in rows]
            axes[0].loglog(frequency, [r["radial_axis_to_wall_pressure_scale_Pa"] for r in rows], label=case["name"])
            fraction = [r["liquid_heat_W_per_m"]/(r["liquid_heat_W_per_m"]+r["wall_heat_W_per_m"]) for r in rows]
            axes[1].semilogx(frequency, fraction, label=case["name"])
        axes[0].axhline(4*.53/.0004, color=".4", ls="--", lw=1, label="4γ/d capillary scale (comparison only)")
        axes[0].set(ylabel="Cycle-mean radial pressure scale (Pa)", title="Axis-to-wall pressure difference")
        axes[1].set(ylabel="Liquid / (liquid + wall) Joule heat", ylim=(-.02, 1.02), title="Local heat distribution; coil omitted")
        for ax in axes:
            ax.axvline(5e4, color=".7", ls=":", lw=1)
            ax.set(xlabel="Frequency (Hz)", xlim=(1e3, 1e7))
            ax.grid(alpha=.2)
        axes[0].legend(fontsize=8, loc="upper left")
        fig.suptitle("Analytical induction screen: 0.4 mm liquid core, prescribed 0.5 T peak external field\nInfinite cylinder and radial wall thickness; no finite coil/nozzle/interface or jet prediction", fontsize=12)
        fig.savefig(a.output.with_suffix(".png"), dpi=170, bbox_inches="tight")


if __name__ == "__main__":
    main()
