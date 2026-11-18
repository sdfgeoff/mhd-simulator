"""Mass-conserving solid-wire feed and constant-property melting heat scale.

No feed mechanism or thermal PDE is solved. Liquid and solid densities differ.
Heat capacity and latent heat remain explicit screening assumptions.
"""

import argparse
import json
import math
from pathlib import Path


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--wire-mm", type=float, default=.8)
    p.add_argument("--liquid-density", type=float, default=7000.)
    p.add_argument("--solid-density", type=float, default=7300.)
    p.add_argument("--heat-capacity", type=float, default=214.)
    p.add_argument("--latent-heat", type=float, default=59000.)
    p.add_argument("--feed-c", type=float, default=25.)
    p.add_argument("--pool-c", type=float, default=250.)
    p.add_argument("--melting-c", type=float, default=227.)
    a = p.parse_args()
    for k, v in vars(a).items():
        if isinstance(v, float) and not math.isfinite(v):
            p.error(f"{k} must be finite")
    if min(a.wire_mm, a.liquid_density, a.solid_density, a.heat_capacity, a.latent_heat) <= 0 or not -273.15 < a.feed_c < a.melting_c <= a.pool_c:
        p.error("Choose positive dimensions/properties, solid feed below melting, and a molten pool")
    wire_area = math.pi/4*(a.wire_mm*.001)**2
    rows = []
    for diameter in [.4, .5, .6]:
        volume = math.pi/6*(diameter*.001)**3
        for frequency in [200, 400]:
            flow = volume*frequency
            mass = flow*a.liquid_density
            rows.append({
                "drop_diameter_mm": diameter, "frequency_hz": frequency,
                "drop_volume_nl": volume*1e12,
                "liquid_flow_mm3_s": flow*1e9,
                "mass_flow_g_min": mass*60000,
                "solid_wire_speed_mm_s": mass/(a.solid_density*wire_area)*1000,
                "feed_sensible_heat_w": mass*a.heat_capacity*(a.pool_c-a.feed_c),
                "feed_latent_heat_w": mass*a.latent_heat,
                "total_feed_enthalpy_w": mass*(a.heat_capacity*(a.pool_c-a.feed_c)+a.latent_heat),
            })
    result = {
        "status": "Steady throughput and feed-enthalpy scale, not demonstrated jetting or a qualified heater",
        "inputs": {k: v for k, v in vars(a).items() if k != "output"},
        "method": "Spherical liquid volume=pi*D^3/6; mass conservation with separate liquid/solid densities; constant cp*(Tpool-Tfeed)+latent heat per unit mass.",
        "solid_density_reference": "https://www.feinhuette.de/en/metals/sn993cu07",
        "limits": [
            "Sn99.3Cu0.7 only. Default solid density7300kg/m3 is a supplier nominal value, liquid density7000kg/m3 is the existing hot screening value.",
            "Default cp214J/kg/K and latent59000J/kg are inherited screening inputs, not measured temperature-dependent alloy enthalpy. The constant cp is used for the whole feed temperature interval.",
            "Solid wire is assumed, without a flux core, voids or entrained gas; no feed-force/traction, melting-length, replenishment-control or pool-level simulation.",
            "Feed heat is one thermal-balance term. Actual heater demand also includes losses and any retained electrical dissipation; this report neither adds all pulse heat to demand nor assumes all pulse heat usefully melts feed.",
        ],
        "cases": rows,
    }
    with a.output.open("x") as f:
        json.dump(result, f, indent=2)
        f.write("\n")
    print(json.dumps(rows, indent=2))


if __name__ == "__main__":
    main()
