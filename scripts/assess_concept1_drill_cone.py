"""Read-only stationary comparison for the prepared 118 degree cone."""
import argparse,hashlib,json
from pathlib import Path
import numpy as np
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--study',choices=['cone','outlet060'],default='cone');args=p.parse_args()
root=Path('artifacts/hardware-research')
out=Path('hardware/concept1-sturdy-tip-review/cad-drill118-v1')
state_name='concept1-drill118-v1-screen-state.json' if args.study=='cone' else 'concept1-drill118-060-v1-screen-state.json'
if args.study=='outlet060': out=Path('hardware/concept1-sturdy-tip-review/cad-drill118-060-v1')
state=json.loads((root/state_name).read_text())
assert state['stage']=='complete_needs_assessment',state['stage']
for path,digest in state['hashes'].items():
    assert hashlib.sha256(Path(path).read_bytes()).hexdigest()==digest,path
cases=[]
cases_spec=[('Sturdy050 128.7 deg','concept1-sturdy-v1-050-electrical100',.7),('Sturdy050 118 deg','concept1-drill118-v1-050-electrical100',.8510757737844503)] if args.study=='cone' else [('0.5 mm outlet / 118 deg','concept1-drill118-v1-050-electrical100',.8510757737844503),('0.6 mm outlet / 118 deg','concept1-drill118-060-v1-electrical100',.8210327428330723)]
for label,folder,top in cases_spec:
    run=root/folder;report=json.loads((run/'report.json').read_text());grid=json.loads((run/'grid.json').read_text());nx,ny,nz=grid['dims'];h=grid['cell_m']
    fields=np.memmap(run/'em.bin',dtype='<f4',mode='r',shape=(nz,ny,nx,16));ids=np.memmap(run/'material-ids.bin',dtype='<u4',mode='r',shape=(nz,ny,nx))
    liquid=ids==4;z=(grid['origin_m'][2]+(np.arange(nz)+.5)*h)*1000
    force=np.where(liquid,fields[:,:,:,10],0).sum(axis=(1,2),dtype=np.float64)*(-1000*h**3)
    spatial={name:float(force[mask].sum()) for name,mask in [('straight_outlet',z<.1),('taper',(z>=.1)&(z<top)),('upper_pool',z>=top)]}
    assert abs(sum(spatial.values())+1000*report['liquid_force_N'][2])<1e-5
    cases.append({'label':label,'source':str(run),'voltage_V':report['terminal_voltage_V'],'Joule_W':report['joule_power_W'],'downward_liquid_force_mN':-1000*report['liquid_force_N'][2],'downward_force_mN_by_z_region':spatial,'native':report})
result={'scope':'Stationary geometry screen only; not pressure decomposition, jetting or thermal qualification. Region boundary follows each cone top.','cases':cases}
(out/'screen-results.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps([{k:v for k,v in c.items() if k!='native'} for c in cases],indent=2))
