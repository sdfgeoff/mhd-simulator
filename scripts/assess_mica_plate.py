"""Assess saved ideal-contact assembly fields; no new PDE solve."""
import json,hashlib
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.colors import LogNorm
r=Path('hardware/mica-plate-review');raw=Path('artifacts/hardware-research');cold=raw/'mica-flat-plate-v2-cold100-h0100';thermal=raw/'mica-flat-plate-v2-thermal-active-span-h0100';state=json.loads((raw/'mica-flat-plate-v2-screens-state.json').read_text());assert state['stage']=='complete_needs_assessment'
for p,digest in state['hashes'].items():assert hashlib.sha256(Path(p).read_bytes()).hexdigest()==digest,p
c=json.loads((cold/'report.json').read_text());t=json.loads((thermal/'report.json').read_text());g=json.loads((cold/'grid.json').read_text());assert g==json.loads((thermal/'grid.json').read_text());nx,ny,nz=g['dims'];h=g['cell_m'];origin=np.array(g['origin_m']);x=(origin[0]+(np.arange(nx)+.5)*h)*1000;y=(origin[1]+(np.arange(ny)+.5)*h)*1000;z=(origin[2]+(np.arange(nz)+.5)*h)*1000
f=np.memmap(cold/'em.bin',dtype='<f4',mode='r',shape=(nz,ny,nx,16));ids=np.memmap(cold/'material-ids.bin',dtype='<u4',mode='r',shape=(nz,ny,nx));temp=np.memmap(thermal/'potential.bin',dtype='<f4',mode='r',shape=(nz,ny,nx));assert c['boundary_current_defect']<=1e-5 and abs(c['power_relative_defect'])<=1e-4;assert t['return_relative_error']<=1e-5 and abs(t['energy_identity_relative_error'])<=1e-4
powers={};lo=1.;hi=0.
X,Y=np.meshgrid(x,y)
for k in range(nz):
 for mid in [4,14,16,17,19,20,21,18]:powers[str(mid)]=powers.get(str(mid),0.)+float(f[k,:,:,11][ids[k]==mid].sum(dtype=np.float64)*h**3)
 # Heater and sensor axes X; exclude floating sheaths, preserve all finite solid/material paths.
 excluded=((Y-5)**2+(z[k]-16)**2<1.6**2)|((Y-8.5)**2+(z[k]-19)**2<.6**2)
 mask=np.isin(ids[k],[4,14,16,17,19,20,21])&~excluded;v=temp[k][mask]+.5
 if len(v):lo=min(lo,float(v.min()));hi=max(hi,float(v.max()))
assert lo>=-2e-5 and hi<=1+2e-5
power=sum(powers.values());assert abs(power/c['joule_power_W']-1)<1e-4
ix=int(np.searchsorted(x,0));liq=float(f[:,:,ix,4][ids[:,:,ix]==4].sum(dtype=np.float64)*h*h)
result={'status':'Completed native100A ideal-contact electrical and material-aware nominal-active-span thermal screens; no fluid or operating temperatures','electrical':c,'thermal':t,'power_by_material_W':powers,'CPU_Joule_relative_difference':power/c['joule_power_W']-1,'zero_plane_x_mm':float(x[ix]),'liquid_plane_current_A':liq,'normalized_temperature_range':[lo,hi],'all_frozen_hashes_match':True,'failed_attempt':'V1cold at1e-6 residual failed energy147.7ppm vs100ppm gate; preserved. V2tightened residual only to1e-7 with samebinary/PDE/geometry/ports and unchanged gates.','limits':['Ideal CuSS electrical and thermal contact; contact-resistance sensitivity stillrequired.','Explicit mica.528/Cu394/SS15/sleeve25W/(mK); unsourced hardware geometry/constants.','Thermal selectedwall at1K relative to isothermalpool, exterioradiabatic; no finiteheater/contact/loss/feed/Joule/magnetheatbalance.','Total force notpressure or trajectory; centralplane cellproxy notunique throughcurrent.']}
(r/'results.json').write_text(json.dumps(result,indent=2)+'\n')
fig,axes=plt.subplots(1,4,figsize=(18,10));fig.subplots_adjust(left=.045,right=.97,top=.86,bottom=.23,wspace=.42)
for j,target in enumerate([-2.6,0]):
 iy=int(np.argmin(abs(y-target)));a=np.array(f[:,iy,:,:]);m=np.array(ids[:,iy,:]);valid=np.isin(m,[4,14,16,17,20]);v=np.linalg.norm(a[:,:,4:7],axis=2)/1e6;ext=[origin[0]*1000,(origin[0]+nx*h)*1000,origin[2]*1000,(origin[2]+nz*h)*1000];im=axes[j].imshow(np.ma.masked_where(~valid,v),origin='lower',extent=ext,cmap='viridis',vmin=0,vmax=35,interpolation='nearest');axes[j].contour(x,z,valid.astype(float),levels=[.5],colors='#687a83',linewidths=.5);axes[j].set(xlim=(-10.5,10.5),ylim=(-.3,28.5),xlabel='x (mm)',ylabel='z (mm)',title=['Copper front current','Steel / liquid current'][j]);fig.colorbar(im,ax=axes[j],shrink=.7,label='|J| (A/mm²)')
