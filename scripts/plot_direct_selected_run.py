"""CAD and accepted fields for the completed 110 A direct-contact study."""
import csv
import argparse
import json
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.patches import Patch
from draw_hardware_layouts import load, section, COLORS
from native_field_io import read_native_fields

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--run',type=Path,default=Path('artifacts/hardware-research/m6-rooted08-flow110-r25-chargefix-h0050'))
parser.add_argument('--output',type=Path,default=Path('hardware/actuator-families/selected-direct-run.png'))
parser.add_argument('--label',default='Direct contact: selected low-current circular-nozzle candidate')
args=parser.parse_args();RUN=args.run
status=json.loads((RUN/'run-status.json').read_text());assert status['status']=='complete'
project=json.loads(Path(status['project']).read_text())
analysis=json.loads((RUN/'flow-analysis.json').read_text());regions=analysis['frames'][-1]['connectivity_thresholds']['0.5']
parts,_=load(project['geometry'])
fig=plt.figure(figsize=(16,11))
gs=fig.add_gridspec(2,4,height_ratios=[2.3,1],left=.055,right=.94,top=.84,bottom=.20,wspace=.38,hspace=.48)
ax=fig.add_subplot(gs[0,0]);section(ax,parts,'xz')
ax.set(xlim=(-1.6,1.6),ylim=(-.4,4.5),title='A. CAD: contact plane')
ax.annotate('copper contacts',xy=(1,.3),xytext=(-1.5,3.6),fontsize=9,arrowprops={'arrowstyle':'-'})
ax.annotate('0.4 mm bore',xy=(0,.2),xytext=(-1.5,-.2),fontsize=9,arrowprops={'arrowstyle':'-'})
ax=fig.add_subplot(gs[0,1]);section(ax,parts,'yz')
ax.set(xlim=(-13,13),ylim=(-3,12),title='B. CAD: magnet plane')
ax.axhline(0,color='#ce7b2e',ls=':',lw=.8)
ax.text(.5,-.36,'M6 nozzle; SmCo pole groups\nContact plane is perpendicular to this view.',transform=ax.transAxes,ha='center',va='top',fontsize=9)
fieldaxes=[]
for col,step in [(2,600),(3,2400)]:
    ax=fig.add_subplot(gs[0,col]);fieldaxes.append(ax)
    p=RUN/f'frame-{step:08d}';m=json.loads((p/'metadata.json').read_text());nx,ny,nz=m['dims']
    f=read_native_fields(p/'fluid.bin',(nz,ny,nx,16));s=np.array(f[:,(ny-1)//2:ny//2+1,:,:].mean(axis=1));del f
    h=m['cell_m']*1000;x0,_,z0=np.array(m['origin_m'])*1000
    x=x0+(np.arange(nx)+.5)*h;z=z0+(np.arange(nz)+.5)*h
    ext=[x0,x0+nx*h,z0,z0+nz*h];alpha=s[:,:,14];opened=s[:,:,12]
    im=ax.imshow(np.ma.masked_where((alpha<.5)|(opened<.5),s[:,:,2]),origin='lower',extent=ext,
                 vmin=-1,vmax=1,cmap='RdBu_r',interpolation='nearest')
    rgba=np.zeros((*alpha.shape,4));rgba[:,:,:3]=.7;rgba[:,:,3]=1-np.clip(opened,0,1)
    ax.imshow(rgba,origin='lower',extent=ext,interpolation='nearest')
    ax.contour(x,z,alpha,levels=[.5],colors=['#253e50'],linewidths=.8)
    ax.axhline(0,color='#ce7b2e',ls=':',lw=.8)
    ax.set(xlim=(-.9,.9),ylim=(-4,1.8),aspect='equal',xlabel='x (mm)',ylabel='z (mm)',
           title=f'{"C" if col==2 else "D"}. Fluid: {m["time_s"]*1e3:.1f} ms')
fig.colorbar(im,ax=fieldaxes,shrink=.8,pad=.04,label='Vertical velocity (m/s); blue = down')
ax=fig.add_subplot(gs[1,:]);rows=list(csv.DictReader((RUN/'current-diagnostics.csv').open()))
t=np.array([float(r['time_s'])*1e6 for r in rows])
ax.plot(t,[float(r['command_A']) for r in rows],color='#287747',lw=2,label='Commanded current')
ax.plot(t,[float(r['current_A']) for r in rows],color='#374350',lw=1,ls=':',label='Solved terminal current')
ax.axhline(0,color='#94a3b8',lw=.7)
peak=status['peak_A'];reverse=peak*status['retract_fraction'];duration=status['positive_duration_s']*1e6;gap=status['retract_delay_s']*1e6
ax.set(xlim=(0,duration*4/3+gap+140),ylim=(-reverse*1.45,peak*1.14),xlabel='Time (µs)',ylabel='Current (A)',title='Same completed run: forward pulse and reverse retrieval')
ax.text(duration/2,peak*1.065,f'+{peak:g} A',ha='center',fontsize=10);ax.text(duration+gap+duration/6,-reverse*1.25,f'−{reverse:g} A',ha='center',fontsize=10)
ax.grid(alpha=.18);ax.legend(loc='upper right',frameon=False)
fig.suptitle(args.label,fontsize=18,y=.965)
fig.text(.5,.91,f'{peak:g} A / {duration:g} µs forward · {gap:g} µs gap · reverse {reverse:g} A / {duration/3:.0f} µs · completed {status["time_s"]*1000:.1f} ms at {status["grid"]["cell_m"]*1e6:.0f} µm grid',ha='center',fontsize=12)
fig.legend(handles=[Patch(facecolor=COLORS[k],label=s) for k,s in [('steel','Stainless'),('liquid','Initial solder'),('copper','Copper contacts'),('magnet','Permanent magnets'),('heater','Heater (CAD only)')]],
           loc='lower center',bbox_to_anchor=(.5,.115),ncol=5,frameon=False)
description=f'{len(regions)} detached region(s) at liquid fraction ≥ 0.5.'
for index,r in enumerate(sorted(regions,key=lambda r:r['volume_nl'],reverse=True)):
    description+=f' Region {index+1}: {r["equivalent_diameter_mm"]:.3f} mm / {-r["velocity_m_s"][2]:.3f} m/s downward.'
fig.text(.5,.075,description+'\nSingle-pulse morphology is not demonstrated repeated jetting or grid convergence.',ha='center',fontsize=10)
fig.text(.5,.025,'Different panel scales. C–D: two central y-cell layers averaged; grey = native solid mask; line = liquid fraction 0.5.\n'
         'Heater shown in CAD, excluded from this flow run. Ideal interfaces; static applied field; no thermal evolution. Existing data only.',ha='center',fontsize=10)
fig.savefig(args.output,dpi=170)
plt.close(fig)
