"""CAD and accepted fields for the completed 110 A direct-contact study."""
import csv
import json
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.patches import Patch
from draw_hardware_layouts import load, section, COLORS
from native_field_io import read_native_fields

RUN=Path('artifacts/hardware-research/m6-rooted08-flow110-r25-chargefix-h0050')
evidence=json.loads(Path('hardware/m6-rooted-flow-refinement-screening.json').read_text())['runs'][1]
assert str(RUN)==evidence['source'] and evidence['native_status']['status']=='complete'
project=json.loads(Path(evidence['native_status']['project']).read_text())
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
ax.set(xlim=(0,500),ylim=(-40,125),xlabel='Time (µs)',ylabel='Current (A)',title='Same completed run: forward pulse and reverse retrieval')
ax.text(120,117,'+110 A',ha='center',fontsize=10);ax.text(320,-34,'−27.5 A',ha='center',fontsize=10)
ax.grid(alpha=.18);ax.legend(loc='upper right',frameon=False)
fig.suptitle('Direct contact: selected low-current circular-nozzle candidate',fontsize=18,y=.965)
fig.text(.5,.91,'110 A / 250 µs forward · 25 µs gap · reverse 27.5 A / 83 µs · completed 3.6 ms at 50 µm grid',ha='center',fontsize=12)
fig.legend(handles=[Patch(facecolor=COLORS[k],label=s) for k,s in [('steel','Stainless'),('liquid','Initial solder'),('copper','Copper contacts'),('magnet','Permanent magnets'),('heater','Heater (CAD only)')]],
           loc='lower center',bbox_to_anchor=(.5,.115),ncol=5,frameon=False)
fig.text(.5,.075,'Final leading region: 0.596 mm / 0.889 m/s downward. Secondary region: 0.417 mm / 0.076 m/s downward.\n'
         'These diameters use liquid fraction ≥ 0.5; both regions are shown. This is not clean single-drop release.',ha='center',fontsize=10)
fig.text(.5,.025,'Different panel scales. C–D: two central y-cell layers averaged; grey = native solid mask; line = liquid fraction 0.5.\n'
         'Heater shown in CAD, excluded from this flow run. Ideal interfaces; static applied field; no thermal evolution. Existing data only.',ha='center',fontsize=10)
fig.savefig('hardware/actuator-families/selected-direct-run.png',dpi=170)
plt.close(fig)
