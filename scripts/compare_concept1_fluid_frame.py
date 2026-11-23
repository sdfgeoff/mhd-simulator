"""Read-only comparison of a shared accepted frame; explicitly a partial diagnostic."""
import argparse,json
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from native_field_io import read_native_fields
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--step',type=int,default=200);p.add_argument('--study',choices=['magnets','pulse','pulse-v4','flightspace','sturdy-040','sturdy-050','drill118','outlet060','pullback060'],default='magnets');a=p.parse_args()
root=Path('artifacts/hardware-research');out=Path('hardware/concept1-larger-poles-review')
studies={
    'magnets':[('5 mm cubes','concept1-developed-flow-v1-driven100'),('10 x 10 x 5 mm poles','concept1-larger-poles-flow-v1-driven100')],
    'pulse':[('250 us forward pulse','concept1-larger-poles-flow-v1-driven100'),('400 us forward pulse','concept1-long-pulse-flow-v3-driven100')],
    'pulse-v4':[('250 us forward pulse','concept1-larger-poles-flow-v1-driven100'),('400 us forward pulse','concept1-long-pulse-flow-v4-driven100')],
    'flightspace':[('Lower boundary Z=-3.5 mm','concept1-long-pulse-flow-v4-driven100'),('Lower boundary Z=-8.5 mm','concept1-flightspace-flow-v3-driven100')],
}
studies['sturdy-040']=[('Earlier 0.8 mm tip','concept1-flightspace-flow-v3-driven100'),('Sturdy 1.2 mm tip / 0.4 mm outlet','concept1-sturdy-flow-v2-040-driven100')]
studies['sturdy-050']=[('Sturdy tip / 0.4 mm outlet','concept1-sturdy-flow-v2-040-driven100'),('Sturdy tip / 0.5 mm outlet','concept1-sturdy-flow-v1-050-driven100')]
studies['drill118']=[('Sturdy050 / 128.7° cone','concept1-sturdy-flow-v1-050-driven100'),('Sturdy050 / 118° cone','concept1-drill118-flow-v1-050-driven100')]
studies['outlet060']=[('0.5 mm outlet / 118° cone','concept1-drill118-flow-v1-050-driven100'),('0.6 mm outlet / 118° cone','concept1-drill118-060-flow-v1-driven100')]
studies['pullback060']=[('25 A pullback','concept1-drill118-060-flow-v1-driven100'),('50 A pullback','concept1-drill118-060-retract050-v1-driven100')]
if a.study in ['sturdy-040','sturdy-050','drill118','outlet060','pullback060']: out=Path('hardware/concept1-sturdy-tip-review')
cases=studies[a.study]
for _,name in cases:
    records=json.loads((root/name/'flow-analysis.json').read_text())['frames']
    assert any(r['step']==a.step for r in records),'Shared accepted frame not available: '+name
fig,axes=plt.subplots(1,2,figsize=(9,7),constrained_layout=True);results=[]
for ax,(label,name) in zip(axes,cases):
    run=root/name;record=next(x for x in json.loads((run/'flow-analysis.json').read_text())['frames'] if x['step']==a.step)
    path=run/f'frame-{a.step:08d}';m=json.loads((path/'metadata.json').read_text());nx,ny,nz=m['dims'];h=m['cell_m']*1000;o=np.array(m['origin_m'])*1000
    f=read_native_fields(path/'fluid.bin',(nz,ny,nx,16));section=f[:,(ny-1)//2:ny//2+1,:,14].mean(axis=1);solid=1-f[:,(ny-1)//2:ny//2+1,:,12].mean(axis=1)
    extent=[o[0],o[0]+nx*h,o[2],o[2]+nz*h]
    outlet_label=(f"outlet {abs(record['exit_plane_mean_uz_m_s']):.2f} m/s "+('down' if record['exit_plane_mean_uz_m_s']<0 else 'up') if record['exit_plane_alpha_area_mm2']>1e-4 else 'outlet nearly dry')
    im=ax.imshow(section,origin='lower',extent=extent,vmin=0,vmax=1,cmap='Blues',interpolation='nearest')
    rgba=np.full((*solid.shape,4),.55);rgba[...,3]=solid;ax.imshow(rgba,origin='lower',extent=extent,interpolation='nearest')
    ax.axhline(-.5,color='darkorange',linestyle=':');ax.set(xlim=(-1.8,1.8),ylim=(-1.6,4.1),xlabel='x (mm)',ylabel='z (mm)',title=label+'\n'+f"{record['below_tip_nl']:.1f} nL below; "+outlet_label)
    results.append({'case':label,'native_frame':str(path),'measurements':record});del f
lower=min(-1.6,min(c['measurements']['front_z_mm_alpha_half'] for c in results if c['measurements']['front_z_mm_alpha_half'] is not None)-.4)
for ax in axes: ax.set_ylim(lower,4.1)
connectivity_note=("no detached region in these frames" if all(not regions for c in results for regions in c["measurements"]["connectivity_thresholds"].values()) else "detached-region counts at alpha=0.5: "+" / ".join(str(len(c["measurements"]["connectivity_thresholds"]["0.5"])) for c in results))
completed=all((root/name/'run-status.json').exists() and json.loads((root/name/'run-status.json').read_text())['status']=='complete' for _,name in cases)
scope='completed runs' if completed and a.step==1200 else 'partial response'
control='Same 100 A pulse / 100 µm grid' if a.study=='magnets' else 'Same head, magnets, 100 A peak / 100 µm grid'
if a.study in ['flightspace','sturdy-040','sturdy-050','drill118','outlet060']: control='Same 100 A / 400 us pulse, magnets and 100 µm grid'
if a.study=='pullback060': control='Same geometry / 100 A forward pulse / 100 µm grid; changed pullback'
fig.suptitle(f"Concept1: shared {results[0]['measurements']['time_us']:.0f} us frame — {scope}\n{control}; {connectivity_note}",fontsize=12)
fig.colorbar(im,ax=axes,label='Liquid fraction',shrink=.6)
prefix={'magnets':'flow-comparison','pulse':'flow-pulse-comparison','pulse-v4':'flow-pulse-v4-comparison','flightspace':'flow-flightspace-comparison','sturdy-040':'flow-sturdy-040-comparison','sturdy-050':'flow-sturdy-050-comparison','drill118':'flow-drill118-comparison','outlet060':'flow-outlet060-comparison','pullback060':'flow-pullback060-comparison'}[a.study]
stem=f'{prefix}-prefix-step{a.step}'
fig.savefig(out/(stem+'.png'),dpi=150)
(out/(stem+'.json')).write_text(json.dumps({'scope':'Shared accepted frame only; running candidate is not a final detachment result. Coarse outlet sampling; breakup unqualified.','study':a.study,'cases':results},indent=2)+'\n')
