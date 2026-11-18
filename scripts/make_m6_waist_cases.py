"""Custom lower M6 nozzle bodies: chamber/waist/pole-gap screening family."""
import argparse
import copy
import hashlib
import json
import math
from pathlib import Path

from make_conduction_hardware import box, triangles
from make_hardware import emit, heater, lathe, orient
from make_v6_conduction import as_solid

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('output', type=Path)
parser.add_argument('--cell-mm', type=float, default=.05)
parser.add_argument('--fill-height-mm', type=float, default=3.5)
parser.add_argument('--released-interface', action='store_true')
parser.add_argument('--wrench-collar', action='store_true')
parser.add_argument('--magnet-stack', type=int, choices=[1,2,3], default=2)
parser.add_argument('--observation-window', action='store_true')
parser.add_argument('--magnet-subdivisions', type=int, choices=range(5), default=3)
parser.add_argument('--only', nargs='+')
parser.add_argument('--heater', action='store_true')
parser.add_argument('--full-domain', action='store_true')
args = parser.parse_args()
if not math.isfinite(args.cell_mm) or args.cell_mm <= 0:
    parser.error('Require positive finite spacing')
source = Path('hardware/designs/slim-square07/case.mhd.json')
base = json.loads(source.read_text())
args.output.mkdir(parents=True, exist_ok=False)
manifest = {'source_project': str(source), 'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
            'cell_mm': args.cell_mm, 'magnet_subdivisions':args.magnet_subdivisions, 'interface': 'M6x1 outer envelope from V6 drawing approximation, z6.5..12.5 mm retained. Helical thread is not modeled; final machining drawing required.',
            'shared': {'outlet_mm': .4, 'boss_square_mm': .7, 'outlet_land_mm': .6,
                       'internal_funnel_half_angle_deg': 45, 'free_level_mm':args.fill_height_mm,
                       'waist_cut_top_mm':5.35 if args.wrench_collar else 5.4, 'magnet_bottom_mm': .1,
                       'pole_height_mm': 5, 'pole_x_width_mm': 25, 'pole_y_depth_mm': 5*args.magnet_stack,
                       'pole_clearance_each_side_mm': .1}, 'cases': []}

def mesh(solid):
    return orient([tuple(tuple(float(v) for v in p) for p in t)
                   for t in triangles(solid.set_tolerance(1e-5).simplify())])

