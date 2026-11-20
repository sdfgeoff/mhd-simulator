"""Develop Geoffrey's confirmed0.1mm/BU concept; retain its underside contact geometry."""
import argparse,copy,hashlib,json,math
from pathlib import Path
from collections import defaultdict,deque
import numpy as np
import manifold3d as cad
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from make_conduction_hardware import box,cylinder,triangles
from make_hardware import emit,orient,lathe
from make_v6_conduction import as_solid
import draw_hardware_layouts as layouts
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--draw-only',action='store_true');a=p.parse_args()
out=Path('hardware/designs/concept1-developed-v1');review=Path('hardware/concept1-developed-review');review.mkdir(parents=True,exist_ok=True)
def cleaned(v,f):
    v,inv=np.unique(v.round(6),axis=0,return_inverse=True);f=inv[f].astype(np.uint32)
    f=f[np.linalg.norm(np.cross(v[f[:,1]]-v[f[:,0]],v[f[:,2]]-v[f[:,0]]),axis=1)>1e-10]
    _,i=np.unique(np.sort(f,axis=1),axis=0,return_index=True);f=f[np.sort(i)];edges=defaultdict(list)
    for i,t in enumerate(f):
        for x,y in zip(t,np.roll(t,-1)):edges[tuple(sorted((x,y)))].append((i,x<y))
    assert all(len(es)==2 for es in edges.values()),'Source topology still open after degenerate removal'
    adj=defaultdict(list)
    for es in edges.values():
        (i,x),(j,y)=es;adj[i].append((j,x==y));adj[j].append((i,x==y))
    flips={}
    for start in range(len(f)):
        if start in flips:continue
        q=deque([start]);flips[start]=False;component=[]
        while q:
            i=q.popleft();component.append(i)
            for j,change in adj[i]:
                if j not in flips:flips[j]=flips[i]^change;q.append(j)
                else:assert flips[j]==flips[i]^change
        for i in component:
            if flips[i]:f[i]=f[i,[0,2,1]]
        vol=np.einsum('ij,ij->i',v[f[component,0]],np.cross(v[f[component,1]],v[f[component,2]])).sum()
        if vol<0:
            for i in component:f[i]=f[i,[0,2,1]]
    s=cad.Manifold(cad.Mesh(v.astype(np.float32),f));assert s.status()==cad.Error.NoError
    return s

