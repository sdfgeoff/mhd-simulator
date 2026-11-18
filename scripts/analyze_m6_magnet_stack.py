"""Reuse accepted static J with native small-window applied-field observations."""
import json
from pathlib import Path

import numpy as np

root=Path('artifacts/hardware-research')
source=root/'m6-waist-v1-ch12-w02-cold30-h0050'
grid=json.loads((source/'grid.json').read_text())
nx,ny,nz=grid['dims']
full=np.memmap(source/'em.bin',dtype='<f4',mode='r').reshape(nz,ny,nx,16)
full_ids=np.memmap(source/'material-ids.bin',dtype='<u4',mode='r').reshape(nz,ny,nx)
rows=[]
for stack,quad in [(2,3),(1,4),(2,4),(3,4)]:
    run=root/f'm6-magstack{stack}-q{quad}-observer'
    report=json.loads((run/'report.json').read_text())
    if report['status']!='complete applied-field observation':raise ValueError('Incomplete field')
    if abs(report['cell_m']/grid['cell_m']-1)>1e-7:raise ValueError('Cell spacing mismatch')
    offsets=[(o-r)/grid['cell_m'] for o,r in zip(report['origin_m'],grid['origin_m'])]
    if any(abs(v-round(v))>1e-4 for v in offsets):raise ValueError('Observation grid is not aligned')
    x,y,z=[round(v) for v in offsets];dx,dy,dz=report['dims']
    sub=full[z:z+dz,y:y+dy,x:x+dx]
    ids=full_ids[z:z+dz,y:y+dy,x:x+dx]
    observed_ids=np.memmap(run/'material-ids.bin',dtype='<u4',mode='r').reshape(dz,dy,dx)
    if not np.array_equal(ids,observed_ids):raise ValueError('Nozzle/liquid observation changed')
    observed=np.memmap(run/'em.bin',dtype='<f4',mode='r').reshape(dz,dy,dx,16)
    liquid=ids==4
    current=sub[...,4:7].astype(float)
    magnetic=observed[...,:3].astype(float)
    force=np.cross(current,magnetic)
    row={'stack_per_pole':stack,'magnets_total':2*stack,'source_subdivision':quad,
         'observation':str(run),'mean_outlet_By_T':float(magnetic[...,1][liquid].mean()),
         'outlet_land_force_N_at30A':(force[liquid].sum(axis=0)*grid['cell_m']**3).tolist(),
         'outlet_downward_force_N_at30A':float(-force[...,2][liquid].sum()*grid['cell_m']**3),
         'liquid_cells':int(liquid.sum()),'integer_source_offset': [x,y,z]}
    if stack==2 and quad==3:
        row['baseline_B_max_difference_T']=float(np.max(abs(magnetic-sub[...,:3])))
        row['baseline_force_relative_difference']=float(force[...,2][liquid].sum()/sub[...,10][liquid].astype(float).sum()-1)
        if abs(row['baseline_force_relative_difference'])>1e-4:raise ValueError('Baseline observation failed to reproduce full-domain force')
    rows.append(row)
reference=next(row for row in rows if row['stack_per_pole']==2 and row['source_subdivision']==4)
for row in rows:
    row['force_ratio_to_two_stack_q4']=row['outlet_downward_force_N_at30A']/reference['outlet_downward_force_N_at30A']
    row['peak_A_matching_two_stack110A']=110/row['force_ratio_to_two_stack_q4']
    row['matching_RMS_A_at_assumed200Hz']=row['peak_A_matching_two_stack110A']*(.000226875*200)**.5
record={'status':'Completed native applied-field comparison on a validated shared J observation window',
        'full_charge_source':str(source),'cases':rows,
        'scope':'Reuses full-domain stationary current because changing B does not alter J at zero velocity with fixed nonmagnetic material properties. Every field observation samples the full magnet CAD; no cropped charge solve. Native grids and phase IDs checked. Cross-products are field postprocessing, not a new PDE solver.',
        'limits':['Equal fixed source magnetization and abutting stacks; actual hot grade, demagnetization and mount tolerances unqualified.',
                  'No moving liquid, u-cross-B, self-field, thermal feedback or droplet/repetition outcome.',
                  'Matched-force amplitudes are comparisons only, not physical-head commands.']}
with Path('hardware/m6-magnet-stack-screening.json').open('x') as output:
    json.dump(record,output,indent=2);output.write('\n')
for row in rows:print(row)
