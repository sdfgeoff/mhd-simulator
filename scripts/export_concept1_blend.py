"""Run inside Blenderbackground; create a developedcopy, do not editConcept1.blend."""
import bpy,json,struct
from pathlib import Path
root=Path('/home/geoffrey/Reference/OffTopic/mhd-hardware-study/hardware/designs/concept1-developed-v1');g=json.loads((root/'assembly.gltf').read_text());p=json.loads((root/'assembly-h0100.mhd.json').read_text());buffers=[(root/b['uri']).read_bytes() for b in g['buffers']]
bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
colors={4:(.03,.45,.65,1),14:(.43,.49,.55,1),16:(.7,.32,.08,1),17:(.7,.32,.08,1),20:(.7,.32,.08,1),19:(.85,.69,.25,1),21:(.92,.87,.7,1),18:(.22,.43,.65,1)}
materials={}
for mid,color in colors.items():
 m=bpy.data.materials.new('Material '+str(mid));m.diffuse_color=color;materials[mid]=m
for i,m in enumerate(g['meshes']):
 primitive=m['primitives'][0];a=g['accessors'][primitive['attributes']['POSITION']];view=g['bufferViews'][a['bufferView']];offset=view.get('byteOffset',0)+a.get('byteOffset',0);flat=struct.unpack_from('<'+'f'*(a['count']*3),buffers[view['buffer']],offset);verts=[tuple(v*10000 for v in flat[j:j+3]) for j in range(0,len(flat),3)];faces=[tuple(range(j,j+3)) for j in range(0,len(verts),3)]
 mesh=bpy.data.meshes.new(m['name']);mesh.from_pydata(verts,[],faces);mesh.update();o=bpy.data.objects.new(m['name'],mesh);bpy.context.collection.objects.link(o);mid=p['bodies'][i]['simulation']['material_id'];o.data.materials.append(materials[mid]);o['simulation_material_id']=mid
scene=bpy.context.scene;scene.unit_settings.system='METRIC';scene.unit_settings.scale_length=.0001;scene.unit_settings.length_unit='MILLIMETERS';scene['source']='Derived from Concept1.blend, original unchanged';scene['scale']='1 Blenderunit=0.1mm';scene['status']='Developed researchCAD, not qualifiedmanufacturingdrawing'
for screen in bpy.data.screens:
 for area in screen.areas:
  if area.type=='VIEW_3D':
   area.spaces.active.shading.color_type='MATERIAL';area.spaces.active.region_3d.view_distance=350;area.spaces.active.region_3d.view_location=(0,0,60)
bpy.ops.wm.save_as_mainfile(filepath=str(root/'Concept1-developed.blend'),compress=True)
print(root/'Concept1-developed.blend')
