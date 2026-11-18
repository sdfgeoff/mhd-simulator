"""Prepare a matched-waveform native EM comparison, not a jetting prediction.

Uses the existing custom-chamber carrier case as the source of coil, waveform,
materials and settings. V6 geometry follows the released-drawing approximation
in make_hardware. Both cases include the same fixed feed wire and free level.
"""
import argparse
import copy
import hashlib
import json
import math
import os
from pathlib import Path

from make_hardware import emit, lathe


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    source = json.loads(args.source.read_text())
    if len(source['cad_curves']) != 1 or len(source['electromagnetics']['wires']) != 1:
        raise ValueError('Expected one source coil')
    if args.output.exists():
        raise FileExistsError(args.output)
    args.output.mkdir(parents=True)
    rin, ro, land, length, level = 1.6, .25, .9, 12.5, 9.
    cone_top = land + (rin-ro)/math.tan(math.pi/6)
    profile = [(3.,length,False),(3.,6.5,False),(2.5,6.5,False),
               (2.5,5.,False),(3.5,5.,True),(3.5,2.,True),
               (.625+2*math.tan(math.radians(35)),2.,False),
               (.625,0.,False),(ro,0.,False),(ro,land,False),
               (rin,cone_top,False),(rin,length-.3,False),(rin+.3,length,False)]
    liquid = [(.4,level,False),(rin,level,False),(rin,cone_top,False),
              (ro,land,False),(ro,0.,False),(0.,0.,False),
              (0.,5.5,False),(.4,5.5,False)]
    wire = [(0.,11.,False),(.4,11.,False),(.4,5.5,False),(0.,5.5,False)]
    bodies = [('Stainless nozzle',lathe(profile)),
              ('Liquid solder',lathe(list(reversed(liquid)))),
              ('Fixed feed wire',lathe(wire))]
    curve = source['cad_curves'][0]['points_mm']
    cad = args.output/'v6-carrier.gltf'
    emit(cad,bodies,curve)
    paths = {}
    for name in ['v6','custom']:
        d = copy.deepcopy(source)
        if name == 'v6':
            d['geometry'] = cad.name
            if [b['simulation']['material_id'] for b in d['bodies']] != [14,4,15]:
                raise ValueError('Unexpected source body order')
            for b, (label, triangles) in zip(d['bodies'],bodies):
                b['name'] = label+' / 0'
                b['triangles'] = len(triangles)
        else:
            d['geometry'] = os.path.relpath((args.source.parent/source['geometry']).resolve(), args.output.resolve())
        d['domain'].update(min_mm=[-9.,-9.,-2.],max_mm=[9.,9.,14.5],cell_mm=.2)
        d['run']['fluid_enabled'] = False
        d['run']['thermal_enabled'] = False
        d['fluid']['surface']['enabled'] = False
        path = args.output/(name+'-cold.mhd.json')
        path.write_text(json.dumps(d,indent=2)+'\n')
        paths[name] = str(path)
    metadata = {
        'source':str(args.source),'source_sha256':hashlib.sha256(args.source.read_bytes()).hexdigest(),
        'projects':paths,'matched':['Current table250A50kHz300us40us ramps','CAD centreline including return','Material table','Domain and resolution','EM timestep/tolerances','Fixed feed wire .8mm diameter,tip5.5mm','Free level9mm'],
        'differences':['V6 drawing-based nozzle versus custom6mm chamber/feed neck','Outlet land .9mm versus .15mm','Chamber/liquid volume and internal flow path'],
        'scope':'Cold native EM comparison, not pure wall-shielding isolation, fluid ejection or circuit qualification. No conventional aluminium heater: it clashes with this encircling winding. Alternate heating is unresolved.',
        'v6_dimensions_mm':{'inlet':3.2,'outlet':.5,'length':12.5,'land':.9},
        'coil_clearance_lower_bound_mm':4.948-.6-3.5/math.cos(math.pi/6),
        'geometry_source':'https://e3d-online.com/pages/e3d-classics-support-v6-nozzle',
        'stock_scope':'V6 dimension approximation with0.5mm outlet, not a catalogued E3D stainless0.5mm SKU. Support table lists stainless0.25/0.4/0.8mm. Reaming or another supplier may be needed.',
        'limitations':['Thread helix/fillets omitted; material conductivity and near-unit permeability uncalibrated at temperature.','200um grid under-resolves the outlet.','Same free level is not equal liquid mass; geometry is intentionally different.']}
    (args.output/'comparison.json').write_text(json.dumps(metadata,indent=2)+'\n')
    print(json.dumps(metadata,indent=2))


if __name__ == '__main__':
    main()
