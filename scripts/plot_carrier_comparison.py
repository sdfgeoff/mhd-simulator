"""Plot actual carrier CAD and completed cold observations; no jetting claim."""
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

from draw_hardware_layouts import load, section, coils

r=Path('artifacts/hardware-research')
d=json.loads((r/'v6-custom-carrier-completed-comparison.json').read_text())
assert d['actual_initial_and_final_metadata_settings_match']
fig,axes=plt.subplots(2,2,figsize=(11,9),layout='constrained',height_ratios=[1.6,1])
for ax,name,title in zip(axes[0],['v6-carrier-matched-v2/v6-carrier','foil-carrier-z6-base-h020'],['V6-like: 3.2 mm inlet / 0.9 mm exit land','Custom: 6 mm chamber / 0.15 mm exit land']):
    parts,curves=load(name)
    section(ax,parts);coils(ax,curves,radius=.6)
    for curve in curves:
        for a,b in zip(curve,curve[1:]):
            if abs(a[1])<1e-5 and abs(b[1])<1e-5:
                ax.plot([a[0],b[0]],[a[2],b[2]],color='#ad6b36',lw=1.5)
    ax.axhline(0,color='darkorange',ls=':',lw=1)
    ax.set(xlim=(-10.6,10.6),ylim=(-2.2,14),title=title)
labels=['V6-like','Custom']
metrics=[x['metrics'] for x in d['runs']]
forces=[-m['mean_roi_force_z_N']*1000 for m in metrics]
axes[1,0].bar(labels,forces,color=['#667b91','#268eb8'])
axes[1,0].set(ylabel='Mean downward liquid force (mN)',title='ROI z = 0–9 mm; not outlet pressure')
for i,v in enumerate(forces):axes[1,0].text(i,v+.5,f'{v:.2f}',ha='center')
liquid=[m['mean_liquid_joule_power_W'] for m in metrics]
solid=[m['mean_solid_joule_power_W'] for m in metrics]
axes[1,1].bar(labels,liquid,label='Liquid',color='#268eb8')
axes[1,1].bar(labels,solid,bottom=liquid,label='Nozzle + feed wire',color='#aab7c3')
axes[1,1].set(ylabel='Mean Joule power during carrier (W)',title='Metal only; excludes winding and inverter')
axes[1,1].legend(frameon=False,fontsize=9)
for ax in axes[1]:ax.spines[['right','top']].set_visible(False);ax.grid(axis='y',alpha=.2);ax.set_axisbelow(True)
fig.suptitle('Matched single-coil cold screen · 250 A peak · 50 kHz · averages 80–120 µs\nSame winding, current, domain and materials; 200 µm grid. No heater or droplet prediction.',fontsize=12)
fig.savefig(r/'diagrams/carrier-v6-custom-comparison.png',dpi=170)
