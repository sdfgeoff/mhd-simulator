"""Flat copper plates on mica: actual assembly envelopes and machining sections."""
import copy,json,math
from pathlib import Path
import manifold3d as cad
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from make_conduction_hardware import box,cylinder,triangles
from make_v6_conduction import as_solid
from make_hardware import emit,orient,lathe
import draw_hardware_layouts as layouts
out=Path('hardware/designs/mica-flat-plate-head-v1');out.mkdir(exist_ok=False)
review=Path('hardware/mica-plate-review');review.mkdir(parents=True,exist_ok=True)
def cy(r,h,c):return cad.Manifold.cylinder(h,r,circular_segments=96,center=True).rotate([90,0,0]).translate(c)
def ring(ro,ri,h,c):return cy(ro,h,c)-cy(ri,h+.2,c)
t=.6+.8/math.tan(math.radians(59))
pool=as_solid(lathe([(0,0,False),(.2,0,False),(.2,.6,False),(1,t,False),(1,6,False),(0,6,False)]))
cavity=as_solid(lathe([(0,-1,False),(.2,-1,False),(.2,.6,False),(1,t,False),(1,23,False),(0,23,False)]))
steel=box([3,3,22],[0,0,11])+box([20,3,2],[0,0,2.2])+box([20,12,9],[0,4.5,17.5])
for sign in [-1,1]:
    steel=steel+box([3,3,12.2],[sign*8.5,0,6.9])+box([7,3.1,5],[sign*6.5,-.05,3.3])
    for cx,kx in [(2.5,2),(6,6.5)]:
        fillet=box([1,3,1],[kx,0,3.7])-cy(1,3.2,[cx,0,4.2])
        if sign<0:fillet=fillet.mirror([1,0,0])
        steel=steel+fillet
steel=steel-cavity-cylinder(1.6,20.2,[0,5,16],'x')-cylinder(.6,20.2,[0,8.5,19],'x')
parts=[]
for sign in [-1,1]:
    x=sign*6.5;label='Left' if sign<0 else 'Right'
    plate=box([7,2,27.2],[x,-2.6,14.4])
    mica=box([7,.1,16.2],[x,-1.55,13.9])
    # Flush90degree M2 countersink in a2mm plate: retains1mm behind the cone.
    countersink=cad.Manifold.cylinder(1,2,1,circular_segments=96,center=True).rotate([-90,0,0]).translate([x,-3.1,3.3])
    plate=plate-countersink-cy(1,2.2,[x,-2.6,3.3])-cy(2.1,2.2,[x,-2.6,20])-cy(1.6,2.2,[x,-2.6,26])
    mica=mica-cy(2.1,.3,[x,-1.55,20])
    steel=steel-cy(1,2.6,[x,-.3,3.3])-cy(1.5,3.3,[x,.05,20])
    # Ideal thread envelope has matching surfaces, not overlapping solid threads.
    lower_screw=countersink+cy(1,3.6,[x,-.8,3.3])
    upper_screw=cy(1.5,5.8,[x,-1.2,20])+cy(2.75,2.5,[x,-5.35,20])
    sleeve=ring(2,1.6,2.1,[x,-2.55,20]);washer=ring(3.5,1.6,.5,[x,-3.85,20])
    # A small virtual copper annulus denotes the cable clamp port, not the whole plate.
    terminal=plate^cy(2.75,2.2,[x,-2.6,26]);plate=plate-terminal
    parts.extend([(label+' flat copper plate',plate,20),(label+' upper mica sheet',mica,19),(label+' flush M2 contact screw envelope',lower_screw,14),(label+' insulated M3 mount screw envelope',upper_screw,14),(label+' insulating shank sleeve envelope',sleeve,21),(label+' mica head washer stack',washer,19),(label+' copper cable clamp virtual port',terminal,16 if sign<0 else 17)])
