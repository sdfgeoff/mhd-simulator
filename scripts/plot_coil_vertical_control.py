"""Signed velocity from completed coil/control exports, with no new simulation."""
import json
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from native_field_io import read_native_fields

fig,axes=plt.subplots(1,2,figsize=(10,9))
fig.subplots_adjust(left=.08,right=.85,bottom=.23,top=.82,wspace=.25)
for ax,current in zip(axes,[0,250]):
    p=Path(f'artifacts/hardware-research/coil-low-fill-flow-v1-{current}A-h020/frame-00001800')
    m=json.loads((p/'metadata.json').read_text());nx,ny,nz=m['dims'];h=m['cell_m']*1000
    f=read_native_fields(p/'fluid.bin',(nz,ny,nx,16))
    s=np.array(f[:,(ny-1)//2:ny//2+1,:,:].mean(axis=1));del f
    x0,_,z0=np.array(m['origin_m'])*1000;x=x0+(np.arange(nx)+.5)*h;z=z0+(np.arange(nz)+.5)*h
    extent=[x0,x0+nx*h,z0,z0+nz*h];alpha=s[:,:,14];opened=s[:,:,12]
    active=(alpha>.5)&(opened>.5)
    im=ax.imshow(np.ma.masked_where(~active,s[:,:,2]),origin='lower',extent=extent,
                 cmap='RdBu_r',vmin=-.15,vmax=.15,interpolation='nearest')
    rgba=np.zeros((*active.shape,4));rgba[:,:,:3]=.68;rgba[:,:,3]=1-np.clip(opened,0,1)
    ax.imshow(rgba,origin='lower',extent=extent,interpolation='nearest')
    ax.contour(x,z,alpha,levels=[.5],colors=['#203747'],linewidths=.8)
    X,Z=np.meshgrid(x,z);u=s[:,:,0];w=s[:,:,2];speed=np.hypot(u,w)
    keep=active&(speed>.002);ii,jj=np.indices(keep.shape);keep&=(ii%4==0)&(jj%4==0)
    ax.quiver(X[keep],Z[keep],u[keep]/speed[keep],w[keep]/speed[keep],angles='xy',
              scale_units='xy',scale=2.5,width=.0035,color='#34424b')
    ax.axhline(0,color='#d17a2b',ls=':',lw=.8)
    ax.set(xlim=(-3.8,3.8),ylim=(-.7,6.7),aspect='equal',xlabel='x (mm)',ylabel='z (mm)',
           title=f'{current} A peak — {"unpowered control" if current==0 else "driven"}')
bar=fig.colorbar(im,cax=fig.add_axes([.89,.30,.025,.45]));bar.set_label('Vertical velocity (m/s): positive = UP')
fig.suptitle('The outlet retreats upward — in both runs',fontsize=16,y=.96)
fig.text(.5,.89,'Same geometry / grid / final time 2.1 ms · red = upward; blue = downward',ha='center',fontsize=11)
fig.text(.5,.14,'Colour shows signed vertical velocity, replacing the earlier speed-only view.\n'
         'Arrows show projected direction only (equal length; cells below 2 mm/s omitted).\n'
         'Two central y-cell layers averaged; grey = solid; line = liquid fraction 0.5.',ha='center',fontsize=10)
fig.text(.5,.045,'At the first outlet cell plane: upward liquid-weighted speed 0.076 m/s (0 A) and 0.091 m/s (250 A).\n'
         'This is startup/interface retraction plus a drive-dependent change, not evidence that the pulse alone sucked the pool upward.',ha='center',fontsize=10)
fig.savefig('hardware/actuator-families/coil-vertical-control.png',dpi=170)
plt.close(fig)
