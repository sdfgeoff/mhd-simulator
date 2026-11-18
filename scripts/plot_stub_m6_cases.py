"""Actual short-nozzle CAD sections and separate steady heat-path comparison."""
import json
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Patch
from draw_hardware_layouts import COLORS,load,section,label
from make_hardware import heater
from make_v6_conduction import as_solid

root=Path('artifacts/hardware-research')
record=json.loads(Path('hardware/m6-stub-cold-screening.json').read_text())
fig,axs=plt.subplots(1,3,figsize=(14,7))
fig.subplots_adjust(left=.055,right=.98,top=.82,bottom=.19,wspace=.28)
for ax,name,plane in zip(axs,['stub3p5','stub4','stub4'],['yz','yz','xz']):
    case=next(c for c in record['cases'] if c['label']==name)
    h=case['heater_start_mm'];parts,_=load('m6-stub-v1/'+name)
    section(ax,parts+[('Reference heater',[],as_solid(heater(h,h+11.5)))],plane)
    ax.set(xlim=(-12,12) if plane=='yz' else (-6,6),ylim=(-3.2,16.5),
           title=f"{case['overall_length_mm']:g} mm overall — {plane.upper()} section\n{case['magnet_projection_below_exit_mm']:.2f} mm magnet projection")
    ax.axhline(0,color='#27735b',ls=':',lw=.9)
    if plane=='yz':
        ax.annotate('',xy=(10,0),xytext=(10,case['magnet_bottom_mm']),arrowprops={'arrowstyle':'<->','color':'#32678b'})
        label(ax,'Pool surface inside\nheated barrel',(0,case['pool_level_mm']),(-11,11.2))
        label(ax,'3 mm pole gap',(-1.5,1.1),(-11,4.5))
    else:
        label(ax,'M6 × 1 envelope\n6 mm engagement',(2.9,7.8),(-5.5,13.7))
        label(ax,'2 mm lower bore',(.95,2),(-5.5,6))
        label(ax,'External copper contacts',(3,.3),(-5.5,-2.4))
fig.suptitle('Short, wider nozzle: move the melt pool into the heater\nActual CAD sections; same 0.4 mm outlet / 0.7 mm tip boss',fontsize=15)
fig.legend(handles=[Patch(facecolor=COLORS[k],label=t) for k,t in [('heater','Reference heater'),('steel','Stainless nozzle'),('liquid','Solder'),('magnet','SmCo bars'),('copper','Copper contacts')]],loc='lower center',bbox_to_anchor=(.5,.09),ncol=5,frameon=False)
fig.text(.5,.065,'Four stock 25 × 5 × 5 mm magnet bars total. Flat-sided lower body: 6 mm X width × 2.8 mm Y thickness.',ha='center',fontsize=10)
fig.text(.5,.03,'Concept geometry: thread helix, tool radii, mounts and tolerances omitted. No machining, thermal-cycle or jetting qualification.',ha='center',fontsize=9)
fig.savefig(root/'diagrams/m6-short-nozzle-layouts.png',dpi=180)

thermal=json.loads(Path('hardware/heater-pool-conductance-screening.json').read_text())
fig,ax=plt.subplots(figsize=(8,4.8));fig.subplots_adjust(left=.13,right=.97,top=.84,bottom=.3)
values=[thermal['cases'][i]['thermal_resistance_K_W'] for i in [0,1,3]]
bars=ax.bar(['Long body\nlow pool','Long body\nraised pool','Short body\nraised pool'],values,color=['#ad6965','#749cae','#4e8993'])
ax.bar_label(bars,labels=[f'{v:.1f} K/W' for v in values],padding=4)
ax.set(ylabel='Heater-to-isothermal-pool resistance (K/W)',ylim=(0,43),title='The pool location dominates this simplified heat path')
ax.spines[['right','top']].set_visible(False)
fig.text(.5,.14,'50 µm native steady-conduction screen; stainless k = 15 W/(m K).',ha='center',fontsize=10)
fig.text(.5,.09,'Perfect heater contact and an isothermal pool; no feeding, losses, latent heat or transient prediction.',ha='center',fontsize=9)
fig.text(.5,.04,'For scale: melting room-temperature feed for 0.5 mm drops at 200 Hz needs roughly 10 W, before losses.',ha='center',fontsize=9)
fig.savefig(root/'diagrams/m6-heater-pool-conductance.png',dpi=180)