iy=int(np.argmin(abs(y)));m=np.array(ids[:,iy,:]);v=np.array(temp[:,iy,:])+.5;X,Z=np.meshgrid(x,z);excluded=(np.hypot(y[iy]-5,Z-16)<1.6)|(np.hypot(y[iy]-8.5,Z-19)<.6);valid=np.isin(m,[4,14,16,17,19,20,21])&~excluded;ext=[origin[0]*1000,(origin[0]+nx*h)*1000,origin[2]*1000,(origin[2]+nz*h)*1000];im=axes[2].imshow(np.ma.masked_where(~valid,v),origin='lower',extent=ext,cmap='inferno',vmin=0,vmax=1,interpolation='nearest');axes[2].contour(x,z,valid.astype(float),levels=[.5],colors='#687a83',linewidths=.5);axes[2].set(xlim=(-10.5,10.5),ylim=(-.3,28.5),xlabel='x (mm)',ylabel='z (mm)',title='Thermal cut through pool\nY ≈ 0 mm');fig.colorbar(im,ax=axes[2],shrink=.7,label='Normalized T (unit conductance only)')
iy=int(np.argmin(abs(y)));a=np.array(f[:,iy,:,:]);m=np.array(ids[:,iy,:]);ext=[origin[0]*1000,(origin[0]+nx*h)*1000,origin[2]*1000,(origin[2]+nz*h)*1000];im=axes[3].imshow(np.ma.masked_where(m!=4,a[:,:,10]*1e-6),origin='lower',extent=ext,cmap='RdBu_r',vmin=-12,vmax=12,interpolation='nearest');axes[3].contour(x,z,(m==4).astype(float),levels=[.5],colors='#687a83',linewidths=.6);axes[3].set(xlim=(-1.5,1.5),ylim=(-.2,6.4),xlabel='x (mm)',ylabel='z (mm)',title='Liquid vertical force\n(different scale)');fig.colorbar(im,ax=axes[3],shrink=.7,label='fz (mN/mm³)')
fig.suptitle('Flat Cu / mica assembly — initial ideal-contact screens',fontsize=18,y=.97)
fig.text(.5,.91,f'100 A: {c["terminal_voltage_V"]:.4f} V / {c["joule_power_W"]:.2f} W instantaneous | downward force {-1000*c["liquid_force_N"][2]:.2f} mN | wall-to-pool R = {t["thermal_resistance_K_W"]:.2f} K/W',ha='center',fontsize=12)
fig.text(.5,.13,'Actual saved GPU fields, 0.1 mm grid. Full connected steel, copper, screws, mica and sleeve paths; full prescribed magnet sources.\nThermal source: nominal cartridge winding-span wall, isothermal liquid and adiabatic exterior. This is not an operating-temperature field.\nIdeal Cu/SS joints; preload, contact resistance, real insulator selection, finite heater power and external heat paths remain unqualified.\nV1 electrical energy check failed; V2 tighter residual passed the unchanged gate. No jetting run or temperature claim.',ha='center',fontsize=10,linespacing=1.7)
fig.savefig(r/'results.png',dpi=160);plt.close(fig);print(json.dumps({'V':c['terminal_voltage_V'],'W':c['joule_power_W'],'F':c['liquid_force_N'],'R':t['thermal_resistance_K_W'],'power_by_material':powers,'temperature_range':[lo,hi]},indent=2))
