"""CPU-only 1.5 mm chamber candidate, preserving the completed 2 mm fixture/settings."""
import copy,hashlib,json,subprocess
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from draw_hardware_layouts import load,section
from make_conduction_hardware import box
basepath=Path('hardware/designs/plain-bore-2mm-boolean-mesh/flow-h0050.mhd.json');base=json.loads(basepath.read_text());folder=Path('hardware/designs/plain-chamber-15mm');gltf=folder/'hot-contact.gltf';cad=json.loads((folder/'hot-contact.mhd.json').read_text());project=copy.deepcopy(base);project['geometry']=str(gltf.resolve())
for b,c in zip(project['bodies'],cad['bodies']):assert b['name']==c['name'];b['triangles']=c['triangles']
check=copy.deepcopy(project);check['geometry']=base['geometry']
for b,c in zip(check['bodies'],base['bodies']):b['triangles']=c['triangles']
assert check==base
old,_=load(base['geometry']);new,_=load(str(gltf));assert len(old)==len(new)
for a,b in zip(old[2:],new[2:]):assert np.array_equal(a[1],b[1])
x=old[0][2]+old[1][2];y=new[0][2]+new[1][2];delta=(x-y).volume()+(y-x).volume();overlap=(new[0][2]^new[1][2]).volume();clip=box([40,40,6],[0,0,3]);u=x^clip;v=y^clip;below_fill_delta=(u-v).volume()+(v-u).volume();assert below_fill_delta<1e-6 and overlap<1e-6
output=folder/'flow-h0050.mhd.json';output.write_text(json.dumps(project,indent=2)+'\n');validation=subprocess.check_output(['target/debug/examples/validate_project',str(output)],text=True)
out=Path('hardware/small-chamber-review');out.mkdir(exist_ok=True);report={'status':'Prepared CPU candidate only; GPU masks and flow pending','project':str(output),'baseline':str(basepath),'chambers_mm':[2,1.5],'liquid_volumes_mm3':[old[1][2].volume(),new[1][2].volume()],'combined_steel_liquid_difference_above_fill_expected_mm3':delta,'combined_steel_liquid_envelope_difference_below_fill_mm3':below_fill_delta,'solid_liquid_overlap_mm3':overlap,'other_body_position_payloads_identical':True,'flow_settings_identical_except_geometry_and_triangle_counts':True,'cpu_loader':validation,'intent':'Test reduced chamber diameter while preserving 0.4mm outlet/0.6mm land, outer shell, contacts, magnets, initial fill height; taper span and liquid volume change inherently. No claimed performance gain.'}
(out/'cad-review.json').write_text(json.dumps(report,indent=2)+'\n')
fig,axes=plt.subplots(1,2,figsize=(8,7));fig.subplots_adjust(top=.82,bottom=.23,wspace=.35)
for ax,parts,d in zip(axes,[old,new],[2,1.5]):section(ax,parts,'xz');ax.set(xlim=(-1.8,1.8),ylim=(-.2,6.4),title=f'{d:g} mm chamber')
fig.suptitle('Smaller chamber candidate — CAD preparation',fontsize=15)
fig.text(.5,.88,'Same exterior, 0.4 mm outlet / 0.6 mm land, contacts and magnets',ha='center',fontsize=10)
fig.text(.5,.10,'Same 6 mm fill height; smaller chamber changes liquid volume and taper span.\nBlue: initial liquid; orange: copper; hatched grey: stainless.\nCPU checks only; GPU masks and pulse performance remain untested.',ha='center',fontsize=10)
fig.savefig(out/'cad-review.png',dpi=160);plt.close(fig);print(json.dumps(report,indent=2))
