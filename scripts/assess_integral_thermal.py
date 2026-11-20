"""Assess ideal bore-wall-to-pool conductance; no operating-temperature calculation."""
import argparse,json
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--study',choices=['initial','near','vertical'],default='initial');args=parser.parse_args()
out=Path('hardware/integral-head-review')/({'near':'near-heater','vertical':'vertical-heater'}.get(args.study,''));cases=[]
inputs=[(k,Path(f'artifacts/hardware-research/integral-{k}-v2-thermal-wall-pool-h0100')) for k in ['slotted-arms','side-heated']] if args.study=='initial' else [('side-heated',Path('artifacts/hardware-research/integral-side-heated-v2-thermal-wall-pool-h0100')),('side-heated-near',Path('artifacts/hardware-research/integral-side-heated-near-v2-thermal-wall-pool-h0100'))]
if args.study=='vertical':inputs=[('Horizontal near: ideal wall',Path('artifacts/hardware-research/integral-side-heated-near-v2-thermal-wall-pool-h0100')),('Vertical: ideal wall',Path('artifacts/hardware-research/integral-vertical-heater-v2-thermal-ideal-h0100')),('Vertical: nominal active span',Path('artifacts/hardware-research/integral-vertical-heater-v2-thermal-active-span-h0100'))]
fig,axes=plt.subplots(1,len(inputs),figsize=(6*len(inputs),10 if args.study=='vertical' else 8));fig.subplots_adjust(left=.06,right=.90,top=.85,bottom=.25,wspace=.30)
def excluded_slice(config,x,y,z):
    xyz=[x[None,:],y[:,None],z]; mask=np.zeros((len(y),len(x)),dtype=bool)
    for name in ['heater','sensor']:
        bore=config[name];axis=bore['axis'];radius2=sum((xyz[j]-bore['centre_mm'][j])**2 for j in range(3) if j!=axis)
        mask|=(radius2 < bore['void_radius_mm']**2)&(xyz[axis]>=bore['void_span_mm'][0])&(xyz[axis]<=bore['void_span_mm'][1])
    return mask
for ax,(kind,run) in zip(axes,inputs):
    r=json.loads((run/'report.json').read_text());g=json.loads((run/'grid.json').read_text());nx,ny,nz=g['dims'];h=g['cell_m'];origin=np.array(g['origin_m']);f=np.memmap(run/'potential.bin',dtype='<f4',mode='r',shape=(nz,ny,nx));ids=np.memmap(run/'material-ids.bin',dtype='<u4',mode='r',shape=(nz,ny,nx));x=(origin[0]+(np.arange(nx)+.5)*h)*1000;z=(origin[2]+(np.arange(nz)+.5)*h)*1000;hx,hz=r['heater_centre_xz_mm'];sx,sz=r.get('sensor_centre_xz_mm',[hx+2.5,hz]);y=(origin[1]+(np.arange(ny)+.5)*h)*1000
    config=r.get('boundary_geometry',{'heater':{'axis':1,'centre_mm':[hx,0,hz],'void_radius_mm':1.6,'void_span_mm':[-1e6,1e6]},'sensor':{'axis':1,'centre_mm':[sx,0,sz],'void_radius_mm':.6,'void_span_mm':[-1e6,1e6]}})
    excluded=np.empty((nz,nx),dtype=bool)
    lo=1.;hi=0.
    for k in range(nz):
        omitted=excluded_slice(config,x,y,z[k]);excluded[k]=omitted[ny//2];mask=np.isin(ids[k],[4,14,16,17])&~omitted;v=f[k][mask]+.5
        if len(v):lo=min(lo,float(v.min()));hi=max(hi,float(v.max()))
    assert lo>=-2e-5 and hi<=1+2e-5,'Maximum principle failure'
    assert r['return_relative_error']<1e-5 and abs(r['energy_identity_relative_error'])<1e-4
    cases.append({'concept':kind,'source':str(run),'native':r,'normalized_body_temperature_range':[lo,hi]})
    m=np.array(ids[:,ny//2,:]);mask=np.isin(m,[4,14,16,17])&~excluded;v=np.array(f[:,ny//2,:])+.5;extent=[origin[0]*1000,(origin[0]+nx*h)*1000,origin[2]*1000,(origin[2]+nz*h)*1000]
    im=ax.imshow(np.ma.masked_where(~mask,v),origin='lower',extent=extent,cmap='inferno',vmin=0,vmax=1,interpolation='nearest');ax.contour(x,z,mask.astype(float),levels=[.5],colors='#71808b',linewidths=.5);ax.contour(x,z,(m==4).astype(float),levels=[.5],colors='#52a4c9',linewidths=.7)
    ax.set(xlim=(-11.5,10.5) if args.study=='vertical' else (-10.5,10.5),ylim=(-.5,29) if args.study=='vertical' else (-.5,20),aspect='equal',xlabel='x (mm)',ylabel='z (mm)',title=kind.replace('-',' ').title()+f'\nG = {r["conductance_W_K"]:.4f} W/K; R = {r["thermal_resistance_K_W"]:.1f} K/W')
    del f,ids
cax=fig.add_axes([.92,.30,.018,.48]);fig.colorbar(im,cax=cax,label='(T − Tpool) / (Theater wall − Tpool)')
fig.suptitle('Optimistic heat delivery through the stainless body',fontsize=17,y=.97)
fig.text(.5,.90,'Prescribed bore-wall temperature · isothermal pool · adiabatic exterior · fixed steel k = 15 W/(m K)',ha='center',fontsize=11)
fig.text(.5,.12,'Each 1 W delivered to the pool needs R K wall-to-pool difference (R shown above).\nThese are unit-conductance fields, not predicted operating temperatures or demonstrated heater powers.\nHeater/interface resistance, external losses, Joule heat, feed melting and magnet thermal paths are omitted.',ha='center',fontsize=10)
fig.text(.5,.025,('Active-span wall: Z = 8.6–20 mm. No heater-internal or cold-tip heat spreading is solved; this is not a complete cartridge model.' if args.study=='vertical' else 'Ideal body-wall boundary avoids assuming thermal contact across sub-cell heater/sensor gaps; floating sheaths are omitted from this thermal screen.'),ha='center',fontsize=9)
fig.savefig(out/'thermal-results.png',dpi=160);plt.close(fig)
(out/'thermal-results.json').write_text(json.dumps({'status':'Completed optimistic bulk-metal conduction screens; not full heat balance','cases':cases},indent=2)+'\n');print(json.dumps([{'concept':c['concept'],'R_K_W':c['native']['thermal_resistance_K_W'],'range':c['normalized_body_temperature_range']} for c in cases],indent=2))
