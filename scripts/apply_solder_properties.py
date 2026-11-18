"""Create a distinct research project with source-traceable molten solder values.

The frozen application material catalogue is not changed. Geometry and all
non-liquid settings are retained. Thermal evolution remains a separate check.
"""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("project", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--alloy", choices=["Sn60Pb40", "Sn99.3Cu0.7"], required=True)
    parser.add_argument("--temperature", type=float, default=250.0)
    args = parser.parse_args()
    if not 250 <= args.temperature <= 900:
        raise ValueError("This tool restricts the combined fits to 250–900 C")
    data = json.loads(
        (
            Path(__file__).resolve().parents[1] / "hardware/solder-properties.json"
        ).read_text()
    )
    source = data[args.alloy]
    p = json.loads(args.project.read_text())
    tc, tk = args.temperature, args.temperature + 273.15
    viscosity = (
        source["viscosity_eta0_mPa_s"]
        * 0.001
        * math.exp(source["viscosity_activation_J_mol"] / (8.314462618 * tk))
    )
    if args.alloy == "Sn60Pb40":
        resistivity = (
            sum(
                c * tc**i
                for i, c in enumerate(source["resistivity_polynomial_uohm_cm_C"])
            )
            * 1e-8
        )
        sigma = 1 / resistivity
        gamma = (
            sum(c * tk**i for i, c in enumerate(source["surface_tension_mN_m_K"]))
            * 0.001
        )
        density = sum(c * tk**i for i, c in enumerate(source["density_g_cm3_K"])) * 1000
    else:
        sigma, gamma, density = (
            source[k]
            for k in [
                "conductivity_screen_S_m",
                "surface_tension_screen_N_m",
                "density_screen_kg_m3",
            ]
        )
    material = next(m for m in p["materials"] if m["id"] == 4)
    material.update(
        name=args.alloy + " molten research properties",
        conductivity_s_m=sigma,
        density_kg_m3=density,
        surface_tension_n_m=gamma,
        dynamic_viscosity_pa_s=viscosity,
        reference=data["primary_thesis"],
        notes=f"Applied at {tc} C from hardware/solder-properties.json; see its units, sources and limitations. No temperature dependence during this thermal-disabled trial.",
    )
    material["thermal"].update(
        solidus_c=source["solidus_c"], liquidus_c=source["liquidus_c"]
    )
    for body in p["bodies"]:
        if body["simulation"]["material_id"] == 4:
            body["simulation"]["temperature_c"] = tc
    p["geometry"] = os.path.relpath(
        (args.project.parent / p["geometry"]).resolve(), args.output.parent.resolve()
    )
    with args.output.open("x") as out:
        json.dump(p, out, indent=2)
    record = {
        "source_project": str(args.project),
        "source_sha256": hashlib.sha256(args.project.read_bytes()).hexdigest(),
        "alloy": args.alloy,
        "temperature_c": tc,
        "liquid_properties": material,
        "limitations": data["limitations"],
    }
    args.output.with_name(args.output.name + "-properties.json").write_text(
        json.dumps(record, indent=2) + "\n"
    )
    print(
        json.dumps(
            {
                "sigma_S_m": sigma,
                "rho_kg_m3": density,
                "gamma_N_m": gamma,
                "mu_Pa_s": viscosity,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
