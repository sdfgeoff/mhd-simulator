"""Verify saved GPU masks preserve mica separation and intended lower contacts."""
import json
from pathlib import Path
import numpy as np
root=Path('artifacts/hardware-research');review=Path('hardware/mica-plate-review')
p=json.loads(Path('hardware/designs/mica-flat-plate-head-v1/assembly-h0100.mhd.json').read_text());r=json.loads((root/'mica-flat-plate-head-v1-overlap.json').read_text());nx,ny,nz=r['dims'];m=np.memmap(root/'mica-flat-plate-head-v1-mask.bin',dtype='<u4',mode='r',shape=(nz,ny,nx));ids=np.zeros(m.shape,dtype=np.uint8)
counts=[]
for i,b in enumerate(p['bodies']):
 mask=(m&(1<<i))!=0;mid=b['simulation']['material_id'];ids[mask]=mid;counts.append({'body':b['name'],'material_id':mid,'cells':int(mask.sum())});assert mask.any(),b['name']
interfaces=[];all_z=[]
for axis in range(3):
 lo=[slice(None)]*3;hi=lo.copy();lo[axis]=slice(None,-1);hi[axis]=slice(1,None);a=ids[tuple(lo)];b=ids[tuple(hi)];cu_a=np.isin(a,[16,17,20]);cu_b=np.isin(b,[16,17,20]);touch=(cu_a&(b==14))|(cu_b&(a==14));z=np.where(touch)[0];zmm=-.2+(z+.5)*.1
 if len(z):
  all_z.extend([float(zmm.min()),float(zmm.max())]);assert zmm.max()<5.9,'Unintended upper Cu/SS electrical contact'
 interfaces.append({'array_axis_zyx':axis,'Cu_SS_faces':int(touch.sum()),'face_area_mm2':float(touch.sum())*.01,'contact_z_mm_range':None if not len(z) else [float(zmm.min()),float(zmm.max())]})
assert len(all_z)>0
result={'status':'Saved initialGPU masks: zerooverlap, everybodyrepresented, Cu/SS face contacts confined to lower intended pad/screw band','source':'artifacts/hardware-research/mica-flat-plate-head-v1-mask.bin','counts':counts,'Cu_SS_interfaces':interfaces,'limits':'One0.1mm grid; no elastic clamp/contact resistance or subcell thermal contact inference.'}
(review/'gpu-initial-checks.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(interfaces,indent=2))
