"""One-piece stainless current-path concepts. Virtual terminal partitions are not joints."""
import argparse,copy,json,math
from pathlib import Path
import manifold3d as cad
from make_conduction_hardware import box,cylinder as base_cylinder,triangles
from make_v6_conduction import as_solid
from make_hardware import emit,orient,lathe
from draw_hardware_layouts import load,section
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--only',choices=['slotted-arms','side-heated','side-heated-near']);parser.add_argument('--output-root',type=Path,default=Path('hardware/designs'));parser.add_argument('--review-root',type=Path,default=Path('hardware/integral-head-review'));args=parser.parse_args()
root=args.review_root;root.mkdir(exist_ok=True,parents=True)
base=json.loads(Path('hardware/designs/plain-bore-2mm-boolean-mesh/flow-h0050.mhd.json').read_text())
def cylinder(r,h,c,axis='z'):
    if axis=='y':return cad.Manifold.cylinder(h,r,circular_segments=96,center=True).rotate([90,0,0]).translate(c)
    return base_cylinder(r,h,c,axis)
t=.6+.8/math.tan(math.radians(59))
pool=as_solid(lathe([(0,0,False),(.2,0,False),(.2,.6,False),(1,t,False),(1,6,False),(0,6,False)]))
reports=[];sources=[]
kinds=[args.only] if args.only else ['slotted-arms','side-heated']
for kind in kinds:
    near=kind=='side-heated-near'
    out=args.output_root/('integral-'+kind+'-v1');out.mkdir(exist_ok=False)
    top=19 if kind=='slotted-arms' else 14
    cavity=as_solid(lathe([(0,-1,False),(.2,-1,False),(.2,.6,False),(1,t,False),(1,top+1,False),(0,top+1,False)]))
    solid=box([3,3,top],[0,0,top/2])
    # Low transverse connections are 3mm in Y and 2mm in Z, not delicate foils.
    solid=solid+box([20,3,2],[0,0,2.2])
    if kind=='slotted-arms':
        solid=solid+box([20,12,6],[0,0,16])
        for sign in [-1,1]:
            solid=solid+box([3,3,7.8],[sign*8.5,0,5.1])
            solid=solid+box([3,8,5],[sign*8.5,0,8.5])
        heater_c=[-5,0,16];sensor_c=[-2.5,0,16]
    else:
        solid=solid+box([8.5,3,4.8],[-5.75,0,3.6])
        solid=solid+box([8.5,12,8],[-5.75,0,10])
        solid=solid+box([3,3,8.8],[8.5,0,5.6])+box([3,8,6 if near else 4],[8.5,0,9 if near else 8])
        heater_c=[-4,0,8.5] if near else [-6,0,10];sensor_c=[-7,0,8.5] if near else [-3.5,0,10]
    # R1 internal window roots, made with a 2mm end mill from Y.
    for sign in ([-1,1] if kind=='slotted-arms' else [1]):
        for centre_x,corner_x in [(2.5,2.0),(6.0,6.5)]:
            fillet=box([1,3,1],[corner_x,0,3.7])-cylinder(1,3.2,[centre_x,0,4.2],'y')
            if sign<0:fillet=fillet.mirror([1,0,0])
            solid=solid+fillet
    solid=solid-cavity-cylinder(1.6,12.2,heater_c,'y')-cylinder(.6,12.2,sensor_c,'y')
    # Blind M3 tap drill holes on outward X faces, away from heater bores.
    terminal_zs=[11.,9.] if near else [8.5,8.5]
    for sign,tz in zip([-1,1],terminal_zs):
        if near:
            # M3 nominal 3mm engagement, plus118degree blind drill point on left.
            depth=3.1 if sign<0 else 3.2
            solid=solid-cylinder(1.25,depth+.1,[sign*(10-depth/2+.05),0,tz],'x')
            if sign<0:
                point=1.25/math.tan(math.radians(59))
                drill_tip=cad.Manifold.cylinder(point,1.25,0,circular_segments=96,center=True).rotate([0,90,0]).translate([-10+depth+point/2,0,tz])
                solid=solid-drill_tip
        else:solid=solid-cylinder(1.25,2.2,[sign*9,0,tz],'x')
    terminal_regions=[]
    for sign,tz in zip([-1,1],terminal_zs):terminal_regions.append(solid^box([1,10,5],[sign*9.5,0,tz]))
    core=solid-terminal_regions[0]-terminal_regions[1]
    parts=[('Integral stainless main body',core,14),('Liquid solder',pool,4),('Positive stainless terminal region',terminal_regions[0],16),('Negative stainless terminal region',terminal_regions[1],17),('Heater sheath envelope',cylinder(1.55,20,heater_c,'y'),14),('Sensor sheath envelope',cylinder(.5,20,sensor_c,'y'),14)]
    for sign in [-1,1]:parts.append((('Front' if sign<0 else 'Rear')+' SmCo magnet stack',box([25,10,5],[0,sign*7,2.5]),18))
    overlaps=[]
    for i,(n,s,_) in enumerate(parts):
        assert s.status()==cad.Error.NoError and s.volume()>0
        for m,r,_ in parts[i+1:]:
            v=(s^r).volume()
            if v>1e-5:overlaps.append({'a':n,'b':m,'volume_mm3':v})
    if overlaps:raise ValueError(json.dumps(overlaps))
    assert len(solid.decompose())==1,'One-piece body must be connected'
    meshes=[(n,orient(triangles(s))) for n,s,_ in parts];emit(out/'body.gltf',meshes,None)
    p=copy.deepcopy(base);p['geometry']=str((out/'body.gltf').resolve());p['domain']={'min_mm':[-13,-13,-1],'max_mm':[13,13,20],'cell_mm':.1};p['bodies']=[]
    steel=next(m for m in p['materials'] if m['id']==14)
    for mid in [16,17]:
        i=next(i for i,m in enumerate(p['materials']) if m['id']==mid);m=copy.deepcopy(steel);m.update(id=mid,name=('Positive' if mid==16 else 'Negative')+' stainless virtual terminal');p['materials'][i]=m
    magnets=[]
    for i,((name,s,mid),(_,faces)) in enumerate(zip(parts,meshes)):
        p['bodies'].append({'name':name+' / 0','triangles':len(faces),'visible':True,'simulation':{'included':True,'material_id':mid,'temperature_c':20 if mid==18 else 250,'fixed_temperature_c':None,'heat_source_w_m3':0}})
        if mid==18:magnets.append({'body':i,'magnetization_a_m':[0,-755985.9796865028,0],'subdivisions':3,'smoothing_mm':.05})
    p['electromagnetics']['magnets']=magnets;(out/'electrical-h0100.mhd.json').write_text(json.dumps(p,indent=2)+'\n')
    report={'concept':kind,'folder':str(out),'solid_components':len(solid.decompose()),'solid_volume_mm3':solid.volume(),'pool_volume_mm3':pool.volume(),'CPU_overlaps':overlaps,'terminal_partition':'Virtual subregions of the SAME stainless solid, same1.1MS/m conductivity; no physical terminal joint or copper busbar','minimum_bridge_nominal_mm':[3,2],'window_root_radius_mm':1,'heater_centre_mm':heater_c,'sensor_centre_mm':sensor_c,'terminal_z_mm':terminal_zs,'outlet_mm':.4,'land_mm':.6,'chamber_mm':2,'fill_mm':6,'heater_envelope_mm':[3.1,20],'heater_bore_mm':3.2,'scope':'CPU CAD only. Complete stainless body+heater/sensor sheaths and magnet geometry. Fasteners, carrier, wires and heater internal insulation not yet defined; no assembly/thermal/jetting claim.'};(out/'manifest.json').write_text(json.dumps(report,indent=2)+'\n');reports.append(report);sources.append(out/'body.gltf')
