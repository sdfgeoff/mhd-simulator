"""Draw the actual relieved lower-nozzle CAD and reference heater clearance."""
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Patch
from draw_hardware_layouts import COLORS, load, section, label
from make_hardware import heater
from make_v6_conduction import as_solid

root=Path('artifacts/hardware-research')
manifest=json.loads((root/'cases/m6-waist-v1/manifest.json').read_text())
by_name={c['label']:c for c in manifest['cases']}
choices=['ch32-w04','ch20-w02','ch12-w02']
figure,axes=plt.subplots(1,3,figsize=(13,7))
figure.subplots_adjust(top=.8,bottom=.15,left=.055,right=.98,wspace=.24)
reference=as_solid(heater(6.5,18.))
for ax,name in zip(axes,choices):
    case=by_name[name]
    parts,_=load('m6-waist-v1/'+name)
    section(ax,parts+[('Reference aluminium heater',[],reference)],'yz')
    gap=case['pole_gap_mm']
    ax.set(xlim=(-12.5,12.5),ylim=(-1.1,18.8),title=f"Chamber {case['chamber_diameter_mm']:.1f} mm\nPole gap {gap:.1f} mm",xlabel='y (mm)',ylabel='z (mm)')
    ax.axhline(0,color='#27735b',ls=':',lw=.8)
    for sign in [-1,1]:
        ax.vlines(sign*(gap/2+5),.1,5.1,colors='#5b7285',ls='--',lw=.7)
    ax.annotate('',xy=(-gap/2,-.5),xytext=(gap/2,-.5),arrowprops={'arrowstyle':'<->','color':'#32678b'})
    if name=='ch32-w04':
        label(ax,'Reference heater\nunchanged',(6,14),(5.1,17.2))
        label(ax,'M6 outer envelope\nretained',(2.8,9),(-11.5,17.2))
        label(ax,'Open solder pool',(0,3.5),(-11.5,6))
    ax.tick_params(labelsize=8)
figure.suptitle('Relief cuts let the poles approach a narrower lower chamber\nActual CAD sections; same 0.4 mm outlet and 0.7 mm tip boss',fontsize=14)
figure.legend(handles=[Patch(facecolor=COLORS[k],label=label) for k,label in
                       [('heater','Reference heater'),('steel','Stainless nozzle'),('liquid','Solder'),('magnet','SmCo bars')]],
              loc='lower center',bbox_to_anchor=(.5,.065),ncol=4,frameon=False)
figure.text(.5,.04,'Pole bottoms sit 0.1 mm above the outlet. Four 25 × 5 × 5 mm bars total; dashed lines mark abutting stacks.',ha='center',fontsize=9)
figure.text(.5,.015,'Geometry screen only: thread helix, tool radii, tolerances and mounts omitted. No jetting or heater-fit qualification.',ha='center',fontsize=9)
figure.savefig(root/'diagrams/m6-waist-layouts.png',dpi=180)
