"""Manufacturing CAD draft A: clamped heated jaws, insulated fasteners, magnet frame.
No solver dispatch. Thread/heater winding/sensor internals are envelope models.
"""
import json,math
from pathlib import Path
import manifold3d as cad
from make_conduction_hardware import box,cylinder as base_cylinder,triangles
from make_hardware import emit,orient,lathe
import draw_hardware_layouts as layouts
from draw_hardware_layouts import load,section,COLORS
from make_v6_conduction import as_solid
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
out=Path('hardware/designs/heated-jaw-assembly-v1');out.mkdir(exist_ok=False)
def cylinder(radius,length,centre,axis='z'):
    if axis=='y':return cad.Manifold.cylinder(length,radius,circular_segments=96,center=True).rotate([90,0,0]).translate(centre)
    return base_cylinder(radius,length,centre,axis)
parts=[];records=[]
def add(name,s,material):
    assert s.volume()>0 and s.status()==cad.Error.NoError,(name,s.status())
    parts.append((name,s));records.append({'name':name,'material':material,'volume_mm3':s.volume()})
def tube(ro,ri,length,centre,axis='z'):
    return cylinder(ro,length,centre,axis)-cylinder(ri,length+.02,centre,axis)
transition=.6+.8/math.tan(math.radians(59))
add('Turned stainless cartridge',as_solid(lathe([(1.5,0,False),(1.5,17,False),(3,17,False),(3,19,False),(1,19,False),(1,transition,False),(.2,.6,False),(.2,0,False)])),'316 stainless')
add('Liquid solder',as_solid(lathe([(0,0,False),(.2,0,False),(.2,.6,False),(1,transition,False),(1,6,False),(0,6,False)])),'Sn99.3Cu0.7')
seat=cylinder(1.5,4,[0,0,2])
for sign in [-1,1]:
    jaw=box([8,20,9],[6,0,10.5])+box([4,2.6,2],[2,0,2.3])+box([1.5,2.6,3.2],[3.75,0,4.9])
    jaw=jaw-seat
    jaw=jaw-cylinder(1.6,20.2,[6,0,8.5],'y')
    jaw=jaw-cylinder(.6,20.2,[8.5,0,8.5],'y')
    for y in [-7,7]:jaw=jaw-cylinder(1.55,8.2,[6,y,12.5],'x')
    # Blind M3 tap-drill envelope from the outside X face, above both heaters.
    jaw=jaw-cylinder(1.25,4.2,[8,0,12.5],'x')
    if sign<0:jaw=jaw.mirror([1,0,0])
    add(('Negative' if sign>0 else 'Positive')+' copper clamp jaw',jaw,'Copper')
    add(('Right' if sign>0 else 'Left')+' heater sheath envelope',cylinder(1.55,20,[sign*6,0,8.5],'y'),'Heater sheath; internal electrical insulation not yet meshed')
    add(('Right' if sign>0 else 'Left')+' sensor sheath envelope',cylinder(.5,20,[sign*8.5,0,8.5],'y'),'Sensor sheath; connection/isolation requires selection')
    add(('Right' if sign>0 else 'Left')+' copper ring terminal envelope',tube(2.5,1.6,.5,[sign*10.25,0,12.5],'x'),'Copper ring lug; cable not modeled')
    screw=cylinder(1.25,5,[sign*8.7,0,12.5],'x')+cylinder(2.75,1.5,[sign*11.25,0,12.5],'x')
    add(('Right' if sign>0 else 'Left')+' terminal screw envelope',screw,'Stainless M3 threaded joint envelope')
# Two horizontal M2 tie bolts. Ceramic sleeves are shorter than jaw-face spacing,
# so the flange washers bear on copper rather than bottoming on the sleeve.
for i,y in enumerate([-7,7]):
    add(f'Clamp {i+1} ceramic sleeve',tube(1.5,1.1,19.8,[0,y,12.5],'x'),'Alumina stock tube envelope; source dimensions pending')
    for sign in [-1,1]:add(f'Clamp {i+1} ceramic washer {sign}',tube(2.5,1.1,.5,[sign*10.25,y,12.5],'x'),'Alumina insulating washer envelope')
    bolt=cylinder(1,23,[0,y,12.5],'x')+cylinder(1.75,1.5,[-11.25,y,12.5],'x')
    # Nut joined to shank as an electrical envelope, not modeled screw threads.
    bolt=bolt+cylinder(2,1.5,[11.25,y,12.5],'x')
    add(f'Clamp {i+1} metal bolt and nut envelope',bolt,'Stainless fastener')
