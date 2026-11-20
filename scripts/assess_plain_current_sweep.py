"""Assess completed current-amplitude or outlet-land controls; --available labels unfinished cases explicitly."""
import argparse,csv,hashlib,json
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from native_field_io import read_native_fields
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--available',action='store_true');p.add_argument('--study',choices=['current','land'],default='current');a=p.parse_args()
root=Path('artifacts/hardware-research');out=Path('hardware/plain-current-review' if a.study=='current' else 'hardware/outlet-land-review');out.mkdir(exist_ok=True)
inputs=[(110,root/'plain-bore-v4-2mm-flow110-r25-h0050'),*[(i,root/f'plain-bore-low-current-v1-2mm-flow{i}-r25-h0050') for i in [90,75]]]
if a.study=='land':
    inputs=[(.6,root/'plain-bore-low-current-v1-2mm-flow90-r25-h0050'),(.9,root/'plain-land-flow-v1-090-flow90-r25-h0050'),(1.2,root/'plain-land-flow-v1-120-flow90-r25-h0050')]
cases=[];pending=[];settings=None
for value,run in inputs:
    current=value if a.study=='current' else 90
    file=run/'run-status.json'
    if not file.exists() or json.loads(file.read_text())['status']!='complete':
        pending.append(value)
        if not a.available:raise ValueError('Incomplete sweep; use --available for explicitly partial comparison')
        continue
    s=json.loads(file.read_text());assert s['accepted_steps']==2400 and abs(s['time_s']-.0036)<1e-12
    keys=['grid','positive_duration_s','rise_s','retract_fraction','retract_delay_s','contact_resistivity_ohm_m2','timestep_s','tail_timestep_s','frequency_hz','uniform_acceleration_m_s2','terminal_boundary','charge_solver']
    if a.study=='land':keys.append('peak_A')
    config={k:s[k] for k in keys}
    if settings is not None:assert config==settings,'Unexpected configuration change'
    settings=config
    rows=list(csv.DictReader((run/'current-diagnostics.csv').open()));assert len(rows)==2400 and {int(x['step']) for x in rows}==set(range(1,2401))
    f=json.loads((run/'flow-analysis.json').read_text());assert f['frames'][-1]['step']==2400
    dose=sum(float(x['command_A'])**2*float(x['timestep_s']) for x in rows)
    defect=(s['lagged_electrical_work_J']-s['joule_generated_J']-s['lagged_mechanical_work_J'])/max(abs(s['lagged_electrical_work_J']),abs(s['joule_generated_J']),1e-30)
    c={'peak_A':current,'source':str(run),'final':f['frames'][-1],'maximum_absolute_relative_volume_change':max(abs(x['relative_liquid_volume_change']) for x in f['frames']),'any_saved_boundary_contact':any(x['liquid_touches_domain_boundary_alpha01'] for x in f['frames']),'head_peak_voltage_V':max(abs(float(x['voltage_V'])) for x in rows),'command_I2dt_A2s':dose,'RMS_A_at_assumed200Hz':float(np.sqrt(200*dose)),'joule_generated_J':s['joule_generated_J'],'energy_identity_relative_defect':defect,'current_diagnostics_sha256':hashlib.sha256((run/'current-diagnostics.csv').read_bytes()).hexdigest()}
    if a.study=='land':c['land_mm']=value
    cases.append((c,rows))
report={'status':'Complete current sweep' if not pending else 'Partial sweep: only terminal complete cases assessed','pending_currents_A':pending,'common_settings':settings,'cases':[c for c,_ in cases],'limits':['Single pulses at one grid and timestep; no repeatability or thermal qualification.','Ideal contacts, fixed properties, static imposed magnetic field, no heater block or current self-field.','Threshold-defined equivalent diameters are not grid-independent measured drop sizes.']}
if a.study=='land':
    report['status']='Complete outlet-land sweep' if not pending else report['status'];report['pending_lands_mm']=report.pop('pending_currents_A')
