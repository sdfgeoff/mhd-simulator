"""Manufacture-first Concept1 candidates; CPU CAD only, preserving other mesh bytes."""
import argparse,copy,json
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import draw_hardware_layouts as drawing
from make_conduction_hardware import box,cylinder,triangles
from make_hardware import lathe,orient
from make_v6_conduction import as_solid

source=Path('hardware/designs/concept1-larger-poles-v1')
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--version',default='v2');args=parser.parse_args()
review=Path('hardware/concept1-sturdy-tip-review')/('cad-'+args.version);review.mkdir(exist_ok=False)
g=json.loads((source/'assembly.gltf').read_text());original=(source/'assembly.bin').read_bytes()
parts,_=drawing.load(str(source/'assembly.gltf'))
records=[];all_parts=[]
for diameter in [.4,.5]:
    out=Path(f'hardware/designs/concept1-sturdy-tip-{args.version}-{round(diameter*100):03d}')
    out.mkdir(exist_ok=False);data=bytearray(original);newg=copy.deepcopy(g);modified={}
    cavity=as_solid(lathe([(0,-.6,False),(diameter/2,-.6,False),(diameter/2,.1,False),(1.5,.7,False),(1.5,17,False),(0,17,False)],segments=32))
    # Rebuild steel from nominal primitives; Boolean operations against rounded
    # imported faces can leave near-coincident slivers that defeat raster integrals.
    steel=box([4,16,4.7],[0,0,2.35])+box([1.2,1.2,.5],[0,0,-.25])+box([22,20,12],[0,0,10.7])-cavity
    steel=steel-cylinder(3.05,22.2,[0,5.5,9.7],'x')-cylinder(1.55,22.2,[0,-5.5,9.7],'x')
    for sign in [-1,1]:
        steel=steel-cylinder(1.6,2.3,[0,sign*4,1.05])-cylinder(1,4.2,[0,sign*4,4.1])
    for y,tip,top in [(5.5,12.7,16.7),(-5.5,11.2,17.2)]:
        steel=steel-cylinder(1.5,top-tip+.2,[5,y,(tip+top)/2+.1])
    modified[0]=steel
    if diameter!=.4:
        modified[1]=as_solid(lathe([(0,-.5,False),(diameter/2,-.5,False),(diameter/2,.1,False),(1.5,.7,False),(1.5,3.7,False),(0,3.7,False)],segments=32))
    for i,(name,_,solid) in enumerate(parts):
        if 'copper strip with flat clamp pad' in name or 'mica sheet under head' in name:
            bounds=solid.bounding_box();sign=1 if bounds[1]+bounds[4]>0 else -1
            modified[i]=solid^box([40,39.4,40],[0,sign*20.3,0])
    assert len(steel.decompose())==1
    newparts=[];changes=[]
    for i,(name,faces,solid) in enumerate(parts):
        if i in modified:
            solid=modified[i];assert solid.volume()>0
            faces=np.asarray(orient(triangles(solid)),dtype=np.float32)
            points=(faces.reshape(-1,3)*.001).astype('<f4')
            primitive=newg['meshes'][i]['primitives'][0]
            accessor=newg['accessors'][primitive['attributes']['POSITION']]
            view=newg['bufferViews'][accessor['bufferView']]
            view.update(byteOffset=len(data),byteLength=points.nbytes)
            accessor.update(count=len(points),min=points.min(0).tolist(),max=points.max(0).tolist())
            data.extend(points.tobytes());changes.append(i)
        newparts.append((name,faces,solid))
    assert data[:len(original)]==original
    overlaps=[]
    for i,(name,_,solid) in enumerate(newparts):
        for other,_,body in newparts[i+1:]:
            v=(solid^body).volume()
            if v>1e-5:overlaps.append([name,other,v])
    assert not overlaps,overlaps
    newg['buffers'][0]['byteLength']=len(data)
    (out/'assembly.bin').write_bytes(data);(out/'assembly.gltf').write_text(json.dumps(newg,indent=2)+'\n')
    for template,filename in [('assembly-h0100.mhd.json','assembly-h0100.mhd.json'),('flow-h0100-flightspace.mhd.json','flow-h0100.mhd.json')]:
        p=json.loads((source/template).read_text());p['geometry']=str((out/'assembly.gltf').resolve())
        for i in changes:p['bodies'][i]['triangles']=len(newparts[i][1])
        (out/filename).write_text(json.dumps(p,indent=2)+'\n')
    checked,_=drawing.load(str(out/'assembly.gltf'));assert len(checked)==len(parts)
    record={'design':str(out),'outlet_diameter_mm':diameter,'straight_land_mm':.6,'square_tip_width_mm':1.2,
            'minimum_nominal_tip_wall_mm':.6-diameter/2,'nominal_contact_area_per_side_mm2':1.2*.4,
            'steel_one_piece':True,'changed_body_indices':changes,'CPU_overlaps':overlaps,
            'unchanged_mesh_payloads':'All original buffer bytes retained; only changed meshes reference newly appended payloads.',
            'status':'CPU CAD only; GPU masks and 100A current/force screen pending',
            'tradeoff':'Broader stainless can bypass liquid current. Larger exit may alter drop size/tail; no improved jetting claim.'}
    (out/'manifest.json').write_text(json.dumps(record,indent=2)+'\n');records.append(record);all_parts.append(checked)
old=drawing.category
drawing.category=lambda n:'liquid' if 'Molten' in n else ('magnet' if 'SmCo' in n else ('mica' if 'ceramic' in n else old(n)))
fig,axes=plt.subplots(1,3,figsize=(13,7),constrained_layout=True)
for ax,bodies,title in zip(axes,[parts,*all_parts],['Current 0.4 mm outlet','Sturdy tip / 0.4 mm outlet','Sturdy tip / 0.5 mm outlet']):
    drawing.section(ax,bodies,'yz',0);ax.set(xlim=(-1.8,1.8),ylim=(-.8,1.4),title=title,xlabel='y (mm)',ylabel='z (mm)');ax.set_aspect('equal')
fig.suptitle('Concept1: stronger tip and broader flat contacts — CPU CAD candidates\nTip width 0.8 → 1.2 mm; nominal contact area 0.32 → 0.48 mm² per side',fontsize=14)
fig.supxlabel('Blue: solder | grey: one-piece stainless | orange: copper | yellow: mica. Same outer assembly, clamps and magnets.\nOnly strip/mica inner edges move to fit the broader tip. Electrical shunting and jetting performance remain untested.',fontsize=10)
fig.savefig(review/'proposed-tip-sections.png',dpi=160)
(review/'cad-review.json').write_text(json.dumps(records,indent=2)+'\n')
print(json.dumps(records,indent=2))