parts=[('One-piece stainless heater block and nozzle',steel,14),('Liquid solder',pool,4),*parts,('Heater cartridge sheath envelope',cylinder(1.55,25.4,[0,5,16],'x'),14),('Sensor sheath envelope',cylinder(.5,25.4,[0,8.5,19],'x'),14)]
parts.extend([('Front SmCo magnet stack',box([25,10,5],[0,-9,2.5]),18),('Rear SmCo magnet stack',box([25,10,5],[0,7,2.5]),18)])
assert len(steel.decompose())==1
for i,(n,s,_) in enumerate(parts):
    assert s.status()==cad.Error.NoError and s.volume()>0,n
    for m,r,_ in parts[i+1:]:
        v=(s^r).volume();assert v<1e-5,(n,m,v)
meshes=[(n,orient(triangles(s))) for n,s,_ in parts];emit(out/'body.gltf',meshes,None)
base=json.loads(Path('hardware/designs/plain-bore-2mm-boolean-mesh/flow-h0050.mhd.json').read_text());p=copy.deepcopy(base);p['geometry']=str((out/'body.gltf').resolve());p['domain']={'min_mm':[-10.2,-6.8,-.2],'max_mm':[10.2,10.7,28.2],'cell_mm':.1};p['bodies']=[]
# Explicit fixed screening properties. Contact resistance/hot hardware values need separate qualification.
copper=copy.deepcopy(next(m for m in p['materials'] if m['id']==16));copper.update(id=20,name='Broad copper plate',conductivity_s_m=25e6);copper['thermal']['conductivity_w_m_k']=394.
for mid in [16,17]:
    j=next(j for j,m in enumerate(p['materials']) if m['id']==mid);p['materials'][j]=copy.deepcopy(copper);p['materials'][j].update(id=mid,name=('Positive' if mid==16 else 'Negative')+' copper cable port')
mica=copy.deepcopy(next(m for m in p['materials'] if m['id']==7));mica.update(id=19,name='Mica sheet and head washer',conductivity_s_m=0.);mica['thermal']['conductivity_w_m_k']=.528
insulator=copy.deepcopy(mica);insulator.update(id=21,name='Insulating sleeve envelope, ceramic placeholder');insulator['thermal']['conductivity_w_m_k']=25.
p['materials']=[m for m in p['materials'] if m['id'] not in [19,20,21]]+[mica,copper,insulator]
mag=[]
for i,((n,s,mid),(_,faces)) in enumerate(zip(parts,meshes)):
    p['bodies'].append({'name':n+' / 0','triangles':len(faces),'visible':True,'simulation':{'included':True,'material_id':mid,'temperature_c':20 if mid==18 else 250,'fixed_temperature_c':None,'heat_source_w_m3':0}})
    if mid==18:mag.append({'body':i,'magnetization_a_m':[0,-755985.9796865028,0],'subdivisions':3,'smoothing_mm':.05})
