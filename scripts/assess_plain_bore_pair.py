"""Assess only completed V4 exports and render the requested CAD/fluid/current comparison."""
import csv
import hashlib
import json
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from draw_hardware_layouts import load, section
from native_field_io import read_native_fields

ROOT=Path('artifacts/hardware-research')
OUT=Path('hardware/plain-bore-review')
PLAN=OUT/'simulation-plan-mesh-v4.json'
plan=json.loads(PLAN.read_text())
cases=[]
for job in plan['jobs']:
    run=Path(job['output']);status=json.loads((run/'run-status.json').read_text())
    if status['status']!='complete' or status['accepted_steps']!=2400 or abs(status['time_s']-.0036)>1e-12:
        raise ValueError('Both completed3.6ms cases required; no partial result is promoted')
    flow=json.loads((run/'flow-analysis.json').read_text());rows=list(csv.DictReader((run/'current-diagnostics.csv').open()))
    if len(rows)!=2400 or {int(r['step']) for r in rows}!=set(range(1,2401)):
        raise ValueError('Incomplete accepted-current diagnostics')
    dose=sum(float(r['command_A'])**2*float(r['timestep_s']) for r in rows)
    energy=(status['lagged_electrical_work_J']-status['joule_generated_J']-status['lagged_mechanical_work_J'])
    energy/=max(abs(status['lagged_electrical_work_J']),abs(status['joule_generated_J']),1e-30)
    with (run/'frame-00002400/fluid.bin').open('rb') as f:field_hash=hashlib.file_digest(f,'sha256').hexdigest()
    project=json.loads(Path(job['command'][1]).read_text())
    case={'bore_mm':job['bore_mm'],'source':str(run),'status':status,'final':flow['frames'][-1],
          'maximum_absolute_relative_liquid_volume_change':max(abs(r['relative_liquid_volume_change']) for r in flow['frames']),
          'any_saved_liquid_boundary_contact':any(r['liquid_touches_domain_boundary_alpha01'] for r in flow['frames']),
          'accepted_command_I2dt_A2s':dose,'RMS_A_at_assumed200Hz':float(np.sqrt(200*dose)),
          'electrical_energy_identity_relative_defect':energy,'final_fluid_sha256':field_hash,
          'current_diagnostics_sha256':hashlib.sha256((run/'current-diagnostics.csv').read_bytes()).hexdigest()}
    cases.append((case,project,rows))
keys=['grid','peak_A','positive_duration_s','rise_s','retract_fraction','retract_delay_s','contact_resistivity_ohm_m2',
      'timestep_s','tail_timestep_s','frequency_hz','uniform_acceleration_m_s2','terminal_boundary','charge_solver']
for key in keys:
    if cases[0][0]['status'][key]!=cases[1][0]['status'][key]:raise ValueError('Unexpected run-setting difference: '+key)
report={'status':'Both corrected-mesh cases complete3.6ms; exploratory single-pulse results, not convergence or repeated jetting',
        'plan':str(PLAN),'cases':[c for c,_,_ in cases],
        'limits':['Custom M6-compatible parts, not stock-subtractive conversions.',
                  'Same corrected exterior, pole gap4mm, fill height6mm, physical properties and pulse; liquid volumes differ.',
                  'Eight common nominal-taper cells have differing classifications, recorded in mesh-repair.json.',
                  'Ideal electrical contact, static imposed magnet field, no current self-field, thermal evolution, oxide, calibrated wetting or feeding.',
                  'One50um grid and one pulse do not establish grid convergence, clean repeatability or200Hz thermal equilibrium.']}
