"""Read-only contact sensitivity and zero-control field regression."""
import json,hashlib
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
raw=Path('artifacts/hardware-research');out=Path('hardware/mica-plate-review/contact-sensitivity');state=json.loads((raw/'mica-pair-contact-v1-state.json').read_text());assert state['stage']=='complete_needs_assessment'
for p,digest in state['hashes'].items():assert hashlib.sha256(Path(p).read_bytes()).hexdigest()==digest,p
old=raw/'mica-flat-plate-v2-cold100-h0100';zero=raw/'mica-pair-contact-v1-electrical-zero';g=json.loads((zero/'grid.json').read_text());assert g==json.loads((old/'grid.json').read_text());nx,ny,nz=g['dims'];shape=(nz,ny,nx,16)
a=np.memmap(old/'em.bin',dtype='<f4',mode='r',shape=shape);b=np.memmap(zero/'em.bin',dtype='<f4',mode='r',shape=shape);ma=np.memmap(old/'material-ids.bin',dtype='<u4',mode='r',shape=shape[:3]);mb=np.memmap(zero/'material-ids.bin',dtype='<u4',mode='r',shape=shape[:3]);max_diff=0.
for z in range(nz):assert np.array_equal(ma[z],mb[z]);max_diff=max(max_diff,float(np.max(np.abs(a[z]-b[z]))))
assert max_diff==0.,'Zeroelectrical control changed saved fields'
a=np.memmap(raw/'mica-flat-plate-v2-thermal-active-span-h0100/potential.bin',dtype='<f4',mode='r');b=np.memmap(raw/'mica-pair-contact-v1-thermal-zero/potential.bin',dtype='<f4',mode='r');assert np.array_equal(a,b),'Zerothermal control changed savedpotential'
e=[];t=[]
for label,rho in [('zero',0),('low',3e-9),('high',3e-8)]:
 d=json.loads((raw/('mica-pair-contact-v1-electrical-'+label)/'report.json').read_text());assert d['boundary_current_defect']<=1e-5 and abs(d['power_relative_defect'])<=1e-4;e.append({'rho_ohm_m2':rho,'native':d})
for label,rho in [('zero',0),('low',1e-4),('high',1e-3)]:
 d=json.loads((raw/('mica-pair-contact-v1-thermal-'+label)/'report.json').read_text());assert d['return_relative_error']<=1e-5 and abs(d['energy_identity_relative_error'])<=1e-4;t.append({'resistance_m2_K_W':rho,'native':d})
result={'status':'Allsix nativecasescomplete; fixedCAD/100A electrical; independent thermal interface sensitivity','all_frozen_hashes_match':True,'zero_control':{'electrical_all_saved_field_words_identical':True,'material_mask_identical':True,'thermal_all_saved_potential_words_identical':True},'electrical':e,'thermal':t,'limits':['Resistance values are sensitivity parameters, not measured hotjoint ranges or preload/oxide law.','Selectedfaces only: stainlessgroup14 to coppergroup20(includingports16/17). Mica/steel and mica/copper interfacecontacts remain ideal; finite mica bulk resistance included.','Contactpower is included in nativeJouleheat/energyidentity. Totalpowerchange also includesredistribution ofbulkcurrent, notpurejointheat.','No transientthermal/fluid/jetting/operatingtemperature result.']};(out/'results.json').write_text(json.dumps(result,indent=2)+'\n')
fig,axes=plt.subplots(1,2,figsize=(12,7));fig.subplots_adjust(top=.80,bottom=.30,wspace=.35)
xx=np.arange(3);v=[c['native']['terminal_voltage_V'] for c in e];R=[c['native']['thermal_resistance_K_W'] for c in t]
axes[0].bar(xx,v,color=['#486d88','#6592b1','#a6c3d5']);axes[0].set(xticks=xx,xticklabels=['0','3e-9','3e-8'],xlabel='Electrical contact resistivity (Ω m²)',ylabel='Terminal voltage at 100 A (V)',ylim=(0,.26),title='Same imposed current: added voltage / heat')
for i,c in enumerate(e):axes[0].text(i,v[i]+.008,f"{c['native']['joule_power_W']:.2f} W\n{-1000*c['native']['liquid_force_N'][2]:.2f} mN down",ha='center',fontsize=10)
axes[1].bar(xx,R,color=['#9b673c','#c49356','#e4bd83']);axes[1].set(xticks=xx,xticklabels=['0','1e-4','1e-3'],xlabel='Thermal contact resistance (m² K/W)',ylabel='Wall-to-pool thermal resistance (K/W)',ylim=(0,30),title='Independent thermal interface sensitivity')
for i,value in enumerate(R):axes[1].text(i,value+.7,f'{value:.2f}',ha='center')
fig.suptitle('Cu / stainless joints — resistance sensitivity on the unchanged mica head',fontsize=16,y=.96)
fig.text(.5,.86,'Material-pair GPU treatment verified against analytic series resistance; zero controls reproduce prior saved fields exactly',ha='center',fontsize=10)
fig.text(.5,.14,'These resistance values are test parameters, NOT measured contact-quality ranges. Electrical and thermal parameters are independent.\nOnly Cu / SS faces receive the added resistance, including Cu / SS screw contacts. Mica-interface contacts remain ideal.\nAll charge / energy / heat-return checks pass. No preload, oxide law, operating-temperature or jetting prediction.',ha='center',fontsize=10,linespacing=1.7)
fig.savefig(out/'results.png',dpi=160);plt.close(fig);print(json.dumps({'V':v,'R':R,'zero_saved_fields_identical':True},indent=2))
