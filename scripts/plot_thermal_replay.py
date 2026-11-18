"""Plot accepted native temperature diagnostics; current path remains fixed."""
import csv
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
r=Path('artifacts/hardware-research')
fig,axes=plt.subplots(1,2,figsize=(11,4.7),layout='constrained')
for ax,title,cases in zip(axes,['Machined tip + near-flush poles','Wider contacts on unmodified cone'],[
 [('nearflush-thermal-diffusion300-h010','100 µm'),('nearflush-thermal-tipROI300-h0050','50 µm thermal window')],
 [('wide-thermal-diffusion300-h010','100 µm'),('wide-thermal-diffusion300-h0050','50 µm')]]):
 for name,label in cases:
  rows=list(csv.DictReader((r/name/'diagnostics.csv').open()))
  ax.plot([float(x['time_s'])*1e6 for x in rows],[float(x['maximum_temperature_C']) for x in rows],label=label)
 ax.set(title=title,xlabel='Time (µs)',ylabel='Maximum model temperature (°C)',xlim=(0,360),ylim=(248,327))
 ax.axvspan(0,250,alpha=.05,color='tab:red');ax.axvspan(275,358.333,alpha=.06,color='tab:blue')
 ax.grid(alpha=.2);ax.legend(frameon=False)
fig.suptitle('Native GPU heat diffusion · fixed accepted cold current path · 300 A / 250 µs\nStarts 250 °C; reverse pulse −75 A. Ideal external contacts; insulated one-shot model. Not a safety limit.',fontsize=11)
fig.savefig(r/'diagrams/thermal-replay-300A.png',dpi=170)
