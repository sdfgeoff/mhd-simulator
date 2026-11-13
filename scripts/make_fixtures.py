"""Generate original, millimetre-scale acceptance geometry; no CAD kernel required."""
import math, json, struct, pathlib
root=pathlib.Path(__file__).resolve().parents[1]
assets=root/'assets'

def revolve(profile,n=64):
    vertices=[]; faces=[]
    for r,z in profile:
        vertices.extend([(r*math.cos(i*2*math.pi/n),r*math.sin(i*2*math.pi/n),z) for i in range(n)])
    for j in range(len(profile)):
        k=(j+1)%len(profile)
        for i in range(n):
            q=(i+1)%n
            for f in [(j*n+i,k*n+i,k*n+q),(j*n+i,k*n+q,j*n+q)]:
                a,b,c=[vertices[t] for t in f]
                x=[b[t]-a[t] for t in range(3)]; y=[c[t]-a[t] for t in range(3)]
                cross=[x[1]*y[2]-x[2]*y[1],x[2]*y[0]-x[0]*y[2],x[0]*y[1]-x[1]*y[0]]
                if sum(v*v for v in cross)>1e-18:faces.extend(f)
    return vertices,faces

def box():
    v=[(x,y,z) for x in [-1,1] for y in [-1.5,1.5] for z in [-3,3]]
    return v,[0,2,3,0,3,1,4,5,7,4,7,6,0,1,5,0,5,4,2,6,7,2,7,3,0,4,6,0,6,2,1,3,7,1,7,5]

def save(name,objects):
    blob=bytearray();views=[];accessors=[];meshes=[];nodes=[]
    for label,(vertices,indices),translation in objects:
        positions=[tuple(c*.001 for c in p) for p in vertices]
        for data,target in [(b''.join(struct.pack('<3f',*p)for p in positions),34962),(struct.pack('<'+'I'*len(indices),*indices),34963)]:
            views.append({'buffer':0,'byteOffset':len(blob),'byteLength':len(data),'target':target});blob.extend(data)
        accessors.extend([{'bufferView':len(views)-2,'componentType':5126,'count':len(vertices),'type':'VEC3','min':[min(p[i]for p in positions)for i in range(3)],'max':[max(p[i]for p in positions)for i in range(3)]},{'bufferView':len(views)-1,'componentType':5125,'count':len(indices),'type':'SCALAR'}])
        meshes.append({'name':label,'primitives':[{'attributes':{'POSITION':len(accessors)-2},'indices':len(accessors)-1}]})
        nodes.append({'name':label,'mesh':len(meshes)-1,'translation':[v*.001 for v in translation]})
    doc={'asset':{'version':'2.0','generator':'MHD Workbench acceptance fixtures'},'scene':0,'scenes':[{'nodes':list(range(len(nodes)))}],'nodes':nodes,'meshes':meshes,'buffers':[{'uri':name+'.bin','byteLength':len(blob)}],'bufferViews':views,'accessors':accessors}
    (assets/(name+'.gltf')).write_text(json.dumps(doc,indent=2));(assets/(name+'.bin')).write_bytes(blob)

save('hotend',[
    ('Chamber + nozzle',revolve([(3,10),(1.5,10),(1.5,0),(.25,-2),(.8,-2),(3,0)]),(0,0,0)),
    ('Molten metal',revolve([(0,9),(1.5,9),(1.5,0),(.25,-2),(0,-2)]),(0,0,0)),
    ('Coil envelope',revolve([(3.5,7),(4.2,7),(4.2,3),(3.5,3)]),(0,0,0)),
    ('Magnet +X',box(),(6,0,5)),('Magnet -X',box(),(-6,0,5))])
save('annulus',[('Annulus',revolve([(3,4),(1.5,4),(1.5,-4),(3,-4)]),(0,0,0))])
save('open-triangle',[('Open triangle',([(0,0,0),(1,0,0),(0,1,0)],[0,1,2]),(0,0,0))])
# Exercise both mesh container formats without adding a runtime exporter.
doc=json.loads((assets/'hotend.gltf').read_text());doc['buffers'][0].pop('uri')
j=json.dumps(doc,separators=(',',':')).encode();j+=b' '*((-len(j))%4)
b=(assets/'hotend.bin').read_bytes();b+=b'\0'*((-len(b))%4)
(assets/'hotend.glb').write_bytes(struct.pack('<III',0x46546c67,2,12+8+len(j)+8+len(b))+struct.pack('<II',len(j),0x4e4f534a)+j+struct.pack('<II',len(b),0x004e4942)+b)
v,ix=revolve([(3,4),(1.5,4),(1.5,-4),(3,-4)])
stl=bytearray(b'MHD Workbench original annulus fixture'.ljust(80,b'\0'))
stl.extend(struct.pack('<I',len(ix)//3))
for i in range(0,len(ix),3):
    stl.extend(struct.pack('<12fH',0,0,0,*(c for index in ix[i:i+3]for c in v[index]),0))
(assets/'annulus.stl').write_bytes(stl)