p['electromagnetics']['magnets']=mag;(out/'assembly-h0100.mhd.json').write_text(json.dumps(p,indent=2)+'\n')
report={'status':'CPU assembly CAD; no physics results','body_count':len(parts),'steel_components':len(steel.decompose()),'CPU_overlaps':0,'copper_plate_each_mm':[7,2,27.2],'upper_mica_thickness_mm':.1,'lower_contact_each_nominal_mm':[7,5],'contact_pad_front_y_mm':-1.6,'upper_body_front_y_mm':-1.5,'copper_span_y_mm':[-3.6,-1.6],'heater_axis':'x','heater_centre_yz_mm':[5,16],'heater_envelope_mm':[3.1,25.4],'sensor_axis':'x','sensor_centre_yz_mm':[8.5,19],'front_pole_face_y_mm':-4,'rear_pole_face_y_mm':2,'pole_gap_mm':6,'insulated_M3_mounts_xz_mm':[[-6.5,20],[6.5,20]],'M2_flush_contact_screws_xz_mm':[[-6.5,3.3],[6.5,3.3]],'M3_cable_ports_xz_mm':[[-6.5,26],[6.5,26]],'insulator_sleeve_OD_ID_length_mm':[4,3.2,2.1],'mica_head_washer_OD_ID_thickness_mm':[7,3.2,.5],'limitations':['Sleeve and washer dimensions are provisional envelopes, not sourced components or compression qualification.','Flat Cu plates need only profile cutting, drilling and M2 countersinks; no bent fingers or fitted jaws.','Steel blank has two broad contact pads only0.1mm proud of the upper front face; mica must not extend into the intended lower contacts.','M3 cable lug/bolt/nut outside upper block not modeled. Virtual copper annuli prescribe cable-clamp ports for later stationary solve.','Thread envelopes use matching nominal diameters without modeled thread teeth; actual tap drills, engagement and drill cones need manufacturing drawings.','Heater retention, clamping preload, magnetic carriers, external mounting/ground paths and clearances to actual lugs/leads unfinished.','Thermal constants provisional fixed inputs, not hot-property qualification. Do not run old integral thermal mode: it treats copper as stainless and omits mica.','All steel,Cu,fastener,insulator current/thermal paths must be retained in full assembly tests. Ideal CuSS contact is only an initial limit; proper interface resistance sensitivity required.','No GPU initial masks, electrical, thermal, jetting or operating-temperature result yet.']}
(out/'manifest.json').write_text(json.dumps(report,indent=2)+'\n');(review/'cad-review.json').write_text(json.dumps(report,indent=2)+'\n')
loaded,_=layouts.load(str(out/'body.gltf'))
orig=layouts.category;layouts.category=lambda n:'mica' if 'sleeve' in n.lower() else orig(n)
fig,axes=plt.subplots(1,3,figsize=(16,10));fig.subplots_adjust(left=.05,right=.98,top=.86,bottom=.23,wspace=.3)
for ax,plane,height,title in [(axes[0],'xz',2.6,'A. Copper front cut (Y = -2.6 mm)'),(axes[1],'xz',0,'B. Central steel / melt cut (Y = 0)'),(axes[2],'yz',6.5,'C. Mount / contact cut (X = +6.5 mm)')]:
    layouts.section(ax,loaded,plane,height);ax.set(xlim=(-14,14),ylim=(-1,30),title=title)
axes[0].annotate('Cable clamp ports',xy=(6.5,26),xytext=(-1,29),arrowprops={'arrowstyle':'->'},fontsize=9)
axes[0].annotate('Insulated upper mounts',xy=(-6.5,20),xytext=(-13,23),arrowprops={'arrowstyle':'->'},fontsize=9)
axes[0].annotate('Flush contact screws',xy=(6.5,3.3),xytext=(-3,8),arrowprops={'arrowstyle':'->'},fontsize=9)
axes[1].annotate('Open feed bore',xy=(0,22),xytext=(-11,26),arrowprops={'arrowstyle':'->'},fontsize=9)
axes[2].annotate('Mica + shank sleeve\n+ head washer',xy=(-2,20),xytext=(-12,25),arrowprops={'arrowstyle':'->'},fontsize=9)
axes[2].annotate('Direct Cu / SS\nlower contact',xy=(-1.6,3.3),xytext=(4,8),arrowprops={'arrowstyle':'->'},fontsize=9)
fig.suptitle('Mica solution — two flat copper plates on an integrated stainless head',fontsize=17,y=.97)
fig.text(.5,.91,'Proposed 100 A assembly | 2 mm chamber / 0.4 mm outlet / 0.6 mm land | heater and sensor drill from X',ha='center',fontsize=11)
fig.text(.5,.13,'Copper plates: simple 7 x 27.2 x 2 mm rectangles. Upper mica 0.1 mm; lower 7 x 5 mm pads are intentional metal contacts.\nM2 countersinks leave 1 mm of copper behind the cones. Upper M3 bolts are insulated at the head AND shank.\nFront pole face Y = -4 mm, rear Y = +2 mm: 6 mm gap, shifted toward the copper side. Full field must be recomputed.\nCPU connected-solid / overlap checks only. Insulators are unsourced envelopes; preload, contacts, mounts and heat balance remain unqualified.',ha='center',fontsize=10,linespacing=1.7)
fig.savefig(review/'proposed-cuts.png',dpi=160);plt.close(fig);print(json.dumps(report,indent=2))
