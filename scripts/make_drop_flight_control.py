"""Generate an isolated SnCu drop for native capillary/gravity flight checks."""
import argparse
import copy
import json
import math
from pathlib import Path

from make_hardware import emit, lathe

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('output',type=Path)
p.add_argument('--cell-mm',type=float,default=.1)
a=p.parse_args()
if a.output.exists():raise FileExistsError(a.output)
a.output.mkdir(parents=True)
profile=[(0.,.3,False)]+[(.3*math.sin(math.pi*i/64),.3*math.cos(math.pi*i/64),False) for i in range(1,64)]+[(0.,-.3,False)]
triangles=lathe(profile)
emit(a.output/'drop.gltf',[('Isolated solder drop',triangles)],None)
source=Path('hardware/designs/v6-flat-shortbar/case.mhd.json')
base=json.loads(source.read_text())
base['geometry']='drop.gltf'
base['cad_curves']=[]
base['bodies']=[{'name':'Isolated solder drop / 0','triangles':len(triangles),'visible':True,
                 'simulation':{'included':True,'material_id':4,'temperature_c':250.,'fixed_temperature_c':None,'heat_source_w_m3':0.}}]
base['domain']={'min_mm':[-2.,-2.,-3.],'max_mm':[2.,2.,3.],'cell_mm':a.cell_mm}
base['electromagnetics'].update(wires=[],magnets=[],uniform=[])
base['run'].update(fluid_enabled=True,thermal_enabled=False,prescribed_jet=None,dt_s=2e-6)
base['fluid'].update(surface={'enabled':True},initial_velocity_m_s=[0.,0.,0.],periodic=[False,False,False],advection=True)
for label,g in [('zeroG',0.),('gravity',-9.81),('translation-periodic',0.),('translation-periodic-nearzero-sigma',0.),('translation-periodic-pressure-tight',0.)]:
 d=copy.deepcopy(base);d['fluid']['acceleration_m_s2']=[0.,0.,g]
 if label.startswith('translation-periodic'):
  d['fluid']['periodic']=[True,True,True];d['fluid']['initial_velocity_m_s']=[0.,0.,-.2]
 if label.endswith('pressure-tight'):
  d['fluid']['relative_tolerance']=1e-6;d['fluid']['absolute_divergence_tolerance']=1e-6
 if label.endswith('nearzero-sigma'):
  next(m for m in d['materials'] if m['id']==4)['surface_tension_n_m']=1e-12
 (a.output/(label+'.mhd.json')).write_text(json.dumps(d,indent=2)+'\n')
(a.output/'scope.json').write_text(json.dumps({'source':str(source),'purpose':'Native isolateddrop atrest, zeroG/gravity matchedcases; diagnose unexpected detached-drop deceleration in hardware runs. Noelectromagnetics or thermal evolution.','diameter_mm':.6,'initial_velocity_m_s':[0,0,0],'grid_mm':a.cell_mm,'domain_mm':[[-2,-2,-3],[2,2,3]],'reference':'COM should remainstationary forzeroG, and accelerate approximately withgravity minus small buoyancy/drag forgravity. Actual initial voxel mass/COM must be measured, not replaced with exactCADsphere values.','limits':'Finite boundaries, staircased initial interface and capillary discretization still require assessment. A6mstrial isdiagnostic, not jettervalidation.'},indent=2)+'\n')
