"""Compare completed native stationary cases; never launches a solver."""
import hashlib,json
from pathlib import Path
import numpy as np
root=Path('artifacts/hardware-research')
review=Path('hardware/concept1-larger-poles-review')
state=json.loads((root/'concept1-larger-poles-v1-state.json').read_text())
assert state['stage']=='complete_needs_assessment',state['stage']
for p,h in state['hashes'].items():assert hashlib.sha256(Path(p).read_bytes()).hexdigest()==h,p
cases=[]
for name,folder in [('5mm cubes','concept1-developed-v1-electrical-ideal'),('10x10x5mm poles','concept1-larger-poles-v1-electrical100')]:
    run=root/folder;r=json.loads((run/'report.json').read_text())
    assert r['boundary_current_defect']<=1e-5 and abs(r['power_relative_defect'])<=1e-4
    g=json.loads((run/'grid.json').read_text());nx,ny,nz=g['dims'];h=g['cell_m'];o=np.array(g['origin_m'])
    f=np.memmap(run/'em.bin',dtype='<f4',mode='r',shape=(nz,ny,nx,16))
    x=o[0]+(np.arange(nx)+.5)*h;y=o[1]+(np.arange(ny)+.5)*h;z=o[2]+(np.arange(nz)+.5)*h
    ix=int(np.argmin(abs(x)));iy=int(np.argmin(abs(y)));zz=(z>=-.0005)&(z<=.0001)
    b=float(np.mean(f[zz,iy,ix,0],dtype=np.float64))
    cases.append({'name':name,'native':r,'near_outlet_Bx_T':b})
ratio=cases[1]['native']['liquid_force_N'][2]/cases[0]['native']['liquid_force_N'][2]
out={'status':'Completed stationary comparison only; no new jetting result',
     'cases':cases,'downward_force_ratio':ratio,
     'scope':'Same100A, all nonmagnet body masks identical, idealCuSS, fixed hot properties. Full source meshes; cropped isolated outer magnet occupancy. Not dynamic impulse or thermal qualification.'}
(review/'stationary-comparison.json').write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps({'cases':[{'name':c['name'],'B_T':c['near_outlet_Bx_T'],'Fz_N':c['native']['liquid_force_N'][2],'V':c['native']['terminal_voltage_V'],'W':c['native']['joule_power_W']} for c in cases],'force_ratio':ratio},indent=2))