display_sources=([Path('hardware/designs/integral-side-heated-v1/body.gltf'),*sources] if kinds==['side-heated-near'] else sources)
display_kinds=(['side-heated',*kinds] if kinds==['side-heated-near'] else kinds)
fig,axes=plt.subplots(2,len(display_sources),figsize=(6*len(display_sources),11),squeeze=False);fig.subplots_adjust(top=.88,bottom=.18,hspace=.32,wspace=.3)
labels={'slotted-arms':'1: symmetric slotted terminal arms','side-heated':'3: side-heated current bottleneck','side-heated-near':'3: heater closer to the pool'}
for col,(path,kind) in enumerate(zip(display_sources,display_kinds)):
    parts,_=load(str(path))
    for ax,plane in zip(axes[:,col],['xz','yz']):section(ax,parts,plane);ax.set(xlim=(-13,13),ylim=(-1,20),title=labels[kind] if plane=='xz' else 'Perpendicular magnet section')
    axes[0,col].annotate('100 A in',xy=(-10,11 if kind=='side-heated-near' else 8.5),xytext=(-12,11.8),fontsize=9,arrowprops={'arrowstyle':'->'})
    axes[0,col].annotate('return',xy=(10,9 if kind=='side-heated-near' else 8.5),xytext=(7,11.8),fontsize=9,arrowprops={'arrowstyle':'->'})
fig.suptitle('One-piece stainless heads — proposed machining and electrical controls',fontsize=16,y=.98)
fig.text(.5,.93,'Same 2 mm chamber / 0.4 mm outlet / 0.6 mm land / 6 mm pool height · nominal 100 A · no copper contact assembly',ha='center',fontsize=10)
fig.text(.5,.09,'Hatched grey: integral stainless (terminal regions are virtual solver partitions, not joints). Blue: melt; pale blue: SmCo.\nWindows are cut through the 3 mm thickness from Y. Heater/sensor bores are drilled from Y; current terminals tapped from X.\nCPU closed-solid/overlap checks only. Steel bypass, thermal adequacy, R1 window roots included; mounting/thermal/electrical behavior remain unqualified.',ha='center',fontsize=10)
fig.savefig(root/'proposed-cuts.png',dpi=160);plt.close(fig);(root/'cad-review.json').write_text(json.dumps(reports,indent=2)+'\n');print(json.dumps(reports,indent=2))
