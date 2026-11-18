"""Accepted cold comparison of short heater-compatible nozzle concepts."""
import json
from pathlib import Path
import numpy as np
root=Path('artifacts/hardware-research')
manifest=json.loads((root/'cases/m6-stub-v1/manifest.json').read_text())
rows=[]
for c in manifest['cases']:
 run=root/f'm6-stub-v1-{c["label"]}-cold30-h0050'
 r=json.loads((run/'report.json').read_text());g=json.loads((run/'grid.json').read_text());nx,ny,nz=g['dims'];h=g['cell_m']
 if abs(r['power_relative_defect'])>1e-4:raise ValueError('Power balance failed')
 f=np.memmap(run/'em.bin',dtype='<f4',mode='r').reshape(nz,ny,nx,16)
 ids=np.memmap(run/'material-ids.bin',dtype='<u4',mode='r').reshape(nz,ny,nx)
 z=g['origin_m'][2]+(np.arange(nz)+.5)*h
 mask=(ids==4)&(z[:,None,None]>=0)&(z[:,None,None]<.0006)
 force=-float(f[...,10][mask].astype(float).sum()*h**3)
 rows.append({**c,'output':str(run),'cell_um':h*1e6,'outlet_mean_By_T':float(f[...,1][mask].astype(float).mean()),'outlet_downward_force_N_at30A':force,'total_liquid_downward_force_N_at30A':-r['liquid_force_N'][2],'nozzle_and_modeled_copper_R_ohm':r['terminal_voltage_V']/r['current_A'],'power_relative_defect':r['power_relative_defect'],'true_relative_residual':r['true_relative_residual'],'native_liquid_volume_mm3':float((ids==4).sum()*h**3*1e9)})
record={'status':'Completed cold screen only; no short-nozzle jetting result','source':manifest,'cases':rows,'scope':'Same M6x1 threaded length, outlet, tip contacts, source magnetization and gap. Shorter projection below heater shifts magnets downward relative to the outlet; this trades 0.45–2.45 mm nominal magnet projection for a shorter body and stronger local field. Pool levels are 2 mm inside the heated barrel. Cold source material properties and ideal joints; no thermal feedback or moving liquid.'}
with Path('hardware/m6-stub-cold-screening.json').open('x') as output:json.dump(record,output,indent=2);output.write('\n')
for c in rows:print(c['label'],'By',round(c['outlet_mean_By_T'],4),'F mN',round(c['outlet_downward_force_N_at30A']*1000,4),'R mOhm',round(c['nozzle_and_modeled_copper_R_ohm']*1000,4),'projection',c['magnet_projection_below_exit_mm'])