def generate():
    out.mkdir(exist_ok=False)
    source=Path('/home/geoffrey/Reference/OffTopic/mhd-simulator/Concept1.blend');snapshot=Path('artifacts/hardware-research/concept1-review');meta=json.loads((snapshot/'scene.json').read_text());assert hashlib.sha256(source.read_bytes()).hexdigest()==meta['source_sha256'],'User source changed; re-inspect first'
    meshes=json.loads((snapshot/'evaluated-meshes.json').read_text());solids={m['name']:cleaned(np.array(m['vertices'])*.1,np.array(m['triangles'])) for m in meshes}
    # Rebuild the same nominal lower profile so newcuts do not create
    # nanometre Boolean slivers at rounded imported cylinder coordinates.
    profile=[(0,-.5,False),(.2,-.5,False),(.2,.1,False),(1.5,.7,False),(1.5,3.7,False),(0,3.7,False)]
    pool=as_solid(lathe(profile,segments=32))
    cavity=as_solid(lathe([(0,-.6,False),(.2,-.6,False),(.2,.1,False),(1.5,.7,False),(1.5,17,False),(0,17,False)],segments=32))
    lower=box([4,16,4.7],[0,0,2.35])+box([.8,.8,.5],[0,0,-.25])
    reference=lower-cavity
    for sign in [-1,1]:reference=reference-cad.Manifold.cylinder(5,1,circular_segments=32,center=True).translate([0,sign*4,2.35])
    source_difference=(reference-solids['HeatBlock Stainless Steel']).volume()+(solids['HeatBlock Stainless Steel']-reference).volume()
    print('Nominal lowerhead/source symmetricdifference mm3',source_difference)
    assert source_difference<1e-3,'Nominal rebuild changed the user shape appreciably'
    assert abs(pool.volume()/solids['Molten Tin'].volume()-1)<1e-6
    steel=lower+box([22,20,12],[0,0,10.7])-cavity
    steel=steel-cylinder(3.05,22.2,[0,5.5,9.7],'x')-cylinder(1.55,22.2,[0,-5.5,9.7],'x')
    parts=[]
    for sign in [-1,1]:
        y=sign*4;label='Positive' if sign<0 else 'Negative';src='0.4mmm Copper Sheet'+('.001' if sign<0 else '')
        # Original3mmwide strip with a single rectangularwide pad for a3.2mm
        # shoulder clearance. Original0.32mm2 CuSS tip-edge contact retained.
        cu=solids[src]+cad.Manifold(box([4.8,5.2,.4],[0,y,-.3]).to_mesh());cu=cu-cylinder(1.6,.6,[0,y,-.3])
        mica=solids['0.1mm Mica Sheet'+('' if sign<0 else '.001')]+cad.Manifold(box([4.8,5.2,.1],[0,y,-.05]).to_mesh());mica=mica-cylinder(1.6,.3,[0,y,-.05])
        steel=steel-cylinder(1.6,2.3,[0,y,1.05])-cylinder(1,4.2,[0,y,4.1])
        sleeve=cylinder(1.5,2.5,[0,y,.75])-cylinder(1.05,2.7,[0,y,.75])
        flange=cylinder(2.1,1.5,[0,y,-1.25])-cylinder(1.05,1.7,[0,y,-1.25])
        screw=cylinder(1,8,[0,y,2])+cylinder(1.8,1.3,[0,y,-2.65])
        # Supply port on the far end of each strip; no whole-strip equipotential.
        terminal=cu^box([4,1,.6],[0,sign*7.9,-.3]);cu=cu-terminal
        parts.extend([(label+' copper strip with flat clamp pad',cu,20),(label+' mica sheet under head',mica,19),(label+' M2 ceramic shoulder sleeve',sleeve,21),(label+' M2 ceramic shoulder flange',flange,21),(label+' insulated M2x8 clamp screw',screw,14),(label+' copper cable port virtual region',terminal,16 if sign<0 else 17)])
    # Cartridge retaining screws are represented as ideal nominal thread envelopes.
    for y,tip,top in [(5.5,12.7,16.7),(-5.5,11.2,17.2)]:
        steel=steel-cylinder(1.5,top-tip+.2,[5,y,(tip+top)/2+.1]);parts.append((('Heater' if y>0 else 'Sensor')+' retaining grub envelope',cylinder(1.5,top-tip,[5,y,(tip+top)/2]),14))
    parts=[('Integrated stainless Concept1 head and heater block',steel,14),('Molten SnCu pool original concept',pool,4),*parts,('6x20 heater cartridge sheath envelope',cylinder(3,20,[0,5.5,9.7],'x'),14),('3x15 sensor cartridge sheath envelope',cylinder(1.5,15,[0,-5.5,9.7],'x'),14)]
    for sign in [-1,1]:
        parts.append((('Left' if sign<0 else 'Right')+'5mm SmCo cube',box([5,5,5],[sign*5.2,0,0]),18))
        parts.append((('Left' if sign<0 else 'Right')+' magnet-face mica shield',box([.2,5,5],[sign*2.6,0,0]),19))
    assert len(steel.decompose())==1
    overlaps=[]
    for i,(n,s,_) in enumerate(parts):
        assert s.status()==cad.Error.NoError and s.volume()>0,n
        for m,t,_ in parts[i+1:]:
            v=(s^t).volume()
            if v>1e-5:overlaps.append({'a':n,'b':m,'volume_mm3':v})
    assert not overlaps,overlaps
    faces=[]
    for n,s,_ in parts:
        try:faces.append((n,orient(triangles(s))))
        except AssertionError:print('Closurecheckfailed',n);raise
    emit(out/'assembly.gltf',faces,None)
    project=copy.deepcopy(json.loads(Path('hardware/designs/mica-flat-plate-head-v1/assembly-h0100.mhd.json').read_text()));project['geometry']=str((out/'assembly.gltf').resolve());project['domain']={'min_mm':[-11.2,-10.2,-3.5],'max_mm':[11.2,10.2,17.4],'cell_mm':.1};project['bodies']=[];mag=[]
    for i,((n,s,mid),(_,f)) in enumerate(zip(parts,faces)):
        project['bodies'].append({'name':n+' / 0','triangles':len(f),'visible':True,'simulation':{'included':True,'material_id':mid,'temperature_c':20 if mid==18 else 250,'fixed_temperature_c':None,'heat_source_w_m3':0}})
        if mid==18:mag.append({'body':i,'magnetization_a_m':[755985.9796865028,0,0],'subdivisions':3,'smoothing_mm':.05})
    project['electromagnetics']['magnets']=mag;(out/'assembly-h0100.mhd.json').write_text(json.dumps(project,indent=2)+'\n')
    manifest={'status':'DevelopedCAD only, no simulation yet','source':str(source),'source_sha256':meta['source_sha256'],'confirmed_mm_per_Blenderunit':.1,'bodies':len(parts),'CPU_overlaps':overlaps,'steel_components':1,'original_pool_volume_mm3':pool.volume(),'nominal_lower_head_vs_source_symmetric_difference_mm3':source_difference,'heater_bore_axis':'X','heater_centre_yz_mm':[5.5,9.7],'heater_diameter_length_mm':[6,20],'sensor_centre_yz_mm':[-5.5,9.7],'sensor_diameter_length_mm':[3,15],'upper_block_mm':[22,20,12],'upper_block_z_mm':[4.7,16.7],'feed_bore_diameter_mm':3,'magnet_inner_faces_x_mm':[-2.7,2.7],'pole_gap_mm':5.4,'magnet_face_mica_mm':.2,'air_gap_steel_to_mica_mm':.5,'copper_clamp_pad_width_length_mm':[4.8,5.2],'ceramic_candidate':'McMaster92107A111, M2 sleeveL2.5 OD3 ID2.1 flangeOD4.2 thickness1.5','sleeve_bottom_axial_clearance_mm':.2,'thread_engagement_nominal_mm':3.8,'minimum_target_clearance_below_exit_mm':2.8,'source_changes':['Nozzle/pool nominalprofile rebuilt to avoid Boolean slivers; lowerhead/source symmetricdifference recorded, not a new design.','Upperheaterblock machined integrally withstainlesshead; alternativecurrentbypass included.','Copper/mica gainflatwidepads aroundM2holes; boltinsulatedatheadandshank.','Magnetsmoved.7mmoutwards, mica.2+air.5gap tosteel; noqualifiedmagnettemperature.'],'limits':['Geometryenvelopesnotfullydimensionedtolerancedmanufacturingdrawings. Ceramiccomposition/k/preload/shipping notqualified.','Heater/sensor family dimensions, notcoldzone/internalthermalstructurequalification. Noheaterpower/ambientloss/feed/dynamicthermalbalanceyet.','Retainingscrews useidealnominalthreads; actualtapdrillcones/contacttorqueunfinished.','Magnetcarrier/frame/cables/electricalgrounds remainunfinished; no thermalcontacttomagnetassumed.','FieldassumesBr.95T SmCoandunitmu; sourcefiledidnotspecifygrade/direction. Declaredtestassumption.']}
    (out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');(review/'cad-review.json').write_text(json.dumps(manifest,indent=2)+'\n')
if not a.draw_only:generate()
loaded,_=layouts.load(str(out/'assembly.gltf'));original=layouts.category;layouts.category=lambda n:'magnet' if 'SmCo' in n else ('mica' if 'ceramic' in n.lower() else original(n))
fig,axes=plt.subplots(1,3,figsize=(16,10));fig.subplots_adjust(left=.05,right=.98,top=.85,bottom=.23,wspace=.32)
for ax,plane,height,title in [(axes[0],'yz',0,'A. Electrode / clamp axis (X=0)'),(axes[1],'xz',0,'B. Magnet axis (Y=0)'),(axes[2],'yz',5,'C. Heater and retaining screws (X=5)')]:
    layouts.section(ax,loaded,plane,height);ax.set(xlim=(-12,12),ylim=(-4,19),title=title)
axes[0].annotate('Ceramic shoulder\nhead + shank insulation',xy=(-4,-1),xytext=(-11,5.8),arrowprops={'arrowstyle':'->'},fontsize=9)
axes[0].annotate('Original tip contacts',xy=(.4,-.3),xytext=(3,2),arrowprops={'arrowstyle':'->'},fontsize=9)
axes[1].annotate('0.5mm air + 0.2mm mica',xy=(2.6,1),xytext=(1.5,5.5),arrowprops={'arrowstyle':'->'},fontsize=9)
fig.suptitle('Concept1 developed — heater / sensor and insulated underside clamps',fontsize=17,y=.97)
fig.text(.5,.91,'Confirmed scale: 1 BU = 0.1 mm | original 3 mm pool / 0.4 mm outlet | nominal 100 A | original source file unchanged',ha='center',fontsize=11)
fig.text(.5,.13,'Upper block: one machined stainless part with the head. Heater 6x20 mm; sensor 3x15 mm, both drilled from X.\nFlat copper/mica pads widen locally for M2 ceramic shoulder washers. Sleeve has 0.2 mm axial clearance; M2x8 nominal engagement 3.8 mm.\n5 mm cube poles move to X=±2.7 mm; mica shields separate them from steel. Clamp heads extend 2.8 mm below the outlet.\nCAD checks only. Magnet grade / contact / thermal constants are declared screening assumptions; carriers, cooling, tolerances and preload unfinished.',ha='center',fontsize=10,linespacing=1.7)
fig.savefig(review/'proposed-cuts.png',dpi=160);plt.close(fig)
print(out/'assembly-h0100.mhd.json')
