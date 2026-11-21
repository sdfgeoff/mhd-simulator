"""Enlarge only the two Concept1 poles; preserve every other geometry byte."""
import copy, hashlib, json
from pathlib import Path
import numpy as np

source=Path('hardware/designs/concept1-developed-v1')
out=Path('hardware/designs/concept1-larger-poles-v1')
review=Path('hardware/concept1-larger-poles-review')
out.mkdir(exist_ok=False); review.mkdir(exist_ok=False)
g=json.loads((source/'assembly.gltf').read_text())
original=(source/'assembly.bin').read_bytes(); data=bytearray(original)
changed=[]; ranges=[]
for mesh in g['meshes']:
    if 'SmCo' not in mesh['name']: continue
    accessor=g['accessors'][mesh['primitives'][0]['attributes']['POSITION']]
    view=g['bufferViews'][accessor['bufferView']]
    start=view.get('byteOffset',0)+accessor.get('byteOffset',0)
    size=accessor['count']*12
    xyz=np.frombuffer(data,dtype='<f4',count=accessor['count']*3,offset=start).reshape(-1,3)
    old=xyz.copy(); sign=-1 if old[:,0].mean()<0 else 1
    # Inner faces remain +/-2.7 mm; double normal thickness and Y pole width.
    xyz[:,0]=sign*(.0027+2*(abs(old[:,0])-.0027))
    xyz[:,1]=old[:,1]*2
    accessor['min']=xyz.min(axis=0).tolist(); accessor['max']=xyz.max(axis=0).tolist()
    changed.append({'name':mesh['name'],'old_bounds_mm':[(old.min(0)*1000).tolist(),(old.max(0)*1000).tolist()],
                    'new_bounds_mm':[(xyz.min(0)*1000).tolist(),(xyz.max(0)*1000).tolist()]})
    ranges.append((start,start+size))
assert len(changed)==2
for i,(a,b) in enumerate(zip(original,data)):
    if a!=b: assert any(lo<=i<hi for lo,hi in ranges)
(out/'assembly.gltf').write_text(json.dumps(g,indent=2).replace('5mm SmCo cube','10x10x5mm SmCo pole')+'\n')
(out/'assembly.bin').write_bytes(data)
for name in ['assembly-h0100.mhd.json','flow-h0100.mhd.json']:
    p=json.loads((source/name).read_text()); p['geometry']=str((out/'assembly.gltf').resolve())
    # Same fluid domain as baseline. Full magnet meshes remain field sources;
    # only electrically isolated outer magnet occupancy extends beyond its air box.
    (out/name).write_text(json.dumps(p,indent=2).replace('5mm SmCo cube','10x10x5mm SmCo pole')+'\n')
record={'status':'CAD candidate; no performance result yet','authorization':[860,871],
        'pole_dimensions_xyz_mm':[10,10,5],'pole_gap_mm':5.4,
        'unchanged':'All nonmagnet geometry bytes, source magnetization, current/pulse, domain and numerics.',
        'changes':changed,'nonmagnet_buffer_bytes_identical':True,
        'baseline_binary_sha256':hashlib.sha256(original).hexdigest(),
        'limits':['Same 2.5mm magnet extent below Z0; no increased flight obstruction.',
                  'Magnet carriers/cooling unresolved; not an operating-temperature claim.',
                  'Full source meshes retained; isolated outer magnet occupancy cropped by unchanged domain.']}
(review/'cad-review.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
