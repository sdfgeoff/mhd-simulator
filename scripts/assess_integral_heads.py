"""Read completed native static fields; current/force/Joule diagnostics, no field solver."""
import argparse,json,hashlib
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.colors import LogNorm
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--study',choices=['initial','near','vertical'],default='initial');args=parser.parse_args()
root=Path('artifacts/hardware-research');out=Path('hardware/integral-head-review')/({'near':'near-heater','vertical':'vertical-heater'}.get(args.study,''));cases=[]
inputs=[(k,root/f'integral-{k}-v2-cold100-h0100') for k in ['slotted-arms','side-heated']] if args.study=='initial' else [('side-heated',root/'integral-side-heated-v2-cold100-h0100'),('side-heated-near',root/'integral-side-heated-near-v1-cold100-h0100')]
if args.study=='vertical':inputs=[('side-heated-near',root/'integral-side-heated-near-v1-cold100-h0100'),('vertical-heater',root/'integral-vertical-heater-v2-cold100-h0100')]
fig,axes=plt.subplots(2,3,figsize=(14,10));fig.subplots_adjust(left=.06,right=.94,top=.87,bottom=.15,wspace=.4,hspace=.35)
for row,(kind,run) in enumerate(inputs):
    report=json.loads((run/'report.json').read_text());grid=json.loads((run/'grid.json').read_text());nx,ny,nz=grid['dims'];h=grid['cell_m'];origin=np.array(grid['origin_m']);field=np.memmap(run/'em.bin',dtype='<f4',mode='r',shape=(nz,ny,nx,16));ids=np.memmap(run/'material-ids.bin',dtype='<u4',mode='r',shape=(nz,ny,nx))
    assert report['boundary_current_defect']<=1e-5 and abs(report['power_relative_defect'])<=1e-4
    sums={};maximums={}
    for mid in [4,14,16,17,18]:
        total=0.;maximum=0.
        for k in range(nz):
            values=field[k,:,:,11][ids[k]==mid];total+=values.sum(dtype=np.float64)*h**3
            if len(values):maximum=max(maximum,float(values.max()))
        sums[str(mid)]=total;maximums[str(mid)]=maximum/1e9
    power=sum(sums.values());assert abs(power/report['joule_power_W']-1)<1e-4
    x_centres=origin[0]+(np.arange(nx)+.5)*h;plane=int(np.searchsorted(x_centres,0));assert plane<nx
    liquid_plane_current=float(field[:,:,plane,4][ids[:,:,plane]==4].sum(dtype=np.float64)*h*h)
    case={'zero_plane_cell_x_mm':float(x_centres[plane]*1000),'zero_plane_liquid_current_A':liquid_plane_current,'native_legacy_plane_cell_x_mm':float(x_centres[nx//2]*1000),'concept':kind,'source':str(run),'native':report,'joule_power_by_material_W':sums,'peak_voxel_joule_density_by_material_W_mm3':maximums,'CPU_sum_Joule_relative_difference':power/report['joule_power_W']-1,'liquid_midplane_fraction_proxy':liquid_plane_current/report['current_A'],'lateral_to_axial_force_ratio':abs(report['liquid_force_N'][0]/report['liquid_force_N'][2])};cases.append(case)
    s=np.array(field[:,ny//2,:,:]);mid=np.array(ids[:,ny//2,:]);x=(origin[0]+(np.arange(nx)+.5)*h)*1000;z=(origin[2]+(np.arange(nz)+.5)*h)*1000;extent=[origin[0]*1000,(origin[0]+nx*h)*1000,origin[2]*1000,(origin[2]+nz*h)*1000];metal=np.isin(mid,[14,16,17]);conducting=metal|(mid==4)
    for col in range(3):
        ax=axes[row,col];ax.set_facecolor('#f6f8fa');ax.contour(x,z,conducting.astype(float),levels=[.5],colors='#6c7780',linewidths=.5);ax.contour(x,z,(mid==4).astype(float),levels=[.5],colors='#216b8c',linewidths=.7)
        if col==0:
            density=np.linalg.norm(s[:,:,4:7],axis=2)/1e6;im=ax.imshow(np.ma.masked_where(~conducting,density),origin='lower',extent=extent,cmap='viridis',vmin=0,vmax=35,interpolation='nearest');label='|J| (A/mm²)'
            X,Z=np.meshgrid(x,z);stride=(slice(None,None,6),slice(None,None,6));jx=s[:,:,4];jz=s[:,:,6];norm=np.hypot(jx,jz);mask=conducting&(norm>1e5);qx=np.where(mask,jx/np.maximum(norm,1e-30),np.nan);qz=np.where(mask,jz/np.maximum(norm,1e-30),np.nan);ax.quiver(X[stride],Z[stride],qx[stride],qz[stride],color='white',angles='xy',scale_units='xy',scale=2.2,width=.003)
        elif col==1:
            im=ax.imshow(np.ma.masked_where(~conducting,s[:,:,11]/1e9),origin='lower',extent=extent,cmap='magma',norm=LogNorm(1e-4,1),interpolation='nearest');label='Joule heat at 100 A (W/mm³)'
        else:
            im=ax.imshow(np.ma.masked_where(mid!=4,s[:,:,10]*1e-6),origin='lower',extent=extent,cmap='RdBu_r',vmin=-12,vmax=12,interpolation='nearest');label='Liquid force density fz (mN/mm³)'
            X,Z=np.meshgrid(x,z);stride=(slice(None,None,4),slice(None,None,3));fx=s[:,:,8];fz=s[:,:,10];norm=np.hypot(fx,fz);mask=(mid==4)&(norm>1e3);qx=np.where(mask,fx/np.maximum(norm,1e-30),np.nan);qz=np.where(mask,fz/np.maximum(norm,1e-30),np.nan);ax.quiver(X[stride],Z[stride],qx[stride],qz[stride],color='#273b4b',angles='xy',scale_units='xy',scale=5,width=.006)
        ax.set(xlim=(-1.5,1.5) if col==2 else ((-11.5,10.5) if args.study=='vertical' else (-10.5,10.5)),ylim=(-.2,6.4) if col==2 else ((-.5,29) if args.study=='vertical' else (-.5,20)),aspect='equal',xlabel='x (mm)',ylabel='z (mm)',title=(kind.replace('-',' ').title()+': '+['current','heating','liquid force'][col]))
        fig.colorbar(im,ax=ax,shrink=.8,pad=.03,label=label)
    del field,ids
report={'status':'Both completed native100A full-body static screens; not thermal or jetting simulations','cases':cases,'limits':['One0.1mm grid; no convergence qualification.','Current sharing is a cell-centred central-plane proxy, not a unique fluid through-current.','Full stainless body and heater/sensor sheath geometry retained; isolated outer magnet occupancy partly cropped, full applied magnet sources unchanged.','Fixed hot properties, ideal continuity within one steel part; no external cables/grounds, carrier/fasteners or heater internal insulation.','Joule heat is instantaneous100A power; temperatures, pulse train heating and fluid dynamics not calculated.']};(out/'electrical-results.json').write_text(json.dumps(report,indent=2)+'\n')
fig.suptitle('One-piece stainless heads — native stationary 100 A results',fontsize=17,y=.98)
fig.text(.5,.93,('Same pool / full magnet sources · 0.1 mm cells · vertical pole gap 6 mm versus 4 mm for horizontal' if args.study=='vertical' else 'Same pool and full magnet sources · 0.1 mm cells · all current-carrying metal and heater/sensor sheaths included'),ha='center',fontsize=11)
fig.text(.5,.015,'Arrows show direction only: current in the first column, force in liquid in the third. Blue force colour means downward.\nJoule heat is instantaneous at 100 A, not a temperature. Terminal regions have the same conductivity as the stainless body.\nDifferent force-panel scale; static total forces are not pressures or demonstrated droplet trajectories.',ha='center',va='bottom',fontsize=10)
fig.savefig(out/'electrical-results.png',dpi=160);plt.close(fig)
print(json.dumps([{k:c[k] for k in ['concept','liquid_midplane_fraction_proxy','lateral_to_axial_force_ratio','CPU_sum_Joule_relative_difference']} for c in cases],indent=2))