(out/'results.json').write_text(json.dumps(report,indent=2)+'\n')
fig,axes=plt.subplots(len(cases),3,figsize=(12,5*len(cases)),squeeze=False);fig.subplots_adjust(left=.065,right=.94,top=.89,bottom=.19,wspace=.43,hspace=.4)
for row,(c,rows) in enumerate(cases):
    fieldaxes=[];label=f'{c["peak_A"]} A' if a.study=='current' else f'{c["land_mm"]:.1f} mm land'
    for col,step in enumerate([600,2400]):
        ax=axes[row,col];fieldaxes.append(ax);d=Path(c['source'])/f'frame-{step:08d}';m=json.loads((d/'metadata.json').read_text());nx,ny,nz=m['dims'];f=read_native_fields(d/'fluid.bin',(nz,ny,nx,16));s=np.array(f[:,(ny-1)//2:ny//2+1,:,:].mean(axis=1));del f
        h=m['cell_m']*1000;x0,_,z0=np.array(m['origin_m'])*1000;extent=[x0,x0+nx*h,z0,z0+nz*h];alpha=s[:,:,14];op=s[:,:,12]
        im=ax.imshow(np.ma.masked_where((alpha<.5)|(op<.5),s[:,:,2]),origin='lower',extent=extent,cmap='RdBu_r',vmin=-1.2,vmax=1.2,interpolation='nearest')
        rgba=np.zeros((*alpha.shape,4));rgba[:,:,:3]=.7;rgba[:,:,3]=1-np.clip(op,0,1);ax.imshow(rgba,origin='lower',extent=extent,interpolation='nearest');ax.contour(x0+(np.arange(nx)+.5)*h,z0+(np.arange(nz)+.5)*h,alpha,levels=[.5],colors=['#253e50'],linewidths=.7)
        ax.set(xlim=(-1,1),ylim=(-4,1.5),aspect='equal',xlabel='x (mm)',ylabel='z (mm)',title=f'{label}: {m["time_s"]*1000:.1f} ms')
    fig.colorbar(im,ax=fieldaxes,shrink=.8,pad=.03,label='Vertical velocity (m/s); blue = down')
    ax=axes[row,2];t=[float(x['time_s'])*1e6 for x in rows];ax.plot(t,[float(x['current_A']) for x in rows],color='#287747');ax.set(xlim=(0,500),ylim=(-35,120),xlabel='Time (µs)',ylabel='Current (A)',title='Solved terminal current');ax.grid(alpha=.2)
    regions=c['final']['connectivity_thresholds']['0.5'];text=f'{len(regions)} detached region(s) at fraction 0.5\n'
    text+='\n'.join(f'{r["equivalent_diameter_mm"]:.3f} mm; vz {r["velocity_m_s"][2]:+.3f} m/s' for r in regions)
    text+=f'\nHead peak {c["head_peak_voltage_V"]:.3f} V\nRMS at assumed 200 Hz: {c["RMS_A_at_assumed200Hz"]:.2f} A'
    pos=ax.get_position();ax.set_position([pos.x0,pos.y0+pos.height*.30,pos.width,pos.height*.70])
    ax.text(.5,-.43,text,transform=ax.transAxes,ha='center',va='top',fontsize=9)
fig.suptitle('Lower-current controls — same corrected 2 mm nozzle' if a.study=='current' else 'Outlet-length controls — same exterior and 90 A drive',fontsize=16,y=.98)
fig.text(.5,.935,('0.4 mm outlet / 0.6 mm land' if a.study=='current' else '0.4 mm outlet / 2 mm chamber; internal taper moves upward')+' · same pulse timing and reverse fraction · 50 µm / 1.5 µs · final time 3.6 ms',ha='center',fontsize=10)
fig.text(.5,.015,('Pending: '+', '.join((f'{i} A' if a.study=='current' else f'{i} mm land') for i in pending)+'. Only completed runs shown.\n' if pending else '')+'Blue means downward; grey is native solid mask; outline is liquid fraction 0.5. Sections average central two y layers.\nHead voltage excludes external wiring/switching inductance. Ideal contacts; fixed properties; heater excluded.\nOne pulse per case; assumed 200 Hz RMS is current-dose arithmetic, not repeated-operation qualification.',ha='center',va='bottom',fontsize=9)
fig.savefig(out/'results-comparison.png',dpi=160);plt.close(fig)
print(json.dumps([{k:c[k] for k in ['peak_A','head_peak_voltage_V','RMS_A_at_assumed200Hz','energy_identity_relative_defect']} for c,_ in cases],indent=2))
