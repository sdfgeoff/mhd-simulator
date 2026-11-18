"""Measure accepted isolated-drop fields against stated kinematic references."""
import argparse
import json
from pathlib import Path

import numpy as np

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('run', type=Path)
parser.add_argument('--mode', choices=['static', 'translation', 'gravity'], required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
status = json.loads((args.run / 'run-status.json').read_text())
if status['status'] != 'complete':
    raise ValueError('Require a completed control')
rows = []
for path in sorted(args.run.glob('frame-*/metadata.json')):
    metadata = json.loads(path.read_text())
    if metadata['byte_order'] != 'little':
        raise ValueError('Expected little-endian field export')
    nx, ny, nz = metadata['dims']
    field = np.memmap(path.parent / 'fluid.bin', dtype='<f4', mode='r').reshape(nz, ny, nx, 16)
    alpha = field[..., 14].astype(float)
    weight = alpha.sum()
    if weight <= 0 or not np.isfinite(weight):
        raise ValueError('Missing liquid')
    cell = metadata['cell_m']
    z = metadata['origin_m'][2] + (np.arange(nz) + .5) * cell
    row = {
        'time_s': metadata['time_s'], 'step': metadata['step'],
        'volume_m3': float(weight * cell**3),
        'COM_z_m': float((alpha * z[:, None, None]).sum() / weight),
        'liquid_vz_m_s': float((alpha * field[..., 2]).sum() / weight),
        'boundary_max_alpha': float(max(alpha[0].max(), alpha[-1].max(),
                                       alpha[:, 0].max(), alpha[:, -1].max(),
                                       alpha[:, :, 0].max(), alpha[:, :, -1].max())),
    }
    materials = {m['id']: m for m in metadata['materials']}
    liquid = materials[metadata['fluid']['material_id']]
    density = (liquid['density_kg_m3'] * alpha
               + materials[0]['density_kg_m3'] * (1 - alpha))
    row['cell_estimated_mixture_momentum_z_kg_m_s'] = float((density * field[..., 2]).sum() * cell**3)
    if all(metadata['fluid']['periodic']):
        # Read-only integral of the production CSF face expression, using the
        # exported alpha/curvature. This does not advance or correct a field.
        curvature = field[..., 15].astype(float)
        interface_weight = alpha * (1 - alpha)
        previous_alpha = np.roll(alpha, 1, axis=0)
        previous_weight = np.roll(interface_weight, 1, axis=0)
        previous_curvature = np.roll(curvature, 1, axis=0)
        denominator = interface_weight + previous_weight
        face_curvature = np.where(
            denominator > 1e-6,
            (interface_weight * curvature + previous_weight * previous_curvature)
            / np.maximum(denominator, 1e-20),
            .5 * (curvature + previous_curvature),
        )
        row['exported_CSF_net_z_N'] = float(
            liquid['surface_tension_n_m'] * (face_curvature * (alpha - previous_alpha)).sum() * cell**2
        )
    rows.append(row)
if not rows or rows[-1]['step'] != status['accepted_steps']:
    raise ValueError('Final accepted frame missing')
initial = rows[0]
if initial['step'] != 0:
    raise ValueError('Initial accepted frame missing')
gravity = metadata['fluid']['acceleration_m_s2'][2] if args.mode == 'gravity' else 0.
for row in rows:
    time = row['time_s'] - initial['time_s']
    row['reference_z_m'] = initial['COM_z_m'] + initial['liquid_vz_m_s'] * time + .5 * gravity * time**2
    row['reference_vz_m_s'] = initial['liquid_vz_m_s'] + gravity * time
    row['position_error_m'] = row['COM_z_m'] - row['reference_z_m']
    row['velocity_error_m_s'] = row['liquid_vz_m_s'] - row['reference_vz_m_s']
    row['relative_volume_drift'] = row['volume_m3'] / initial['volume_m3'] - 1
result = {
    'mode': args.mode, 'run': str(args.run), 'status': status,
    'initial_equivalent_diameter_mm': float((6 * initial['volume_m3'] / np.pi)**(1/3) * 1000),
    'frames': rows,
    'reference_scope': 'Constant translation/static reference or dragless uniform-gravity reference. Gravity ignores small buoyancy/drag and finite-box effects; this is a consistency diagnostic, not independent CFD.',
    'measurement': 'Alpha-weighted exported cell velocity and native-cell COM; use actual initial native volume/centre. Mixture momentum uses cell-centred velocity and is not the exact staggered-face solver momentum. Periodic CSF integral uses exported face curvature weighting; it is a diagnostic, not a correction.',
}
with args.output.open('x') as output:
    json.dump(result, output, indent=2)
    output.write('\n')
print(json.dumps({'mode': args.mode, 'last': rows[-1],
                  'max_volume_drift': max(abs(x['relative_volume_drift']) for x in rows)}, indent=2))
