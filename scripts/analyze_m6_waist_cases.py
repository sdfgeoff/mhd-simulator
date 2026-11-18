"""Measure accepted current/field screens for the custom M6 chamber family."""
import argparse
import json
from pathlib import Path

import numpy as np

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('cases', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
root = Path('artifacts/hardware-research')
manifest = json.loads((args.cases/'manifest.json').read_text())
rows = []
for case in manifest['cases']:
    run = root/f'{args.cases.name}-{case["label"]}-cold30-h0050'
    report = json.loads((run/'report.json').read_text())
    if Path(report['project']).resolve() != Path(case['project']).resolve():
        raise ValueError('Cold source mismatch')
    if report.get('terminal_boundary') != 'x-ends':
        raise ValueError('Expected resolved copper with end-face terminals')
    if report['true_relative_residual'] > 1e-5 or abs(report['power_relative_defect']) > 1e-4:
        raise ValueError('Unaccepted charge/energy residual')
    grid = json.loads((run/'grid.json').read_text())
    nx, ny, nz = grid['dims']
    h = grid['cell_m']
    field = np.memmap(run/'em.bin', dtype='<f4', mode='r').reshape(nz, ny, nx, 16)
    ids = np.memmap(run/'material-ids.bin', dtype='<u4', mode='r').reshape(nz, ny, nx)
    z = grid['origin_m'][2] + (np.arange(nz)+.5)*h
    liquid = ids == 4
    land = liquid & (z[:,None,None] >= 0) & (z[:,None,None] < .0006)
    if not land.any():
        raise ValueError('No outlet liquid')
    force = -float(field[...,10][land].astype(float).sum()*h**3)
    power = report['joule_power_W']
    current = report['current_A']
    materials = {}
    for material in [4,14,16,17]:
        mask = ids == material
        materials[str(material)] = {'joule_W':float(field[...,11][mask].astype(float).sum()*h**3),
                                    'peak_Q_W_m3':float(field[...,11][mask].max())}
    rows.append({**case, 'cold_output':str(run), 'cell_um':h*1e6,
                 'current_A':current, 'nozzle_and_modeled_copper_R_ohm':report['terminal_voltage_V']/current,
                 'static_total_power_W':power, 'liquid_volume_native_mm3':float(liquid.sum()*h**3*1e9),
                 'land_mean_By_T':float(field[...,1][land].astype(float).mean()),
                 'land_downward_force_N':force,
                 'all_liquid_downward_force_N':-report['liquid_force_N'][2],
                 'land_force_per_sqrt_total_power_N_sqrt_W':force/np.sqrt(power),
                 'current_return_relative_error':abs(current+report['return_current_A'])/current,
                 'true_relative_residual':report['true_relative_residual'],
                 'native_power_relative_defect':report['power_relative_defect'], 'by_material':materials})
reference = rows[0]
for row in rows:
    row['land_force_ratio_to_family_control'] = row['land_downward_force_N']/reference['land_downward_force_N']
    row['force_per_sqrt_power_ratio_to_family_control'] = row['land_force_per_sqrt_total_power_N_sqrt_W']/reference['land_force_per_sqrt_total_power_N_sqrt_W']
result = {'status':'Completed cold whole-layout screen; no jetting result', 'source':manifest, 'cases':rows,
          'scope':'Chamber, external relief and magnet gap vary intentionally. Native liquid masks and B fields differ. All share the same 0.4 mm outlet and 0.7 mm tip boss, material parameters, native grid and terminal type. This family also changes funnel angle and raises the magnets relative to the prior slim-tip case.',
          'limits':['Cold binary phase geometry; moving alpha/current, self-field, real contact films, thermal feedback, hydraulic pressure and droplet outcomes are not determined here.',
                    'Land JxB integral is not a chamber-pressure calculation. Total pool force alone does not establish useful nozzle pumping.',
                    'M6 outer envelope is retained but thread helix, machining radii, fit and clamping need detailing. Larger feed bore has less wall at the real thread root.']}
with args.output.open('x') as output:
    json.dump(result, output, indent=2)
    output.write('\n')
for row in rows:
    print(row['label'], 'By',round(row['land_mean_By_T'],4),
          'land mN',round(row['land_downward_force_N']*1000,4),
          'R mOhm',round(row['nozzle_and_modeled_copper_R_ohm']*1000,4),
          'F/sqrtP ratio',round(row['force_per_sqrt_power_ratio_to_family_control'],4))
