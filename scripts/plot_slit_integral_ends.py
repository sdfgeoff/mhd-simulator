"""Exact lower-head CAD sections; no simulated field or droplet is shown."""
import json
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.collections import LineCollection
from matplotlib.patches import Polygon
from prepare_lower_coil_package import accessor_bytes, section_lines
from make_v6_conduction import as_solid

path=Path("hardware/designs/split-slit-integral-end-review/geometry.gltf")
doc=json.loads(path.read_text());data=(path.parent/doc["buffers"][0]["uri"]).read_bytes()
fig,axes=plt.subplots(1,2,figsize=(10.5,5.8),constrained_layout=True)
for index,color,label in [(6,"#555555","Stainless chamber + end walls"),(2,"#bd6d20","Copper electrodes"),(3,"#bd6d20",None),(1,"#197db0","Initial liquid")]:
    a=doc["meshes"][index]["primitives"][0]["attributes"]["POSITION"]
    pts=np.frombuffer(accessor_bytes(doc,data,a),dtype="<f4").reshape(-1,3,3).astype(float)*1000
    for ax,solid_points,height in [(axes[0],pts,.25),(axes[1],pts[:,:,[1,2,0]],0)]:
        contours=as_solid(solid_points).slice(height).to_polygons()
        for contour in contours:
            ax.add_patch(Polygon(contour,closed=True,facecolor=color,edgecolor="none",alpha=.4))
    for ax,order,offset in [(axes[0],[0,2,1],.25),(axes[1],[1,0,2],0)]:
        transformed=pts[:,:,order].copy();transformed[:,:,1]-=offset
        ax.add_collection(LineCollection(section_lines(transformed),colors=color,linewidths=2,label=label if ax is axes[0] else None))
axes[0].set(xlim=(-1.8,1.8),ylim=(-1.8,1.8),xlabel="x (mm)",ylabel="y (mm)",title="Outlet cross-section · z = 0.25 mm")
axes[1].set(xlim=(-1.8,1.8),ylim=(-.3,2.1),xlabel="y (mm)",ylabel="z (mm)",title="Centre between electrodes · x = 0")
for ax in axes:
    ax.set_aspect("equal");ax.grid(alpha=.15)
axes[0].annotate("Integral end wall",xy=(0,1),xytext=(-1.65,1.5),fontsize=10,arrowprops={"arrowstyle":"->"})
axes[0].annotate("0.2 mm gap",xy=(.1,0),xytext=(.35,-1.1),fontsize=10,arrowprops={"arrowstyle":"->"})
axes[1].axhline(0,color="#999999",linestyle=":",linewidth=.7)
axes[1].annotate("Nominal 0.5 mm\nbasal flange",xy=(1.25,.25),xytext=(.35,1.1),fontsize=10,arrowprops={"arrowstyle":"->"})
axes[0].legend(loc="lower center",bbox_to_anchor=(.5,-.32),frameon=False,fontsize=9)
fig.suptitle("Slit with integral stainless end walls\nOne chamber part replaces the tube and two loose end shims",fontsize=14)
fig.supxlabel("Prepared CAD only: no electrical or flow result yet. Thin walls, current bypass and hot seams remain unqualified.\nView crops the unchanged upper holder, magnets and extended electrode leads.",fontsize=9)
output=Path("artifacts/hardware-research/diagrams/slit-integral-end-cad.png")
fig.savefig(output,dpi=160)
print(output)
