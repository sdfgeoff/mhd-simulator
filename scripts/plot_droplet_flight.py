"""Plot independent flight-screen outputs; lines connect computed targets."""
import argparse
import json

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('report')
p.add_argument('output')
a=p.parse_args()
d=json.load(open(a.report))
fig,axes=plt.subplots(1,2,figsize=(10,4.5),layout='constrained')
for speed,style in [(.15,'-'),(.4,'--')]:
    c=next(c for c in d['cases'] if c['diameter_mm']==.5 and c['gas']=='air' and c['emissivity']==.8 and c['initial_speed_m_s']==speed)
    axes[0].plot([r['distance_mm'] for r in c['targets']],[r['flight_ms'] for r in c['targets']],style,marker='o',label=f'Release {speed:.2f} m/s')
for gas,color in [('air','tab:blue'),('nitrogen','tab:orange'),('argon','tab:green')]:
    band=[]
    for emissivity in [.1,.8]:
        c=next(c for c in d['cases'] if c['diameter_mm']==.5 and c['initial_speed_m_s']==.15 and c['gas']==gas and c['emissivity']==emissivity)
        x=[r['distance_mm'] for r in c['targets']]
        band.append([r['cooling_k'] for r in c['targets']])
    axes[1].fill_between(x,band[0],band[1],alpha=.15,color=color)
    axes[1].plot(x,np.mean(band,axis=0),marker='o',color=color,label=gas.capitalize())
axes[0].set(ylabel='Flight time (ms)',title='Gravity + drag; still air')
axes[1].set(ylabel='Temperature decrease (K)',title='Release 0.15 m/s; emissivity 0.1–0.8 bands')
for ax in axes:
    ax.set(xlabel='Droplet-centre travel after release (mm)',xlim=(0,10.3),ylim=(0,None))
    ax.grid(alpha=.2);ax.legend(frameon=False)
fig.suptitle('Independent spherical-drop screen · 0.5 mm SnCu · starts 250 °C · surroundings 25 °C\nConvection + radiation; no oxide growth, gas jet, hot-head view factors or impact model',fontsize=11)
fig.savefig(a.output,dpi=170)
