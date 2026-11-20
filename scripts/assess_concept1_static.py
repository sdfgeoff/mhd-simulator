"""Read saved complete Concept1 assembly stationaryfields; no PDE solve."""
import json,hashlib
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.colors import LogNorm
root=Path('artifacts/hardware-research');out=Path('hardware/concept1-developed-review');state=json.loads((root/'concept1-developed-v1-screens-state.json').read_text());assert state['stage']=='complete_needs_assessment'
for p,digest in state['hashes'].items():assert hashlib.sha256(Path(p).read_bytes()).hexdigest()==digest,p
p=json.loads(Path('hardware/designs/concept1-developed-v1/assembly-h0100.mhd.json').read_text());mats={m['id']:m for m in p['materials']};cases=[]
# Same250us forward +83.33us quarter-current retract pulse as the planned flow control.
rise=20e-6;equivalent_peak_time=(250e-6-4*rise/3)+.25**2*(250e-6/3-4*rise/3)
for label in ['ideal','contact-low','contact-high']:
 run=root/('concept1-developed-v1-electrical-'+label);r=json.loads((run/'report.json').read_text());g=json.loads((run/'grid.json').read_text());nx,ny,nz=g['dims'];h=g['cell_m'];origin=np.array(g['origin_m']);f=np.memmap(run/'em.bin',dtype='<f4',mode='r',shape=(nz,ny,nx,16));ids=np.memmap(run/'material-ids.bin',dtype='<u4',mode='r',shape=(nz,ny,nx));sums={};maxheat={}
 assert r['liquid_force_N'][2]<0,'Wrong polarity';assert r['boundary_current_defect']<=1e-5 and abs(r['power_relative_defect'])<=1e-4
 for k in range(nz):
  for material in [4,14,16,17,20]:
   values=f[k,:,:,11][ids[k]==material];sums[str(material)]=sums.get(str(material),0.)+float(values.sum(dtype=np.float64)*h**3)
   if len(values):maxheat[str(material)]=max(maxheat.get(str(material),0.),float(values.max()))
 assert abs(sum(sums.values())/r['joule_power_W']-1)<1e-4
 adiabatic={}
 for key,q in maxheat.items():
  mat=mats[int(key)];rho=mat.get('density_kg_m3');cp=mat['thermal'].get('specific_heat_j_kg_k') or mat['thermal'].get('heat_capacity_j_kg_k')
  if rho and cp:adiabatic[key]=q*equivalent_peak_time/(rho*cp)
 cases.append({'label':label,'native':r,'Joule_by_material_W':sums,'peak_cell_q_W_m3':maxheat,'static_pulse_cell_adiabatic_delta_K_estimate':adiabatic})
 if label=='ideal':
  ix=int(np.argmin(abs(origin[0]+(np.arange(nx)+.5)*h)));plane=np.array(f[:,:,ix,:]);mid=np.array(ids[:,:,ix]);y=(origin[1]+(np.arange(ny)+.5)*h)*1000;z=(origin[2]+(np.arange(nz)+.5)*h)*1000;extent=[origin[1]*1000,(origin[1]+ny*h)*1000,origin[2]*1000,(origin[2]+nz*h)*1000]
 del f,ids
r=json.loads((root/'concept1-developed-v1-thermal-wall/report.json').read_text());thermal=np.memmap(root/'concept1-developed-v1-thermal-wall/potential.bin',dtype='<f4',mode='r',shape=(nz,ny,nx));ids=np.memmap(root/'concept1-developed-v1-thermal-wall/material-ids.bin',dtype='<u4',mode='r',shape=(nz,ny,nx));assert r['return_relative_error']<=1e-5 and abs(r['energy_identity_relative_error'])<=1e-4
Y=np.broadcast_to(y[:,None],(ny,nx));lo=1.;hi=0.
for k in range(nz):
 excluded=(np.hypot(Y-5.5,z[k]-9.7)<3.05)|(np.hypot(Y+5.5,z[k]-9.7)<1.55);mask=np.isin(ids[k],[4,14,16,17,19,20,21])&~excluded;values=thermal[k][mask]+.5
 if len(values):lo=min(lo,float(values.min()));hi=max(hi,float(values.max()))
