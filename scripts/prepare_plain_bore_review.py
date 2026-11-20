"""Prepare the requested bore pair and a CAD review; never launch a solver."""
import copy
import hashlib
import json
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from draw_hardware_layouts import load, section

baseline=Path('artifacts/hardware-research/cases/m6-rooted08-flow-compactgas-h0050.mhd.json')
base=json.loads(baseline.read_text())
out=Path('hardware/plain-bore-review');out.mkdir(exist_ok=True)
geometries=[];projects=[];mesh_hashes=[]
for bore in [2,3]:
    folder=Path(f'hardware/designs/plain-bore-{bore}mm')
    gltf=folder/'hot-contact.gltf';g=json.loads(gltf.read_text())
    b=(folder/g['buffers'][0]['uri']).read_bytes()
    hashes=[]
    for mesh in g['meshes']:
        a=g['accessors'][mesh['primitives'][0]['attributes']['POSITION']]
        v=g['bufferViews'][a['bufferView']];start=v.get('byteOffset',0)+a.get('byteOffset',0)
        hashes.append(hashlib.sha256(b[start:start+a['count']*12]).hexdigest())
    mesh_hashes.append(hashes)
    j=copy.deepcopy(base);j['geometry']=str(gltf.resolve())
    cad_project=json.loads((folder/'hot-contact.mhd.json').read_text())
    for target,source in zip(j['bodies'],cad_project['bodies']):
        assert target['name']==source['name']
        target['triangles']=source['triangles']
    path=folder/'flow-h0050.mhd.json'
    if path.exists() and json.loads(path.read_text())!=j:raise ValueError('Existing project differs')
    path.write_text(json.dumps(j,indent=2)+'\n');projects.append(j);geometries.append(gltf)
assert mesh_hashes[0][2:]==mesh_hashes[1][2:], 'Non-bore CAD changed between pair'
a,b=map(copy.deepcopy,projects)
for j in [a,b]:
    j.pop('geometry')
    for body in j['bodies'][:2]:body.pop('triangles')
assert a==b,'Unexpected configuration difference'
manifest={'status':'CAD prepared for user review; no GPU simulation started',
          'requested_by':[797,798,799,801], 'topic':25,
          'baseline_project':str(baseline),'cell_um':50,'fill_height_mm':6,
          'bores_mm':[2,3],'shared_neck_width_mm':3.4,'shared_pole_gap_mm':4,
          'air_clearance_per_magnet_mm':.3,'outlet_mm':.4,'outlet_land_mm':.6,
          'forward_A':110,'forward_us':250,'reverse_A':27.5,'reverse_us':250/3,
          'gap_us':25,'dt_us':1.5,'steps':2400,'end_ms':3.6,
          'non_bore_mesh_payloads_identical':True,'configuration_identical_except_geometry_and_two_triangle_counts':True,
          'heater_excluded_from_flow':True,
          'limits':['New shared wider neck and pole gap differ from historical110A candidate.',
                    'Same fill height, not equal molten volume.',
                    'Drill taper length changes with bore diameter at fixed118degree drill point.',
                    'Exterior and magnets are common between the requested pair; old narrow-neck result is not a single-variable control.',
                    'All other research assumptions retained: ideal interfaces, static applied field, no thermal evolution.']}
(out/'cad-review.json').write_text(json.dumps(manifest,indent=2)+'\n')
fig,axes=plt.subplots(2,3,figsize=(13,11))
fig.subplots_adjust(left=.06,right=.97,top=.85,bottom=.15,hspace=.48,wspace=.3)
sources=[Path('hardware/designs/rooted08-hot-contact-m6/hot-contact.gltf'),*geometries]
for k,(source,label) in enumerate(zip(sources,['Previous stepped bore','Proposed straight 2 mm','Proposed straight 3 mm'])):
    parts,_=load(str(source));section(axes[0,k],parts,'xz');section(axes[1,k],parts,'yz')
    axes[0,k].set(xlim=(-3.6,3.6),ylim=(-.4,10.5),title=label)
    axes[1,k].set(xlim=(-13,13),ylim=(-3,11.5),title='Magnet plane (perpendicular)')
    for ax in axes[:,k]:ax.axhline(0,color='#d17a2b',ls=':',lw=.7)
    axes[0,k].text(.5,-.19, '3.2 → 2 mm chamber\n2.8 mm neck; 3.4 mm pole gap' if k==0 else
                   f'{k+1} mm chamber throughout\n3.4 mm neck; 4.0 mm pole gap',
                   transform=axes[0,k].transAxes,ha='center',va='top',fontsize=9)
fig.suptitle('CAD review before simulation: remove the internal chamber step',fontsize=17,y=.97)
fig.text(.5,.91,'Both proposals retain the 0.4 mm outlet, 0.6 mm land, 118° drill taper, tip contacts and M6 mounting envelope.',ha='center',fontsize=10)
fig.text(.5,.075,'Blue = initial liquid; orange = copper contacts; light blue = SmCo magnets; hatched grey = nozzle / heater.\n'
         'Same 6 mm fill height; liquid volume differs. Full CAD includes the heater; flow projects exclude it as before.',ha='center',fontsize=10)
fig.text(.5,.025,'No simulation has started. Neck and pole spacing were widened for both proposals so the 3 mm bore retains 0.2 mm minimum side wall.',ha='center',fontsize=10)
fig.savefig(out/'cad-comparison.png',dpi=170)
plt.close(fig)
