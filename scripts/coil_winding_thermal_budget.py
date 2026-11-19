"""Lumped winding heat/cooling requirements; no thermal field solver or rating."""
import hashlib
import json
import math
from pathlib import Path


source = Path("hardware/designs/single-coil-250A/case.mhd.json")
project = json.loads(source.read_text())
wire = project["electromagnetics"]["wires"][0]
points = project["cad_curves"][wire["cad_curve"]]["points_mm"]
assert len(points) == 246  # Two121-point helices followed by four return points.
winding = points[:-4]
radius = wire["core_radius_mm"] * .001
area = math.pi * radius**2
length = sum(math.dist(a, b) for a, b in zip(points, points[1:])) * .001
ri = min(math.hypot(*p[:2]) for p in winding) * .001 - radius
ro = max(math.hypot(*p[:2]) for p in winding) * .001 + radius
height = (max(p[2] for p in winding) - min(p[2] for p in winding)) * .001
packed_area = 2 * math.pi * ro * height + 2 * math.pi * (ro**2 - ri**2)
bare_area = 2 * math.pi * radius * length
rho20, alpha20 = 1 / 58e6, .00393
r20 = rho20 * length / area
mass = length * area * 8890
heat_capacity = mass * 394
assumed_r = length / (25e6 * area)
native = json.loads(Path("hardware/unipolar-coil-v2-first-result.json").read_text())["cases"][0]
assert abs(assumed_r / native["round_wire_assumption"]["resistance_ohm"] - 1) < 1e-12
waveforms = [("100/100us at250A", 250, 200),
             ("50/250us at212.132A", math.sqrt(30**2 / (300e-6 / 3 * 200)), 300),
             ("10/100us at250A (not yet native-complete)", 250, 110)]
cases = []
for label, peak, duration_us in waveforms:
    i2dt = peak**2 * duration_us * 1e-6 / 3
    rms2 = i2dt * 200
    rows = []
    for temp in [100, 150, 180, 200, 220, 240, 250]:
        resistance = r20 * (1 + alpha20 * (temp - 20))
        heat = rms2 * resistance
        for head_g in [0, .005, .01]:
            head_heat = head_g * (250 - temp)
            required = max(0., (heat + head_heat) / (temp - 25))
            rows.append({"coil_temperature_C": temp, "linearized_resistance_ohm": resistance,
                         "winding_heat_W_at_assumed200Hz": heat,
                         "assumed_head_to_coil_conductance_W_K": head_g,
                         "head_heat_into_coil_W": head_heat,
                         "required_total_coil_to_ambient_conductance_W_K": required,
                         "effective_h_W_m2_K_if_packed_envelope_area": required / packed_area,
                         "effective_h_W_m2_K_if_all_bare_wire_area": required / bare_area})
    cases.append({"waveform": label, "I2dt_A2s": i2dt, "rms_A_at_assumed200Hz": math.sqrt(rms2),
                  "at_existing_25MS_m_assumption": {"winding_heat_W_at_assumed200Hz": rms2 * assumed_r,
                      "one_pulse_adiabatic_uniform_rise_K": i2dt * assumed_r / heat_capacity},
                  "cooling_requirements": rows})
record = {
    "status": "Lumped winding/cooling sensitivity only; no measured temperature, cooling capacity or thermal qualification",
    "source_project": str(source), "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
    "geometry": {"round_wire_radius_m": radius, "wire_length_including_return_m": length,
                 "coil_inner_radius_m": ri, "coil_outer_radius_m": ro, "coil_height_m": height,
                 "packed_annular_envelope_area_m2": packed_area, "all_bare_wire_area_m2": bare_area,
                 "copper_mass_kg": mass, "uniform_heat_capacity_J_K": heat_capacity},
    "resistance": {"IACS_20C_resistivity_ohm_m": rho20, "near20C_temperature_coefficient_K_inv": alpha20,
                   "R20_ohm": r20, "existing_25MS_m_R_ohm": assumed_r,
                   "linearized_temperature_corresponding_to25MS_m_C": 20 + (58e6 / 25e6 - 1) / alpha20},
    "method": "R(T)=R20[1+alpha20(T−20)]. Required net cooling conductance Gout=[Irms²R(T)+Ghead(250−T)]/(T−25). Gout includes all paths to ambient; no convection coefficient or cooling capability is assumed. Envelope/bare-wire areas bracket geometric exposure estimates, not rigorous bounds on heat transfer.",
    "cases": cases,
    "sources": [
        {"url": "https://help.copper.fyi/hc/en-us/articles/4410229175442-How-does-copper-set-the-standard-for-electrical-conductivity", "use": "IACS resistivity1/58ohm mm2/m, density8890kg/m3 and near20C coefficient. Linear extrapolation to hot conditions is a sensitivity, not a calibrated alloy curve."},
        {"url": "https://nvlpubs.nist.gov/nistpubs/Legacy/hb/nbshandbook100.pdf", "use": "Copper wire tables reference for20C resistivity and temperature coefficient; table-specific validity not transferred to the full hot range."},
        {"url": "https://mwswire.com/round-magnet-wire/", "use": "Manufacturer identifies polyimide magnet wire as thermal class240C. Class does not establish this winding's operating temperature or assembly lifetime."},
    ],
    "limits": [
        "The earlier25MS/m assumption was explicit electrical conservatism, not a solved winding temperature. Linearized IACS correspondence is about356C; do not label it as copper at100C.",
        "No prediction that a winding touching or near a250C head satisfies a200/240C insulation class. Bobbin, enamel build, support and measured winding temperature remain unspecified.",
        "AssumedGhead0..0.01W/K is sensitivity only; no gap conduction/radiation/contact calculation or validated bound.",
        "Uniform temperature, fixed dimensions and averagedI²R omit localized hot spots, skin/proximity loss, PWM ripple, terminals and winding stress. Heat capacity394J/kgK is an inherited rounded copper assumption.",
        "All-wire exposed area includes return lead and hidden contacts; packed-envelope area omits some real exposed surfaces. Neither assigns a convection/radiation coefficient.",
        "200Hz is arithmetic only; no stable repeated droplet or feed replenishment result. No passive/forced cooling method is qualified.",
    ],
}
output = Path("hardware/coil-winding-thermal-budget.json")
output.write_text(json.dumps(record, indent=2) + "\n")
print(output)
for c in cases:
    row = next(r for r in c["cooling_requirements"] if r["coil_temperature_C"] == 200 and r["assumed_head_to_coil_conductance_W_K"] == .005)
    print(c["waveform"], "at200C:", row)
