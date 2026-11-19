"""Render existing low-fill CAD, accepted fields and pulse history; no new solve."""
import csv
import json
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np
from draw_hardware_layouts import load, section, coils
from native_field_io import read_native_fields

ROOT = Path('artifacts/hardware-research')
RUN = ROOT / 'coil-low-fill-flow-v1-250A-h020'
PROJECT = ROOT / 'cases/coil-low-fill-flow-v1/low-fill-250A-flow-h020.mhd.json'
OUT = Path('hardware/actuator-families/selected-coil-run.png')
project = json.loads(PROJECT.read_text())
status = json.loads((RUN/'run-status.json').read_text())
assert status['status'] == 'complete'
samples = np.array(project['electromagnetics']['wires'][0]['current']['samples'])
parts, curves = load(project['geometry'])
fig = plt.figure(figsize=(15,11))
gs = fig.add_gridspec(2,3,height_ratios=[2.2,1],left=.06,right=.94,top=.85,bottom=.18,hspace=.45,wspace=.35)
cad = fig.add_subplot(gs[0,0]);section(cad,parts);coils(cad,curves,radius=.6)
cad.set(xlim=(-7.5,7.5),ylim=(-1,11),title='A. Actual CAD / initial liquid')
cad.annotate('feed wire',xy=(0,8),xytext=(-7,10),fontsize=9,arrowprops={'arrowstyle':'-'})
cad.annotate('free surface at 5.6 mm',xy=(-2,5.6),xytext=(-7,1.2),fontsize=9,arrowprops={'arrowstyle':'-'})
cad.annotate('0.5 mm outlet',xy=(0,0),xytext=(1,-.7),fontsize=9,arrowprops={'arrowstyle':'-'})
planes=[]
for step in [300,1800]:
    path=RUN/f'frame-{step:08d}';m=json.loads((path/'metadata.json').read_text())
    nx,ny,nz=m['dims'];f=read_native_fields(path/'fluid.bin',(nz,ny,nx,16))
    s=np.array(f[:,(ny-1)//2:ny//2+1,:,:].mean(axis=1));del f
    h=m['cell_m']*1000;x,_,z=np.array(m['origin_m'])*1000
    xx=x+(np.arange(nx)+.5)*h;zz=z+(np.arange(nz)+.5)*h
    planes.append((m,s,xx,zz,[x,x+nx*h,z,z+nz*h]))
vmax=max(float(np.linalg.norm(s[:,:,:3],axis=2)[s[:,:,14]>.5].max()) for _,s,_,_,_ in planes)
for index,(m,s,x,z,extent) in enumerate(planes,1):
    ax=fig.add_subplot(gs[0,index]);alpha=s[:,:,14];opened=s[:,:,12]
    speed=np.linalg.norm(s[:,:,:3],axis=2)
    pic=ax.imshow(np.ma.masked_where((alpha<.5)|(opened<.5),speed),origin='lower',extent=extent,
                  cmap='viridis',vmin=0,vmax=vmax,interpolation='nearest')
    rgba=np.zeros((*opened.shape,4));rgba[:,:,:3]=.68;rgba[:,:,3]=1-np.clip(opened,0,1)
    ax.imshow(rgba,origin='lower',extent=extent,interpolation='nearest')
    ax.contour(x,z,alpha,levels=[.5],colors=['#184d70'],linewidths=.9)
    ax.axhline(0,color='#d17a2b',ls=':',lw=.8)
    ax.set(xlim=(-3.8,3.8),ylim=(-.8,6.7),aspect='equal',xlabel='x (mm)',ylabel='z (mm)',
           title=f'{"B" if index==1 else "C"}. Saved fluid at {m["time_s"]*1e3:.2f} ms')
fig.colorbar(pic,ax=fig.axes[1:3],shrink=.7,label='Liquid speed (m/s)',pad=.035)
ax=fig.add_subplot(gs[1,:]);rows=list(csv.DictReader((RUN/'run-diagnostics.csv').open()))
t=np.array([float(r['time_s']) for r in rows]);B=np.array([float(r['max_B_T']) for r in rows])
ax.plot(samples[:,0]*1e6,samples[:,1],color='#287747',lw=2,label='Prescribed coil current')
ax.set(xlim=(0,160),xlabel='Time (µs)',ylabel='Coil current (A)',ylim=(0,275))
ax.tick_params(axis='y',labelcolor='#287747');ax.grid(alpha=.2)
ax2=ax.twinx();ax2.plot(t*1e6,B,color='#6658a6',lw=2,label='Computed domain maximum |B|')
ax2.set_ylabel('Computed maximum |B| (T)',color='#6658a6')
ax2.tick_params(axis='y',labelcolor='#6658a6')
ax.axvline(150,color='#64748b',ls=':',lw=1)
ax.text(149,265,'first saved fluid view →',ha='right',va='top',fontsize=9)
ax.legend(loc='upper right',bbox_to_anchor=(.67,.96),frameon=False,fontsize=9)
ax2.legend(loc='upper right',bbox_to_anchor=(.67,.78),frameon=False,fontsize=9)
fig.suptitle('Selected single-coil run: lower-fill custom chamber — no detached drop',fontsize=17,y=.965)
fig.text(.5,.91,'250 A peak · 10 µs rise / 100 µs fall · completed EM + fluid/interface simulation through 2.1 ms',ha='center',fontsize=12)
fig.text(.5,.10,'A: stainless grey, initial solder blue, copper winding orange (nominal wire envelope). B–C: saved native fields, not CAD liquid.\n'
         'Grey = impermeable solid; blue line = liquid fraction 0.5; colour = liquid speed. Two central y-cell layers averaged.\n'
         '200 µm grid: 2.5 cells across the outlet. The 110 µs pulse ends before the first saved fluid frame at 150 µs.',ha='center',fontsize=10)
fig.text(.5,.035,'Selected for the clearest final interface response among the matched single-pulse coil studies, not successful jetting.\n'
         'Final driven/control phase redistribution difference: 290 nL; velocity RMS difference: 0.00375 m/s. No new simulation was run.',ha='center',fontsize=10)
fig.savefig(OUT,dpi=170)
plt.close(fig)
print(OUT)
