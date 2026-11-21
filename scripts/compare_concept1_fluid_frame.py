"""Read-only comparison of a shared accepted frame; explicitly a partial diagnostic."""
import argparse,json
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from native_field_io import read_native_fields
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--step',type=int,default=200);a=p.parse_args()
root=Path('artifacts/hardware-research');out=Path('hardware/concept1-larger-poles-review')
fig,axes=plt.subplots(1,2,figsize=(9,7),constrained_layout=True);results=[]
for ax,(label,name) in zip(axes,[('5 mm cubes','concept1-developed-flow-v1-driven100'),('10 x 10 x 5 mm poles','concept1-larger-poles-flow-v1-driven100')]):
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
completed=all((root/name/'run-status.json').exists() and json.loads((root/name/'run-status.json').read_text())['status']=='complete' for name in ['concept1-developed-flow-v1-driven100','concept1-larger-poles-flow-v1-driven100'])
scope='completed runs' if completed and a.step==1200 else 'partial response'
fig.suptitle(f"Concept1: shared {results[0]['measurements']['time_us']:.0f} us frame — {scope}\nSame 100 A pulse / 100 µm grid; {connectivity_note}",fontsize=12)
fig.colorbar(im,ax=axes,label='Liquid fraction',shrink=.6)
stem=f'flow-comparison-prefix-step{a.step}'
fig.savefig(out/(stem+'.png'),dpi=150)
(out/(stem+'.json')).write_text(json.dumps({'scope':'Shared accepted frame only; running larger-pole case is not a final detachment result. Four cells across outlet; breakup unqualified.','cases':results},indent=2)+'\n')
