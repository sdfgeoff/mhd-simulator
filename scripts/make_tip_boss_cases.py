"""Machine slimmer outlet bosses from the accepted near-flush CAD; no new bore."""
import argparse
import copy
import hashlib
import json
import math
from pathlib import Path

import numpy as np
from make_conduction_hardware import box, triangles
from make_hardware import emit, orient
from make_v6_conduction import as_solid

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('output', type=Path)
parser.add_argument('--only', choices=['control','square08','rect08x06','square06','square07'], nargs='+')
parser.add_argument('--cell-mm', type=float, default=.1)
parser.add_argument('--magnet-subdivisions', type=int, choices=range(5), default=4)
args = parser.parse_args()
if not math.isfinite(args.cell_mm) or args.cell_mm <= 0:
    parser.error('Require positive finite cell spacing')
source = Path('hardware/designs/v6-flat-shortbar/case.mhd.json')
base = json.loads(source.read_text())
geometry = source.parent / base['geometry']

def read_parts(path):
    document = json.loads(path.read_text())
    buffers = [(path.parent / b['uri']).read_bytes() for b in document['buffers']]
    result = []
    for mesh in document['meshes']:
        primitive, = mesh['primitives']
        if primitive['mode'] != 4 or 'indices' in primitive:
            raise ValueError('Require the generated unindexed triangle format')
        accessor = document['accessors'][primitive['attributes']['POSITION']]
        view = document['bufferViews'][accessor['bufferView']]
        if accessor['componentType'] != 5126 or accessor['type'] != 'VEC3':
            raise ValueError('Require f32 positions')
        offset = view.get('byteOffset', 0) + accessor.get('byteOffset', 0)
        # Convert to f64 before units, preserving unmodified source f32 vertices.
        points = np.frombuffer(buffers[view['buffer']], dtype='<f4', count=accessor['count']*3,
                               offset=offset).astype(float).reshape(-1, 3, 3) * 1000
        result.append((mesh['name'], points.tolist()))
    return result

parts = read_parts(geometry)
nozzle = as_solid(parts[0][1])
# Simplify only modified meshes to remove coplanar boolean slivers (10 nm).
machining_source = nozzle

def export_mesh(shape):
    values = np.asarray(triangles(shape.set_tolerance(1e-5).simplify()))
    return orient([tuple(tuple(float(v) for v in point) for point in triangle) for triangle in values])
liquid = as_solid(parts[1][1])
args.output.mkdir(parents=True, exist_ok=False)
manifest = {'source_project': str(source), 'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
            'source_geometry': str(geometry), 'cell_mm': args.cell_mm,
            'magnet_quadrature_subdivisions': args.magnet_subdivisions, 'cases': [],
            'scope': 'Cold geometry screen only. Same bore, pool, shoulder, magnets, domain and material properties. 0.1 mm minimum wall is a manufacturing/thermal risk, not a recommendation.'}
for label, xwidth, ywidth in [('control', .8, None), ('square08', .8, .8),
                              ('rect08x06', .8, .6), ('square06', .6, .6), ('square07', .7, .7)]:
    if args.only and label not in args.only:
        continue
    bodies = copy.deepcopy(parts)
    shape = nozzle
    if label != 'control':
        shape = machining_source
        for axis, width in [(0, xwidth), (1, ywidth)]:
            for sign in [-1, 1]:
                size = [20., 20., .62]
                centre = [0., 0., .29]
                centre[axis] = sign * (10 + width/2)
                shape = shape - box(size, centre)
        bodies[0] = ('Machined outlet boss ' + label, export_mesh(shape))
        for i, sign in enumerate([-1, 1]):
            pad = box([5 - xwidth/2, ywidth, .4], [sign*(5+xwidth/2)/2, 0., .3])
            bodies[2+i] = (parts[2+i][0], export_mesh(pad))
            if (shape ^ pad).volume() > 1e-6 or (liquid ^ pad).volume() > 1e-6:
                raise ValueError('Contact overlap')
    if shape.volume() <= 0 or (shape ^ liquid).volume() > 1e-6:
        raise ValueError(f'Invalid nozzle/liquid separation: {label}, overlap={(shape ^ liquid).volume()}')
    path = args.output / (label + '.gltf')
    emit(path, bodies, None)
    checked = read_parts(path)
    unchanged = [1, 4, 5] if label != 'control' else list(range(len(parts)))
    for i in unchanged:
        if not np.array_equal(np.asarray(checked[i][1]), np.asarray(parts[i][1])):
            raise ValueError('Unmodified geometry changed')
    project = copy.deepcopy(base)
    project['geometry'] = path.name
    project['domain']['cell_mm'] = args.cell_mm
    for magnet in project['electromagnetics']['magnets']:
        magnet['subdivisions'] = args.magnet_subdivisions
    for body, (name, mesh) in zip(project['bodies'], bodies):
        body['name'], body['triangles'] = name + ' / 0', len(mesh)
    output = path.with_suffix('.mhd.json')
    output.write_text(json.dumps(project, indent=2) + '\n')
    manifest['cases'].append({'label': label, 'project': str(output), 'x_width_mm': xwidth,
                              'y_width_mm': ywidth, 'flat_reach_mm': .6,
                              'modified_mesh_simplification_tolerance_mm': 1e-5 if label != 'control' else 0.,
                              'minimum_nominal_wall_mm': (min(xwidth, ywidth or xwidth)-.4)/2,
                              'unchanged_triangle_bodies': unchanged,
                              'contact_area_mm2': .4*(ywidth or .8),
                              'nozzle_volume_mm3': shape.volume()})
(args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps(manifest['cases'], indent=2))