for label, diameter, wall in [('ch32-w04',3.2,.4), ('ch20-w04',2.,.4),
                              ('ch12-w04',1.2,.4), ('ch40-w04',4.,.4),
                              ('ch20-w02',2.,.2), ('ch12-w02',1.2,.2)]:
    if args.only and label not in args.only:
        continue
    ro, radius = .2, diameter/2
    upper = max(1.6, radius)
    cone_top = .6 + radius - ro
    transition = 6.5 - max(0, upper-radius)/math.tan(math.radians(59))
    chamfer = .3 if diameter <= 3.2 else 0.
    mouth = upper+chamfer
    chamfer_depth=chamfer
    if args.released_interface:
        mouth=1.95 if upper==1.6 else upper
        chamfer_depth=(mouth-upper)/math.tan(math.radians(30))
    outer = [(3.,12.5,False),(3.,6.5,False),(2.5,6.5,False),(2.5,5.,False),
             (3.5,5.,True),(3.5,2.,True),(.5+2*math.tan(math.radians(35)),2.,False),
             (.5,0.,False),(ro,0.,False),(ro,.6,False),(radius,cone_top,False),
             (radius,transition,False),(upper,6.5,False),(upper,12.5-chamfer_depth,False)]
    if mouth>upper:
        outer.append((mouth,12.5,False))
    if args.released_interface:
        outer[0:1]=[(2.4,12.5,False),(3.,11.9,False)]
    if args.wrench_collar:
        index=outer.index((2.5,5.,False))
        outer[index:index]=[(2.5,6.2,False),(3.,6.2,True),(3.,5.35,True),(2.5,5.35,False)]
    solid = as_solid(lathe(outer))
    waist = diameter + 2*wall
    gap = waist + .2
    waist_top=5.35 if args.wrench_collar else 5.4
    for sign in [-1,1]:
        solid = solid - box([20.,20.,waist_top-.6], [0.,sign*(10+waist/2),(waist_top+.6)/2])
        solid = solid - box([20.,20.,.62], [sign*10.35,0.,.29])
        solid = solid - box([20.,20.,.62], [0.,sign*10.35,.29])
    fill=args.fill_height_mm
    if not math.isfinite(fill) or not cone_top<fill<12.5-chamfer_depth:
        raise ValueError('Fill must be above the funnel and below the inlet mouth')
    bore=[(ro,0.),(ro,.6),(radius,cone_top),(radius,transition),(upper,6.5),(upper,12.5-chamfer_depth)]
    for (r0,z0),(r1,z1) in zip(bore,bore[1:]):
        if z0<=fill<=z1 and z1>z0:
            free_radius=r0+(r1-r0)*(fill-z0)/(z1-z0);break
    pool_profile=[(0.,fill,False),(free_radius,fill,False)]
    pool_profile += [(r,z,False) for r,z in reversed(bore) if z<fill]
    pool_profile.append((0.,0.,False))
    pool_faces = lathe(list(reversed(pool_profile)))
    pool = as_solid(pool_faces)
    if (solid ^ pool).volume() > 1e-6:
        raise ValueError('Nozzle/liquid overlap: '+label)
    parts = [('Custom M6 lower body '+label, mesh(solid)),('Liquid solder',pool_faces)]
    contacts=[]
    for i,sign in enumerate([-1,1]):
        pad=box([4.65,.7,.4],[sign*2.675,0.,.3]);contacts.append(pad)
        if (solid ^ pad).volume()>1e-6 or (pool ^ pad).volume()>1e-6:
            raise ValueError('Contact overlap')
        parts.append((('Positive' if i==0 else 'Negative')+' copper contact',mesh(pad)))
    magnets=[]
    for i,sign in enumerate([-1,1]):
        depth=5.*args.magnet_stack
        magnet=box([25.,depth,5.],[0.,sign*(gap/2+depth/2),2.6]);magnets.append(magnet)
        if (solid ^ magnet).volume()>1e-6 or (pool ^ magnet).volume()>1e-6:
            raise ValueError('Magnet overlap')
        parts.append((f'Bias magnet {i+1}',mesh(magnet)))
    reference_heater=as_solid(heater(6.5,18.))
    heater_overlap=sum((s ^ reference_heater).volume() for s in [solid,*contacts,*magnets])
    if heater_overlap>1e-6:raise ValueError('Heater collision')
    if args.heater:
        parts.append(('Conductive aluminium heater', mesh(reference_heater)))
    path=args.output/(label+'.gltf');emit(path,parts,None)
    project=copy.deepcopy(base);project['geometry']=path.name;project['domain']['cell_mm']=args.cell_mm
    if args.full_domain:
        project['domain'].update(min_mm=[-13.,-12.,-2.], max_mm=[13.,12.,18.5])
    if args.observation_window:
        project['domain'].update(min_mm=[-.25,-.25,0.],max_mm=[.25,.25,.6])
    if args.heater:
        project['bodies'].append({'name':'Conductive aluminium heater / 0','triangles':len(parts[-1][1]),'visible':True,'simulation':{'included':True,'material_id':8,'temperature_c':250.,'fixed_temperature_c':None,'heat_source_w_m3':0.}})
        next(m for m in project['materials'] if m['id']==8)['conductivity_s_m']=13e6
    for magnet in project['electromagnetics']['magnets']:
        magnet['subdivisions']=args.magnet_subdivisions
    for body,(name,faces) in zip(project['bodies'],parts):
        body['name']=name+' / 0';body['triangles']=len(faces)
    output=path.with_suffix('.mhd.json');output.write_text(json.dumps(project,indent=2)+'\n')
    manifest['cases'].append({'label':label,'project':str(output),'chamber_diameter_mm':diameter,
                              'upper_feed_diameter_mm':2*upper,'released_top_chamfers':args.released_interface,'wrench_AF_mm':6. if args.wrench_collar else None,'wrench_band_z_mm':[5.35,6.2] if args.wrench_collar else None,'magnets_total':2*args.magnet_stack,'observation_window_only':args.observation_window,'conductive_heater_included':args.heater,'full_domain':args.full_domain,'waist_width_mm':waist,
                              'waist_wall_mm':wall,'pole_gap_mm':gap,
                              'nominal_boss_wall_mm':.15,'reference_heater_overlap_mm3':heater_overlap,
                              'nominal_liquid_volume_mm3':pool.volume(),
                              'scope':'Whole-layout comparison; chamber, waist and pole gap vary together. Funnel and raised magnets differ from the previous slim-tip candidate. Threads/tool radii/clamps not detailed.'})
(args.output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(manifest['cases'],indent=2))
