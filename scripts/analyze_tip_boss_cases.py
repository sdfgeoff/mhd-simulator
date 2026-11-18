"""Compare accepted cold electrical fields for the matched outlet-boss family."""
import argparse
import json
from pathlib import Path

import numpy as np

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('cases', type=Path)
parser.add_argument('prefix', help='Cold-output prefix relative to artifacts/hardware-research')
parser.add_argument('output', type=Path)
parser.add_argument('--spacing-label', default='h010')
parser.add_argument('--reference', type=Path, help='Accepted family record providing a control at identical settings')
args = parser.parse_args()
root = Path('artifacts/hardware-research')
manifest = json.loads((args.cases / 'manifest.json').read_text())
reference = json.loads(args.reference.read_text()) if args.reference else None
reference_project = None
if reference:
    reference_project = json.loads(Path(reference['cases'][0]['project']).read_text())
    if reference['source']['source_sha256'] != manifest['source_sha256']:
        raise ValueError('Different source family')
rows = []
for case in manifest['cases']:
    run = root / f'{args.prefix}-{case["label"]}-cold30-{args.spacing_label}'
    report = json.loads((run / 'report.json').read_text())
    project = json.loads(Path(case['project']).read_text())
    if Path(report['project']).resolve() != Path(case['project']).resolve():
        raise ValueError('Cold source/project mismatch')
    if reference_project:
        for key in ['materials', 'domain', 'electromagnetics']:
            if project[key] != reference_project[key]:
                raise ValueError('Reference settings differ: '+key)
    grid = json.loads((run / 'grid.json').read_text())
    nx, ny, nz = grid['dims']
    field = np.memmap(run/'em.bin', dtype='<f4', mode='r').reshape(nz, ny, nx, 16)
    ids = np.memmap(run/'material-ids.bin', dtype='<u4', mode='r').reshape(nz, ny, nx)
    cell = grid['cell_m']
    z = grid['origin_m'][2] + (np.arange(nz)+.5)*cell
    land = (ids == 4) & (z[:, None, None] >= 0) & (z[:, None, None] < .0006)
    steel = ids == 14
    force = -float(field[..., 10][land].astype(float).sum()*cell**3)
    current = report['current_A']
    power = report['joule_power_W']
    material = next(m for m in project['materials'] if m['id'] == 14)
    peak_q = float(field[..., 11][steel].max())
    rows.append({**case, 'cold_output': str(run), 'cell_um': cell*1e6,
                 'current_A': current, 'resistance_ohm': report['terminal_voltage_V']/current,
                 'head_power_at_30A_W': power, 'outlet_land_downward_force_N': force,
                 'total_liquid_downward_force_N': -report['liquid_force_N'][2],
                 'liquid_power_W': report['liquid_joule_power_W'],
                 'land_force_per_sqrt_head_power_N_sqrt_W': force/np.sqrt(power),
                 'steel_peak_Q_W_m3': peak_q,
                 'steel_peak_adiabatic_rectangular_175A250us_rise_K': peak_q*(175/current)**2*.00025/(material['density_kg_m3']*material['thermal']['heat_capacity_j_kg_k']),
                 'true_relative_residual': report['true_relative_residual'],
                 'power_relative_defect': report['power_relative_defect']})
base = reference['cases'][0] if reference else rows[0]
base_run = Path(base['cold_output'])
base_grid = json.loads((base_run/'grid.json').read_text())
base_ids = np.memmap(base_run/'material-ids.bin', dtype='<u4', mode='r')
base_field = np.memmap(base_run/'em.bin', dtype='<f4', mode='r').reshape(-1,16)
base_project = json.loads(Path(base['project']).read_text())
for row in rows:
    run = Path(row['cold_output'])
    if json.loads((run/'grid.json').read_text()) != base_grid:
        raise ValueError('Native grid differs')
    project = json.loads(Path(row['project']).read_text())
    for key in ['materials','domain','electromagnetics']:
        if project[key] != base_project[key]:
            raise ValueError('Control settings differ: '+key)
    ids = np.memmap(run/'material-ids.bin', dtype='<u4', mode='r')
    field = np.memmap(run/'em.bin', dtype='<f4', mode='r').reshape(-1,16)
    row['native_liquid_mask_matches_control'] = bool(np.array_equal(ids==4, base_ids==4))
    row['applied_B_matches_control_bitwise'] = bool(np.array_equal(field[:,:4], base_field[:,:4]))
    if not row['native_liquid_mask_matches_control'] or not row['applied_B_matches_control_bitwise']:
        raise ValueError('Unintended liquid or magnetic-field difference')
    row['land_force_ratio_to_control'] = row['outlet_land_downward_force_N']/base['outlet_land_downward_force_N']
    row['resistance_ratio_to_control'] = row['resistance_ohm']/base['resistance_ohm']
    row['force_per_sqrt_power_ratio_to_control'] = row['land_force_per_sqrt_head_power_N_sqrt_W']/base['land_force_per_sqrt_head_power_N_sqrt_W']
result = {'status': 'Completed cold screen; no moving-fluid or thermal qualification',
          'source': manifest, 'cases': rows, 'reference': str(args.reference) if args.reference else None,
          'scope': 'Identical bore/pool, magnets, material parameters and full domain. Shape/contact widths intentionally differ. Wall resolution follows the exported spacing; neither grid establishes peak temperature or moving-interface convergence. Land metric integrates liquid JxB below z=0.6 mm, not a pressure solve.',
          'thermal_scope': 'Adiabatic local Q/(rho cp) sensitivity at a rectangular 175 A/250 us pulse, not native heat diffusion or bipolar waveform. Real contacts, property changes, self-field and repetition omitted.'}
with args.output.open('x') as output:
    json.dump(result, output, indent=2)
    output.write('\n')
for row in rows:
    print(row['label'], 'land mN', row['outlet_land_downward_force_N']*1000,
          'R mOhm', row['resistance_ohm']*1000,
          'force/sqrtP ratio', row['force_per_sqrt_power_ratio_to_control'],
          'adiabatic175 K', row['steel_peak_adiabatic_rectangular_175A250us_rise_K'])
