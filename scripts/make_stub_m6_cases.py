"""Short M6 nozzles: retain heater interface, trade small magnet projection for a stout body."""
import argparse,copy,json,math
from pathlib import Path
from make_conduction_hardware import box,triangles
from make_hardware import emit,heater,lathe,orient
from make_v6_conduction import as_solid

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('output',type=Path)
p.add_argument('--cell-mm',type=float,default=.05)
a=p.parse_args()
if not math.isfinite(a.cell_mm) or a.cell_mm<=0:p.error('Positive finite spacing required')
a.output.mkdir(parents=True,exist_ok=False)
base=json.loads(Path('hardware/designs/slim-square07/case.mhd.json').read_text())
manifest={'status':'Generated research geometry; no performance result','source_interface':'https://robu-prod-media.s3.ap-south-1.amazonaws.com/uploads/2019/03/Schematic.pdf','cases':[]}
def mesh(s):
 return orient([tuple(tuple(float(v) for v in x) for x in t) for t in triangles(s.set_tolerance(1e-5).simplify())])
for below in [3.,3.5,4.,5.]:
 name=f'stub{below:g}'.replace('.','p');length=below+6;fill=below+2
 radius,upper,ro=1.,1.6,.2
 transition=below-(upper-radius)/math.tan(math.radians(59))
 mouth_depth=.35/math.tan(math.radians(30))
 profile=[(2.4,length,False),(3.,length-.6,False),(3.,below,False),
          (2.5,below,False),(2.5,below-.3,False),(3.5,below-.3,True),
          (3.5,.8,True),(.5,0.,False),(ro,0.,False),(ro,.6,False),
          (radius,1.4,False),(radius,transition,False),(upper,below,False),
          (upper,length-mouth_depth,False),(1.95,length,False)]
 solid=as_solid(lathe(profile))
 for sign in [-1,1]:
  solid=solid-box([20.,20.,below-.23],[0.,sign*11.4,(below-.25-.02)/2])
  solid=solid-box([20.,20.,below-.28],[sign*13.,0.,(below-.3-.02)/2])
  solid=solid-box([20.,20.,.62],[sign*10.35,0.,.29])
  solid=solid-box([20.,20.,.62],[0.,sign*10.35,.29])
 pool_profile=[(0.,fill,False),(upper,fill,False),(upper,below,False),
               (radius,transition,False),(radius,1.4,False),(ro,.6,False),(ro,0.,False),(0.,0.,False)]
 pool_faces=lathe(list(reversed(pool_profile)));pool=as_solid(pool_faces)
 if (solid^pool).volume()>1e-6:raise ValueError('Liquid overlap '+name)
 parts=[('Short M6 nozzle '+name,mesh(solid)),('Liquid solder',pool_faces)]
 contacts=[]
 for i,sign in enumerate([-1,1]):
  copper=box([4.65,.7,.4],[sign*2.675,0.,.3]);contacts.append(copper)
  if (solid^copper).volume()>1e-6 or (pool^copper).volume()>1e-6:raise ValueError('Contact collision')
  parts.append((('Positive' if i==0 else 'Negative')+' copper contact',mesh(copper)))
 magnets=[];top=below-.45
 for i,sign in enumerate([-1,1]):
  magnet=box([25.,10.,5.],[0.,sign*6.5,top-2.5]);magnets.append(magnet)
  if (solid^magnet).volume()>1e-6 or (pool^magnet).volume()>1e-6:raise ValueError('Magnet collision '+name)
  parts.append((f'Bias magnet {i+1}',mesh(magnet)))
 reference=as_solid(heater(below,below+11.5))
 overlap=sum((s^reference).volume() for s in [solid,*contacts,*magnets])
 if overlap>1e-6:raise ValueError('Heater collision '+name)
 path=a.output/(name+'.gltf');emit(path,parts,None)
 project=copy.deepcopy(base);project['geometry']=path.name
 project['domain'].update(min_mm=[-3.8,-3.8,-4.],max_mm=[3.8,3.8,length+.5],cell_mm=a.cell_mm)
 for body,(label,faces) in zip(project['bodies'],parts):body['name']=label+' / 0';body['triangles']=len(faces)
 output=path.with_suffix('.mhd.json');output.write_text(json.dumps(project,indent=2)+'\n')
 manifest['cases'].append({'label':name,'project':str(output),'heater_start_mm':below,'overall_length_mm':length,'thread_length_mm':6,'thread':'M6x1 envelope; helix omitted','chamber_diameter_mm':2.,'outlet_mm':.4,'pool_level_mm':fill,'waist_Y_mm':2.8,'wrench_X_AF_mm':6.,'pole_gap_mm':3.,'magnet_bottom_mm':top-5.,'magnet_projection_below_exit_mm':5.-top,'reference_heater_overlap_mm3':overlap,'scope':'Shorter, wider lower body with raised melt pool. Cold field/thermal screening input; no fit, torque, machining or jetting qualification.'})
(a.output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(manifest['cases'],indent=2))
