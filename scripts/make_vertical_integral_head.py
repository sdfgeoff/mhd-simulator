"""Vertical cartridge variant, preserving the one-piece current-path concept."""
import copy,json,math,sys
from pathlib import Path
import manifold3d as cad
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from make_conduction_hardware import box,cylinder,triangles
from make_v6_conduction import as_solid
from make_hardware import emit,orient,lathe
from draw_hardware_layouts import load,section
def generate():
    out=Path('hardware/designs/integral-vertical-heater-v2');out.mkdir(exist_ok=False)
    review=Path('hardware/integral-head-review/vertical-heater');review.mkdir(parents=True,exist_ok=True)
    def cy(r,h,c):return cad.Manifold.cylinder(h,r,circular_segments=96,center=True).rotate([90,0,0]).translate(c)
    t=.6+.8/math.tan(math.radians(59))
    pool=as_solid(lathe([(0,0,False),(.2,0,False),(.2,.6,False),(1,t,False),(1,6,False),(0,6,False)]))
    cavity=as_solid(lathe([(0,-1,False),(.2,-1,False),(.2,.6,False),(1,t,False),(1,15,False),(0,15,False)]))
    solid=box([3,3,14],[0,0,7])+box([20,3,2],[0,0,2.2])
    solid=solid+box([9.5,5,5.6],[-6.25,0,3.2])+box([9.5,12,22],[-6.25,0,17])
    solid=solid+box([3,3,8.8],[8.5,0,5.6])+box([3,8,6],[8.5,0,9])
    for centre_x,corner_x in [(2.5,2.0),(6.0,6.5)]:
        solid=solid+(box([1,3,1],[corner_x,0,3.7])-cy(1,3.2,[centre_x,0,4.2]))
    # Blind vertical bore: 3.2mm straight section to z2.4, plus118degree drill point.
    heater_bottom=2.6;heater_top=28.;bore_bottom=2.4
    point=1.6/math.tan(math.radians(59))
    bore=cylinder(1.6,28.1-bore_bottom,[-4,0,(28.1+bore_bottom)/2])
    bore=bore+cad.Manifold.cylinder(point,0,1.6,circular_segments=96,center=True).translate([-4,0,bore_bottom-point/2])
    solid=solid-cavity-bore-cy(.6,12.2,[-7,0,8.5])
    for sign,tz in [(-1,11),(1,9)]:
        depth=3.1 if sign<0 else 3.2
        outer=11 if sign<0 else 10
        solid=solid-cylinder(1.25,depth+.1,[sign*(outer-depth/2+.05),0,tz],'x')
        if sign<0:
            dp=1.25/math.tan(math.radians(59))
            tip=cad.Manifold.cylinder(dp,1.25,0,circular_segments=96,center=True).rotate([0,90,0]).translate([-11+depth+dp/2,0,tz])
            solid=solid-tip
    term=[solid^box([1,10,5],[x,0,tz]) for x,tz in [(-10.5,11),(9.5,9)]]
    parts=[('Integral stainless main body',solid-term[0]-term[1],14),('Liquid solder',pool,4),('Positive stainless terminal region',term[0],16),('Negative stainless terminal region',term[1],17),('Vertical heater sheath envelope',cylinder(1.55,25.4,[-4,0,15.3]),14),('Transverse sensor sheath envelope',cy(.5,20,[-7,0,8.5]),14)]
    for sign in [-1,1]:parts.append((('Front' if sign<0 else 'Rear')+' SmCo magnet stack',box([25,10,5],[0,sign*8,2.5]),18))
    assert len(solid.decompose())==1
    for i,(name,s,_) in enumerate(parts):
        assert s.status()==cad.Error.NoError and s.volume()>0,name
        for other,r,_ in parts[i+1:]:assert (s^r).volume()<1e-5,(name,other,(s^r).volume())
    meshes=[(n,orient(triangles(s))) for n,s,_ in parts];emit(out/'body.gltf',meshes,None)
    p=copy.deepcopy(json.loads(Path('hardware/designs/integral-side-heated-near-v1/electrical-h0100-compact.mhd.json').read_text()))
    p['geometry']=str((out/'body.gltf').resolve());p['domain']={'min_mm':[-11.2,-6.2,-.2],'max_mm':[10.2,6.2,28.2],'cell_mm':.1}
    p['bodies']=[]
    for (name,s,mid),(_,faces) in zip(parts,meshes):p['bodies'].append({'name':name+' / 0','triangles':len(faces),'visible':True,'simulation':{'included':True,'material_id':mid,'temperature_c':20 if mid==18 else 250,'fixed_temperature_c':None,'heat_source_w_m3':0}})
    # Full25x10x5mm source geometry retained; only isolated outer magnet/sensor occupancy is cropped.
    (out/'electrical-h0100-compact.mhd.json').write_text(json.dumps(p,indent=2)+'\n')
    report={'concept':'vertical-heater','solid_components':len(solid.decompose()),'solid_volume_mm3':solid.volume(),'pool_volume_mm3':pool.volume(),'CPU_overlaps':0,'heater_axis':'z','heater_centre_xy_mm':[-4,0],'heater_sheath_span_z_mm':[2.6,28.0],'heater_bore_radius_mm':1.6,'heater_bore_straight_bottom_z_mm':2.4,'drill_tip_bottom_z_mm':bore_bottom-point,'bottom_floor_min_mm':bore_bottom-point-.4,'inter_bore_min_wall_mm':4-1-1.6,'lower_side_wall_mm':.9,'heater_to_left_terminal_drill_point_gap_mm':(-5.6)-(-11+3.1+1.25/math.tan(math.radians(59))),'sensor_axis':'y','sensor_centre_xz_mm':[-7,8.5],'pole_gap_mm':6,'heater_source':'https://www.hotset.com/wp-content/uploads/2024/09/hotrod-hhp_09_2024_EN_web.pdf','heater_family_nominal_mm':[3.1,25.4],'unheated_tip_mm':6,'unheated_lead_end_mm':8,'nominal_winding_span_z_mm':[8.6,20.0],'pool_span_z_mm':[0,6],'central_feed_bore_top_mm':14,'outer_body_span_mm':[21,12,28],'scope':'CPU CAD only. Catalogue family envelope, not selected stock part or winding-tolerance qualification. Terminal threads, heater retention, leads, carriers and mounts not detailed. Nominal1.04mm drill floor/1.55mm terminal drill clearance need tolerance review. Cropped only isolated sheath/magnet occupancy; full connected current-carrying steel retained. PM source geometry complete. No jetting/operating-temperature claim.'}
    (out/'manifest.json').write_text(json.dumps(report,indent=2)+'\n');(review/'cad-review.json').write_text(json.dumps(report,indent=2)+'\n')
    return report
