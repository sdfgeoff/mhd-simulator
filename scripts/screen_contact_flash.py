"""Analytical short-pulse joint heating sensitivity; not a contact safety limit."""
import json
import math
from pathlib import Path

project = Path('hardware/designs/v6-flat-shortbar/case.mhd.json')
materials = {m['id']: m for m in json.loads(project.read_text())['materials']}
copper, steel = materials[16], materials[6]

def properties(material):
    thermal = material['thermal']
    density, cp, k = material['density_kg_m3'], thermal['heat_capacity_j_kg_k'], thermal['conductivity_w_m_k']
    return {'name': material['name'], 'density_kg_m3': density, 'cp_J_kg_K': cp,
            'k_W_m_K': k, 'effusivity_J_m2_K_sqrt_s': math.sqrt(density * cp * k),
            'diffusivity_m2_s': k / (density * cp)}

cu, ss = properties(copper), properties(steel)
effusivity = cu['effusivity_J_m2_K_sqrt_s'] + ss['effusivity_J_m2_K_sqrt_s']
duration = 250e-6
nominal_area = .8e-3 * .4e-3
rows = []
for current in [175, 300]:
    for resistance in [50e-6, 100e-6, 500e-6, 1e-3]:
        for area_fraction in [1., .25, .1]:
            area = nominal_area * area_fraction
            flux = current**2 * resistance / area
            rise = 2 * flux * math.sqrt(duration / math.pi) / effusivity
            rows.append({'current_A': current, 'single_joint_resistance_ohm': resistance,
                         'active_area_fraction': area_fraction, 'active_area_mm2': area * 1e6,
                         'single_joint_energy_J': current**2 * resistance * duration,
                         'additional_interface_temperature_rise_K': rise})
result = {
    'method': 'Two semi-infinite bodies, equal initial temperature, perfect thermal contact; electrical contact power generated uniformly at their shared plane. DeltaT=2*(I^2*Rc/A)*sqrt(t/pi)/(eCu+eSS), e=sqrt(k*rho*cp).',
    'scope': 'Independent analytical sensitivity for an omitted physical mechanism, not a duplicate native thermal PDE solver or qualified real contact model.',
    'waveform': 'Single rectangular 250 us positive pulse; omits ramps/reverse and repetition. This is not the native bipolar waveform. Fixed resistance and material properties.',
    'material_source': str(project), 'copper': cu, 'steel': ss,
    'nominal_single_joint_area_mm2': nominal_area * 1e6,
    'diffusion_length_sqrt_alpha_t_mm': {m['name']: math.sqrt(m['diffusivity_m2_s'] * duration) * 1000 for m in [cu, ss]},
    'limits': ['Additional local temperature rise only; not directly added to the maximum voxel temperature at a different location.',
               'Resistance and active area are independent uncertain inputs, not measurements or an asserted achievable joint.',
               'No constriction/film geometry, finite tip, lateral spreading, thermal contact resistance, property changes or phase change. Not an upper bound.',
               'Copper thermal penetration is comparable to pad dimensions; semi-infinite one-dimensional assumptions require a resolved joint model.'],
    'references': [{'title': 'Sachdev et al., Failure Mode Analysis of V-Shaped Pyrotechnically Actuated Valves: constant-flux conduction verification', 'url': 'https://ntrs.nasa.gov/api/citations/20120013338/downloads/20120013338.pdf'},
                   {'title': 'Miljkovic, contact between two semi-infinite solids', 'url': 'https://etrl.mechanical.illinois.edu/pdf/ME320/Lecture%2014%20-%20Transient%20Conduction%20-%20Two%20Semi-Infinite%20Bodies.pdf'}],
    'cases': rows,
}
# Exact checks of the analytical response, including the identical-body split.
q, k, rho, cp, time = 1e6, 15., 8000., 500., 250e-6
two_body = 2 * q * math.sqrt(time / math.pi) / (2 * math.sqrt(k*rho*cp))
one_body_half_flux = 2 * (q/2) / k * math.sqrt(k/(rho*cp) * time/math.pi)
assert math.isclose(two_body, one_body_half_flux, rel_tol=1e-14)
for row in rows:
    assert row['additional_interface_temperature_rise_K'] > 0
Path('hardware/contact-flash-screening.json').write_text(json.dumps(result, indent=2) + '\n')
for row in rows:
    if row['single_joint_resistance_ohm'] == 100e-6 and row['active_area_fraction'] in [1., .1]:
        print(row)