(OUT/'results.json').write_text(json.dumps(report,indent=2)+'\n')
fig=plt.figure(figsize=(16,15));gs=fig.add_gridspec(3,4,height_ratios=[2.2,2.2,1],left=.055,right=.94,top=.90,bottom=.15,wspace=.38,hspace=.38)
for row,(case,project,records) in enumerate(cases):
    parts,_=load(project['geometry'])
    ax=fig.add_subplot(gs[row,0]);section(ax,parts,'xz');ax.set(xlim=(-1.8,1.8),ylim=(-.4,6.5),title=f'{case["bore_mm"]} mm: contact plane')
    ax=fig.add_subplot(gs[row,1]);section(ax,parts,'yz');ax.set(xlim=(-13,13),ylim=(-3,12),title='Magnet plane')
    fieldaxes=[]
    for col,step in [(2,600),(3,2400)]:
        ax=fig.add_subplot(gs[row,col]);fieldaxes.append(ax)
        p=Path(case['source'])/f'frame-{step:08d}';m=json.loads((p/'metadata.json').read_text());nx,ny,nz=m['dims']
        f=read_native_fields(p/'fluid.bin',(nz,ny,nx,16));s=np.array(f[:,(ny-1)//2:ny//2+1,:,:].mean(axis=1));del f
        h=m['cell_m']*1000;x0,_,z0=np.array(m['origin_m'])*1000;x=x0+(np.arange(nx)+.5)*h;z=z0+(np.arange(nz)+.5)*h
        alpha=s[:,:,14];op=s[:,:,12];extent=[x0,x0+nx*h,z0,z0+nz*h]
        im=ax.imshow(np.ma.masked_where((alpha<.5)|(op<.5),s[:,:,2]),origin='lower',extent=extent,
                     cmap='RdBu_r',vmin=-1.2,vmax=1.2,interpolation='nearest')
        rgba=np.zeros((*alpha.shape,4));rgba[:,:,:3]=.7;rgba[:,:,3]=1-np.clip(op,0,1)
        ax.imshow(rgba,origin='lower',extent=extent,interpolation='nearest')
        ax.contour(x,z,alpha,levels=[.5],colors=['#253e50'],linewidths=.7);ax.axhline(0,color='#d17a2b',ls=':',lw=.7)
        ax.set(xlim=(-1,1),ylim=(-5,1.8),aspect='equal',xlabel='x (mm)',ylabel='z (mm)',title=f'Saved fluid: {m["time_s"]*1000:.1f} ms')
    fig.colorbar(im,ax=fieldaxes,shrink=.8,pad=.035,label='Vertical velocity (m/s); blue = down')
    regions=case['final']['connectivity_thresholds']['0.5']
    summary=f'{len(regions)} detached region(s) at fraction0.5'
    if regions:
        lead=max(regions,key=lambda r:r['volume_nl']);summary+=f'; largest {lead["equivalent_diameter_mm"]:.3f} mm, downward speed {-lead["velocity_m_s"][2]:.3f} m/s'
    fig.text(.50,min(a.get_position().y0 for a in fieldaxes)-.045,summary,ha='center',fontsize=10)
ax=fig.add_subplot(gs[2,:])
for (case,_,rows),color in zip(cases,['#287747','#b06a2c']):
    t=np.array([float(r['time_s'])*1e6 for r in rows]);ax.plot(t,[float(r['current_A']) for r in rows],color=color,lw=1.5,label=f'{case["bore_mm"]} mm solved terminal current')
ax.set(xlim=(0,500),xlabel='Time (µs)',ylabel='Current (A)',title='Common drive: +110 A forward, 25 µs gap, −27.5 A retrieval (curves overlap)')
ax.grid(alpha=.18);ax.axhline(0,color='#64748b',lw=.7);ax.legend(loc='upper right',frameon=False,fontsize=9)
fig.suptitle('Straight 2 mm versus 3 mm chambers — completed corrected-mesh simulations',fontsize=17,y=.975)
fig.text(.5,.94,'Common exterior and magnets · 0.4 mm outlet · 50 µm cells · 1.5 µs steps · one pulse through 3.6 ms',ha='center',fontsize=11)
fig.text(.5,.08,'CAD: solder blue, copper orange, magnets light blue, metal grey. Heater shown in CAD but excluded from flow.\n'
         'Fluid: signed vertical velocity; blue is downward. Grey is native solid mask; outline is liquid fraction0.5.\n'
         'Fluid sections average the two central y-cell layers. Different CAD/fluid panel scales; no interpolation in the native fields.',ha='center',fontsize=10)
fig.text(.5,.025,'Ideal electrical interfaces and fixed material properties. Single-pulse outcomes are not repeated or grid-converged jetting.\n'
         'Only the final corrected-mesh pair is compared here; earlier failed/older-mesh attempts remain separate.',ha='center',fontsize=10)
fig.savefig(OUT/'results-comparison.png',dpi=170);plt.close(fig)
print(json.dumps([{'bore_mm':c['bore_mm'],'regions':c['final']['connectivity_thresholds']['0.5']} for c,_,_ in cases],indent=2))