assert lo>=-2e-5 and hi<=1+2e-5
result={'status':'Completed100A fullassemblyelectrical/contactparametercases and idealwallconductance; no fluid resultyet','cases':cases,'thermal':r,'normalized_temperature_range':[lo,hi],'all_frozen_hashes_match_at_assessment':True,'pulse_static_estimate':{'positive_us':250,'retract_fraction':.25,'retract_us':250/3,'rise_us':20,'equivalent_100A_time_s':equivalent_peak_time,'limits':'Local0.1mmcellaveraged static q*I²dose/(rhoCp), no heatspreading, no movingcurrent redistribution or microncontactflash. Not a certifiedpeak-temperaturebound or safeoperatingrating.'},'limits':['FieldassumesBr.95T5mmcubes; fixedtemperatureσ andk; no self-field/reaction.','Contactvalues0/1e-10/1e-9ohm*m² are independent scenarios, not measured joints.','Thermalwholewallboundaries notfiniteheaterpower, ambientloss, feedload, magnettemperature or transientJouleheating.']};(out/'static-results.json').write_text(json.dumps(result,indent=2)+'\n')
fig,axes=plt.subplots(1,4,figsize=(18,10));fig.subplots_adjust(left=.04,right=.97,top=.85,bottom=.23,wspace=.45)
conduct=np.isin(mid,[4,14,16,17,20]);X,Z=np.meshgrid(y,z)
plots=[(np.linalg.norm(plane[:,:,4:7],axis=2)/1e6,conduct,'viridis',LogNorm(.1,1500),'Current magnitude (A/mm²)'),(plane[:,:,11]/1e9,conduct,'magma',LogNorm(1e-4,1e3),'Instantaneous q (W/mm³)'),(np.array(thermal[:,:,ix])+.5,np.isin(mid,[4,14,16,17,19,20,21])&~((np.hypot(X-5.5,Z-9.7)<3.05)|(np.hypot(X+5.5,Z-9.7)<1.55)),'inferno',None,'Normalized T (unit conductance only)'),(plane[:,:,10]*1e-6,mid==4,'RdBu_r',None,'Liquid fz (mN/mm³)')]
for j,(v,mask,cmap,norm,label) in enumerate(plots):
 ax=axes[j];opts={'norm':norm} if norm else ({'vmin':0,'vmax':1} if j==2 else {'vmin':-12,'vmax':12});im=ax.imshow(np.ma.masked_where(~mask,v),origin='lower',extent=extent,cmap=cmap,interpolation='nearest',**opts);ax.contour(y,z,mask.astype(float),levels=[.5],colors='#78858c',linewidths=.4);ax.set(xlim=(-2,2) if j in [1,3] else (-10.5,10.5),ylim=(-.7,4.2) if j in [1,3] else (-3.5,17.5),xlabel='y (mm)',ylabel='z (mm)',title=['Whole assembly current','Tip heating (zoom)','Ideal heater wall to pool','Liquid vertical force (zoom)'][j]);fig.colorbar(im,ax=ax,shrink=.7,label=label)
fig.suptitle('Developed Concept1 — complete stationary assembly, 100 A',fontsize=18,y=.97)
fig.text(.5,.91,f'Idealcontact: {cases[0]["native"]["terminal_voltage_V"]:.3f} V / {cases[0]["native"]["joule_power_W"]:.2f} W instantaneous | downward liquid force {-1000*cases[0]["native"]["liquid_force_N"][2]:.2f} mN | idealwall R = {r["thermal_resistance_K_W"]:.2f} K/W',ha='center',fontsize=11)
fig.text(.5,.13,'Actual saved GPU fields, 100 µm cells. Grey outlines show sectioned conducting/thermal geometry; temperature is normalized, not operating°C.\nTip heat / liquid force panels are enlarged.0.4mm outlet has only 4 cells across: these stationary results do not qualify droplet breakup.\nDeclared 5 mm cube SmCo Br 0.95 T, full steel block/clamps included. Realcontacts, finiteheater/coldzones, ambientloss, self-field and magnetheatbalance remain to be qualified.',ha='center',fontsize=10,linespacing=1.7)
fig.savefig(out/'static-results.png',dpi=160);plt.close(fig)
print(json.dumps({'case_metrics':[{'label':c['label'],'V':c['native']['terminal_voltage_V'],'W':c['native']['joule_power_W'],'Fz':c['native']['liquid_force_N'][2],'adiabatic':c['static_pulse_cell_adiabatic_delta_K_estimate']} for c in cases],'R_K_W':r['thermal_resistance_K_W']},indent=2))