out=Path('hardware/designs/integral-vertical-heater-v2')
review=Path('hardware/integral-head-review/vertical-heater')
report=json.loads((out/'manifest.json').read_text()) if '--draw-only' in sys.argv else generate()
loaded,_=load(str(out/'body.gltf'))
fig,axes=plt.subplots(1,3,figsize=(15,10));fig.subplots_adjust(left=.05,right=.98,top=.86,bottom=.21,wspace=.33)
section(axes[0],loaded,'xz');axes[0].set(xlim=(-13,13),ylim=(-1,31),title='A. Central X-Z cut (Y = 0)')
axes[0].plot([-4,-4],[8.6,20],color='#dd4b16',lw=5,label='Catalogue nominal heated span')
axes[0].plot([-4,-4],[2.6,8.6],color='#888888',lw=3,ls='--');axes[0].plot([-4,-4],[20,28],color='#888888',lw=3,ls='--')
axes[0].annotate('Leads up',xy=(-4,28),xytext=(-11,30),arrowprops={'arrowstyle':'->'},fontsize=9)
axes[0].annotate('Melt feed\nopen at 14 mm',xy=(0,14),xytext=(3,17),arrowprops={'arrowstyle':'->'},fontsize=9)
section(axes[1],loaded,'yz');axes[1].set(xlim=(-14,14),ylim=(-1,31),title='B. Magnet / melt section (X = 0)')
axes[1].annotate('6 mm pole gap',xy=(3,3),xytext=(3.5,8),arrowprops={'arrowstyle':'->'},fontsize=9)
# A translated rendering cut through the vertical cartridge centre.
section(axes[2],loaded,'yz',height=-4);axes[2].set(xlim=(-14,14),ylim=(-1,31),title='C. Vertical heater cut (X = -4 mm)')
axes[2].plot([0,0],[8.6,20],color='#dd4b16',lw=5);axes[2].annotate('6 mm cold tip',xy=(0,5.5),xytext=(3,10),arrowprops={'arrowstyle':'->'},fontsize=9)
fig.suptitle('Vertical heater beside the bore — one-piece stainless head',fontsize=17,y=.97)
fig.text(.5,.91,'Proposed 100 A control | 2 mm chamber / 0.4 mm outlet / 0.6 mm land | 3.1 x 25.4 mm cartridge family',ha='center',fontsize=11)
fig.text(.5,.13,'Straight drilled heater and feed bores; transverse sensor; M3 terminals from the outer X faces. No copper contact assembly.\nLower heater body: 5 mm thick, with 0.9 mm side walls and 1.4 mm between bore surfaces. Pole gap increases from 4 to 6 mm.\nOrange: nominal heated span Z = 8.6–20 mm. Grey dashed: unheated ends. Pool top is Z = 6 mm: winding does not overlap the pool.\nCatalogue length tolerance and heater retention remain unresolved. CPU solid/overlap checks only; no electrical or thermal result yet.',ha='center',fontsize=10,linespacing=1.7)
fig.savefig(review/'proposed-cuts.png',dpi=160);plt.close(fig)
print(json.dumps(report,indent=2))
