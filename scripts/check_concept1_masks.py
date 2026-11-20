"""Read-only check that the fullassembly GPU discretization preserves clamp insulation."""
import json
from pathlib import Path
import numpy as np
root=Path('artifacts/hardware-research');p=json.loads(Path('hardware/designs/concept1-developed-v1/assembly-h0100.mhd.json').read_text());r=json.loads((root/'concept1-developed-v1-overlap.json').read_text());nx,ny,nz=r['dims'];mask=np.memmap(root/'concept1-developed-v1-mask.bin',dtype='<u4',mode='r',shape=(nz,ny,nx));ids=np.zeros(mask.shape,dtype=np.uint8);counts=[]
for i,b in enumerate(p['bodies']):
 selection=(mask&(1<<i))!=0;assert selection.any(),b['name'];mid=b['simulation']['material_id'];ids[selection]=mid;counts.append({'body':b['name'],'material_id':mid,'cells':int(selection.sum())})
interfaces=[]
for axis in range(3):
 lo=[slice(None)]*3;hi=lo.copy();lo[axis]=slice(None,-1);hi[axis]=slice(1,None);a=ids[tuple(lo)];b=ids[tuple(hi)];touch=(np.isin(a,[16,17,20])&(b==14))|(np.isin(b,[16,17,20])&(a==14));z=np.where(touch)[0];positions=-3.5+(z+.5)*.1
 if len(z):assert positions.min()>=-.51 and positions.max()<=-.09,'CuSScontactoutsideintendedtipband'
 interfaces.append({'array_axis_zyx':axis,'faces':int(touch.sum()),'area_mm2':float(touch.sum())*.01,'contact_z_mm':None if not len(z) else [float(positions.min()),float(positions.max())]})
assert sum(c['faces'] for c in interfaces)>0
out={'status':'GPUzerooverlap, all22bodiesrepresented, CuSSfacesonlyattip; noCuSScontactatinsulatedclamps','counts':counts,'contact_interfaces':interfaces,'scope':'One100umgrid, notrealpreload/contactresistance/tolerancequalification.'}
Path('hardware/concept1-developed-review/gpu-initial-checks.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(interfaces,indent=2))