# Common aluminium carrier frame: two open pockets, corner columns, top bridge.
frame=box([36,30,1.5],[0,0,16.15])-cylinder(1.7,2,[0,0,16.15])
for sign in [-1,1]:
    # Back, side rails and bottom/top retainers, all >=1mm nominal thickness.
    housing=box([28,2,7],[0,sign*13,-.0+2])
    for x in [-13.5,13.5]:housing=housing+box([1,12,7],[x,sign*8,2])
    for z in [-1,5]:housing=housing+box([28,12,1],[0,sign*8,z])
    # Front retaining lips only at the two x ends, away from the active gap.
    for x in [-12.25,12.25]:housing=housing+box([.5,.5,5],[x,sign*1.75,2])
    frame=frame+housing
    for x in [-16,16]:
        frame=frame+box([2,2,10.9],[x,sign*13,9.95])
        frame=frame+box([4,2,1.5],[(-15 if x<0 else 15),sign*13,5.25])
    add(('Rear' if sign>0 else 'Front')+' SmCo magnet pole stack',box([25,10,5],[0,sign*7,2]),'SmCo; two25x5x5 bars per stack')
add('Aluminium magnet carrier frame',frame,'Aluminium; mounting connection intentionally common within frame')
add('Mica collar insulating ring',tube(3,1.7,.1,[0,0,16.95]),'Mica sheet')
# Geometric checks only: intended touching interfaces have no finite overlap.
overlaps=[]
for i,(n,a) in enumerate(parts):
    for m,b in parts[i+1:]:
        volume=(a^b).volume()
        if volume>1e-5:overlaps.append({'a':n,'b':m,'volume_mm3':volume})
report={'status':'Mechanical CAD draft; not a qualified assembly or solver input','units':'mm','parts':records,'overlaps':overlaps,'nominal':{'stem_OD':3,'chamber_ID':2,'outlet_ID':.4,'outlet_land':.6,'collar_z':[17,19],'jaw_contact_z':[1.3,3.3],'heater_diameter':3.1,'heater_length':20,'bolt':'M2 envelope, y±7 z12.5','ceramic_sleeve_OD_ID_length':[3,2.2,19.8],'magnet_gap':4,'lowest_frame_z':-1.5},'unresolved':['Carriers represented as a monolithic electrical envelope; machine as rails/end plates with fastened joints, detailed joints/bolt holes pending.','Heater/sensor solid sheath envelopes have no element/internal insulation model or selected part.','Insulating sleeve/washer stock dimensions and hot compression limits must be sourced.','Clamp preload and elastic contact not simulated; current contact resistance pending.','Top collar is seated via mica; axial retainer and mounting screw details pending.','Ring lugs and terminal screws are envelopes; lug cable routing, thread contact and actual selected parts pending.','Manufacturing tolerance and assembly clearance checks require selected parts.','No electrical, thermal, field or jetting calculation for this assembly.']}
(out/'mechanical-checks.json').write_text(json.dumps(report,indent=2)+'\n')
if overlaps:raise ValueError(json.dumps(overlaps,indent=2))
meshes=[]
for n,s in parts:
    print('Meshing',n,flush=True);meshes.append((n,orient(triangles(s))))
emit(out/'assembly.gltf',meshes,None)
COLORS['heater']='#b96650'
original_category=layouts.category
layouts.category=lambda name:'mica' if 'ceramic' in name.lower() else original_category(name)
loaded,_=load(str(out/'assembly.gltf'));fig,axes=plt.subplots(1,3,figsize=(16,6));fig.subplots_adjust(top=.80,bottom=.25,wspace=.3)
for ax,plane in zip(axes,['xz','yz','xy']):
    section(ax,loaded,plane,height=12.5 if plane=='xy' else 0)
    ax.set(xlim=(-19,19),ylim=(-16,16) if plane=='xy' else (-2,20),title='XY clamp hardware at z=12.5 mm' if plane=='xy' else plane.upper()+' central section')
    if plane=='xy':ax.set(xlabel='x (mm)',ylabel='y (mm)')
fig.suptitle('Heated-jaw assembly A — mechanical CAD draft',fontsize=17)
fig.text(.5,.90,'Ø3 / Ø2 stainless stem · short Ø0.4 outlet · 3.1 × 20 mm heater envelopes · insulated M2 clamp hardware',ha='center',fontsize=10)
fig.text(.5,.10,'Parts are closed solids; CPU overlap checks only. No electrical/thermal/jetting result.\nYellow: electrical insulators. Third section shows clamp bolts at y=±7 mm. Magnet frame projects 1.5 mm below the outlet.\nCarrier joints, axial retainer, cable routing and heater/sensor internals remain to be detailed.',ha='center',fontsize=10)
fig.savefig(out/'sections.png',dpi=160);plt.close(fig);print(json.dumps({'parts':len(parts),'overlaps':overlaps,'output':str(out)},indent=2))
