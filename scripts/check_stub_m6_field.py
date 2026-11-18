"""Check small native field windows against accepted full-domain static J."""
import json
from pathlib import Path
import numpy as np
root=Path('artifacts/hardware-research');src=root/'m6-stub-v1-stub4-cold30-h0050'
g=json.loads((src/'grid.json').read_text());nx,ny,nz=g['dims'];h=g['cell_m']
f=np.memmap(src/'em.bin',dtype='<f4',mode='r').reshape(nz,ny,nx,16)
ids=np.memmap(src/'material-ids.bin',dtype='<u4',mode='r').reshape(nz,ny,nx)
rows=[]
for q in [3,4]:
 run=root/f'm6-stub4-q{q}-observer';r=json.loads((run/'report.json').read_text())
 offset=[(a-b)/h for a,b in zip(r['origin_m'],g['origin_m'])]
 if any(abs(x-round(x))>1e-4 for x in offset):raise ValueError('Alignment')
 if abs(r['cell_m']/h-1)>1e-7:raise ValueError('Spacing')
 x,y,z=map(round,offset);dx,dy,dz=r['dims'];sub=f[z:z+dz,y:y+dy,x:x+dx]
 subids=ids[z:z+dz,y:y+dy,x:x+dx]
 obsids=np.fromfile(run/'material-ids.bin',dtype='<u4').reshape(dz,dy,dx)
 if not np.array_equal(subids,obsids):raise ValueError('Phase mismatch')
 obs=np.fromfile(run/'em.bin',dtype='<f4').reshape(dz,dy,dx,16);mask=subids==4
 force=-np.cross(sub[...,4:7].astype(float),obs[...,:3].astype(float))[...,2][mask].sum()*h**3
 row={'subdivisions':q,'mean_By_T':float(obs[...,1][mask].mean()),'outlet_force_N_at30A':float(force),'liquid_cells':int(mask.sum()),'observation':str(run)}
 if q==3:
  row['relative_force_difference_from_full']=float(force/(-sub[...,10][mask].astype(float).sum()*h**3)-1)
  if abs(row['relative_force_difference_from_full'])>1e-4:raise ValueError('Full field reproduction failed')
 rows.append(row)
record={'case':'stub4','status':'Completed applied-source quadrature check with fixed accepted static J','cases':rows,'q4_relative_force_change':rows[1]['outlet_force_N_at30A']/rows[0]['outlet_force_N_at30A']-1,'limits':'Source quadrature sensitivity only; fixed magnetization, static liquid and ideal joints; not mesh-converged jetting.'}
with Path('hardware/m6-stub-source-check.json').open('x') as out:json.dump(record,out,indent=2);out.write('\n')
print(json.dumps(record,indent=2))
