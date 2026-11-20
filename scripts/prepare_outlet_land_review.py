"""CPU-only checks and sections of internal-land candidates; no solver dispatch."""
import json
import numpy as np
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from draw_hardware_layouts import load,section

out=Path('hardware/outlet-land-review');out.mkdir(exist_ok=True)
folders=[Path(f'artifacts/hardware-research/cases/plain-land-cpu-v1-{v}') for v in ['060','090','120']]
old=Path('artifacts/hardware-research/cases/plain-bore-v4-2mm-source')
for suffix in ['gltf','bin']:
    assert (old/f'stub4.{suffix}').read_bytes()==(folders[0]/f'stub4.{suffix}').read_bytes(), 'Default regeneration changed completed baseline'
parts=[load(str(p/'stub4.gltf'))[0] for p in folders]
baseline_envelope=parts[0][0][2]+parts[0][1][2]
report={'status':'CPU CAD preparation only; no new GPU voxel check or fluid run', 'default_geometry_byte_identical_to_completed_source':True,'candidates':[]}
fig,axes=plt.subplots(1,3,figsize=(11,6));fig.subplots_adjust(top=.79,bottom=.23,wspace=.28)
for p,items,ax in zip(folders,parts,axes):
    m=json.loads((p/'manifest.json').read_text())['cases'][0];land=m['outlet_land_mm']
    overlap=(items[0][2]^items[1][2]).volume();envelope=items[0][2]+items[1][2];envelope_difference=(envelope-baseline_envelope).volume()+(baseline_envelope-envelope).volume()
    assert overlap<1e-6 and envelope_difference<1e-6
    for a,b in zip(parts[0][2:],items[2:]):assert np.array_equal(a[1],b[1]), 'Non-bore body changed'
    report['candidates'].append({'land_mm':land,'source':str(p),'solid_liquid_overlap_mm3':overlap,'solid_plus_liquid_envelope_difference_mm3':envelope_difference,'liquid_volume_mm3':items[1][2].volume(),'nozzle_volume_mm3':items[0][2].volume(),'other_body_triangle_payloads_identical':True})
    section(ax,items,'xz');ax.set(xlim=(-1.25,1.25),ylim=(-.15,2.5),title=f'{land:.1f} mm internal land');ax.axhline(land,color='#9a5831',ls=':',lw=.8)
fig.suptitle('Longer narrow outlet passages — CAD preparation',fontsize=16)
fig.text(.5,.85,'Same 2 mm chamber, 0.4 mm outlet, outside shape and contact heights',ha='center')
fig.text(.5,.10,'Only the internal land and taper position change. The taper moves upward; initial liquid volume decreases.\nBlue: initial liquid; orange: copper contacts; hatched grey: stainless. Dotted line: end of narrow bore.',ha='center',fontsize=10)
fig.text(.5,.035,'CPU Boolean checks only. GPU masks/overlaps must be checked after the current sequence finishes; no jetting result yet.',ha='center',fontsize=9)
fig.savefig(out/'cad-review.png',dpi=160);plt.close(fig)
(out/'cad-review.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
