"""One-piece stainless end walls versus ideal mica confinement: CAD and DC inputs."""
import copy
import hashlib
import json
from pathlib import Path
import struct

import numpy as np
from make_conduction_hardware import box, cylinder, triangles
from make_v6_conduction import as_solid
from make_hardware import orient
from prepare_lower_coil_package import accessor_bytes


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    root = Path("artifacts/hardware-research")
    source = Path("hardware/designs/split-slit-endstop-review/flow-h010.mhd.json")
    base = json.loads(source.read_text())
    gp = source.parent / base["geometry"]
    original_doc = json.loads(gp.read_text())
    original = (gp.parent / original_doc["buffers"][0]["uri"]).read_bytes()
    assert len(base["bodies"]) == len(original_doc["meshes"]) == len(original_doc["nodes"]) == 12
    assert original_doc["meshes"][6]["name"] == "Stainless chamber tube"
    tube_index = original_doc["meshes"][6]["primitives"][0]["attributes"]["POSITION"]
    tube_points = np.frombuffer(accessor_bytes(original_doc, original, tube_index), dtype="<f4").reshape(-1, 3, 3).astype(float) * 1000
    tube = as_solid(tube_points)
    assert np.allclose(tube_points.reshape(-1,3).min(axis=0), [-1,-1,.5], atol=1e-5)
    assert np.allclose(tube_points.reshape(-1,3).max(axis=0), [1,1,10], atol=1e-5)
    # Nominal0.5mm basal flange.1um overlap into tube handles imported float
    # coordinates. Bore is through-drilled; two opposing cuts leave a0.2mm web.
    flange = cylinder(1.5, .501, [0, 0, .2505]) ^ box([.2, 3.2, .501], [0, 0, .2505])
    # A single through-bore avoids sliver faces where imported float seams
    # intersect an independently generated bore. Only the modified steel body
    # is reconstructed; the other nine payloads remain byte-identical.
    joined = (cylinder(1, 9.5, [0,0,5.25]) + flange) - cylinder(.5, 10.5, [0,0,5])
    joined = joined.set_tolerance(1e-5).simplify()
    assert len(joined.decompose()) == 1 and str(joined.status()) == "Error.NoError"
    positions = np.asarray(orient(triangles(joined))).reshape(-1, 3)
    extra = b"".join(struct.pack("<fff", *(p * .001)) for p in positions)
    doc = copy.deepcopy(original_doc)
    doc["bufferViews"].append({"buffer": 0, "byteOffset": len(original), "byteLength": len(extra)})
    doc["accessors"].append({"bufferView": len(doc["bufferViews"])-1, "componentType": 5126,
                             "count": len(positions), "type": "VEC3",
                             "min": (positions.min(axis=0)*.001).tolist(), "max": (positions.max(axis=0)*.001).tolist()})
    doc["meshes"][6]["primitives"][0]["attributes"]["POSITION"] = len(doc["accessors"])-1
    doc["meshes"][6]["name"] = doc["nodes"][6]["name"] = "Stainless chamber with integral slit end walls"
    doc["meshes"] = doc["meshes"][:10]
    doc["nodes"] = doc["nodes"][:10]
    doc["scenes"][0]["nodes"] = list(range(10))
    doc["buffers"][0].update(uri="geometry.bin", byteLength=len(original)+len(extra))
    review = Path("hardware/designs/split-slit-integral-end-review")
    review.mkdir(parents=True, exist_ok=False)
    (review / "geometry.gltf").write_text(json.dumps(doc, indent=2)+"\n")
    (review / "geometry.bin").write_bytes(original+extra)
    project = copy.deepcopy(base)
    project["geometry"] = "geometry.gltf"
    project["bodies"] = project["bodies"][:10]
    project["bodies"][6].update(name=doc["meshes"][6]["name"]+" / 0", triangles=len(positions)//3)
    checks=[]
    for i in range(10):
        if i == 6:
            continue
        a=original_doc["meshes"][i]["primitives"][0]["attributes"]["POSITION"]
        b=doc["meshes"][i]["primitives"][0]["attributes"]["POSITION"]
        old=accessor_bytes(original_doc,original,a);new=accessor_bytes(doc,original+extra,b)
        assert old==new
        other=as_solid(np.frombuffer(new,dtype="<f4").reshape(-1,3,3).astype(float)*1000)
        overlap=(joined ^ other).volume()
        assert overlap < 1e-5, (i,overlap)
        checks.append({"mesh":doc["meshes"][i]["name"],"sha256":hashlib.sha256(new).hexdigest(),"steel_overlap_mm3":overlap})
    flow = review / "flow-h010.mhd.json"
    flow.write_text(json.dumps(project,indent=2)+"\n")
    # Local current-sharing screen only: same electrode boundary for both
    # end-wall materials. Do not include lead losses or infer a drop threshold.
    folder=root/"cases/slit-end-confinement-dc-v1"
    folder.mkdir(parents=True,exist_ok=False)
    jobs=[]
    for label,p,g in [("mica",base,gp),("integral-steel",project,review/"geometry.gltf")]:
        p=copy.deepcopy(p);p["geometry"]=str(g.resolve())
        p["run"].update(fluid_enabled=False,thermal_enabled=False,prescribed_jet=None)
        p["fluid"]["surface"]["enabled"]=False
        p["electromagnetics"].update(wires=[],magnets=[],uniform=[])
        path=folder/(label+".mhd.json");path.write_text(json.dumps(p,indent=2)+"\n")
        output=root/("slit-end-confinement-dc-v1-"+label)
        jobs.append({"label":label,"project":str(path),"project_sha256":sha(path),"output":str(output),
                     "command":["target/debug/examples/conduction_probe",str(path),"30",str(output),"0","body","1e-5"]})
    record={"status":"Prepared one-piece end-wall CAD and stationary current-sharing screen; no native result",
            "source":str(source),"source_sha256":sha(source),"unchanged_mesh_payloads":checks,
            "nominal_machining":"Through-drill1mm bore; turn2mm tube above a3mm diameter basal flange; two opposing0.5mm-deep cuts leave0.2mm-wide end walls. No small drilled outlet and no loose end shims.",
            "construction_overlap_mm":.001,"joined_stainless_components":1,
            "source_tube_volume_mm3":tube.volume(),"new_stainless_volume_mm3":joined.volume(),
            "limits":["This exchanges insulating end walls for conducting stainless; current can bypass through them. Check native current sharing before flow selection.",
                      "0.2mm-wide walls are thin machining/clamping features. Strength, burrs, flatness and hot Cu/SS seam sealing are unqualified.",
                      "Smooth ideal electrical contact, fixed conductivities, neutral wetting and two-cell gap remain assumptions. Heater excluded from this local shunt screen.",
                      "No thermal, oxide, lifetime, repeated-pulse or build recommendation. A connected CAD solid does not demonstrate a hot seal."]}
    (review/"manifest.json").write_text(json.dumps(record,indent=2)+"\n")
    plan={**record,"jobs":jobs,"gates":{"true_relative_residual":1e-5,"boundary_current_defect":1e-5,"power_relative_defect":1e-4}}
    (root/"slit-end-confinement-dc-v1-plan.json").write_text(json.dumps(plan,indent=2)+"\n")
    Path("hardware/slit-end-confinement-dc-plan.json").write_text(json.dumps(plan,indent=2)+"\n")
    print(json.dumps(record,indent=2))


if __name__ == "__main__":
    main()
