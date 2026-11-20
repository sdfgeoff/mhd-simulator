"""Copy completed flow settings onto longer-land CAD; CPU validation only."""
import copy,hashlib,json,subprocess
from pathlib import Path
baseline=Path('hardware/designs/plain-bore-2mm-boolean-mesh/flow-h0050.mhd.json')
base=json.loads(baseline.read_text())
def payloads(path):
    g=json.loads(path.read_text());buffers=[(path.parent/b['uri']).read_bytes() for b in g['buffers']];h=[]
    for mesh in g['meshes']:
        a=g['accessors'][mesh['primitives'][0]['attributes']['POSITION']];v=g['bufferViews'][a['bufferView']];start=v.get('byteOffset',0)+a.get('byteOffset',0)
        h.append(hashlib.sha256(buffers[v['buffer']][start:start+a['count']*12]).hexdigest())
    return h
old=payloads(Path(base['geometry']));report={'status':'Full fixtures and CPU loader validated; GPU masks NOT checked, no flow run','baseline':str(baseline),'cases':[]}
for suffix in ['090','120']:
    folder=Path(f'hardware/designs/plain-outlet-land-{suffix}');geometry=folder/'hot-contact.gltf';cad=json.loads((folder/'hot-contact.mhd.json').read_text());p=copy.deepcopy(base);p['geometry']=str(geometry.resolve())
    assert len(p['bodies'])==len(cad['bodies'])
    for b,c in zip(p['bodies'],cad['bodies']):
        assert b['name']==c['name'];b['triangles']=c['triangles']
    assert payloads(geometry)[2:]==old[2:], 'Contact/magnet/heater fixture changed'
    check=copy.deepcopy(p);check['geometry']=base['geometry']
    for b,c in zip(check['bodies'],base['bodies']):b['triangles']=c['triangles']
    assert check==base, 'Flow settings changed'
    output=folder/'flow-h0050.mhd.json';output.write_text(json.dumps(p,indent=2)+'\n')
    r=subprocess.run(['target/debug/examples/validate_project',str(output)],capture_output=True,text=True,check=True)
    report['cases'].append({'land_mm':int(suffix)/100,'project':str(output),'geometry':str(geometry),'all_non_nozzle_liquid_position_payloads_identical_to_baseline':True,'settings_identical_except_geometry_and_triangle_counts':True,'cpu_loader_output':r.stdout,'gpu_preflight':'pending; do not launch before checking actual masks'})
Path('hardware/outlet-land-review/project-preparation.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
