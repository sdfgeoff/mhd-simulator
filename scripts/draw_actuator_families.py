"""Conceptual mechanism diagrams only: no CAD generation or numerical experiment."""
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle, Polygon, Circle, FancyArrowPatch

OUT = Path('hardware/actuator-families')
OUT.mkdir(exist_ok=True)
C = dict(liquid='#bde6f2', wall='#c6cdd3', copper='#ce8438', current='#287747',
         force='#c13932', field='#6658a6', insulator='#e5d193', text='#233542')

def box(a,x,y,w,h,color,edge='#51616a',lw=1.2):
    a.add_patch(Rectangle((x,y),w,h,facecolor=color,edgecolor=edge,lw=lw))
def arrow(a,x,y,u,v,color,style='-',width=2.2):
    a.add_patch(FancyArrowPatch((x,y),(u,v),arrowstyle='-|>',mutation_scale=15,
                              color=color,lw=width,linestyle=style,zorder=5))
def txt(a,x,y,s,size=11,ha='left',color=None,**kw):
    a.text(x,y,s,fontsize=size,ha=ha,va='center',color=color or C['text'],**kw)
def pipe(a,points,width=24):
    x,y=zip(*points);a.plot(x,y,lw=width+3,color='#51616a',solid_capstyle='butt')
    a.plot(x,y,lw=width,color=C['liquid'],solid_capstyle='butt')
def nozzle(a,x,y):
    a.add_patch(Polygon([(x-1,y),(x+1,y),(x+.25,y-.8),(x+.25,y-1.3),
                         (x-.25,y-1.3),(x-.25,y-.8)],facecolor=C['wall'],edgecolor='#51616a'))
    box(a,x-.12,y-1.3,.24,1.3,C['liquid'],lw=.5)
def drop(a,x,y):
    a.add_patch(Circle((x,y),.16,color='#3c9dbd'))
def dotfield(a,x,y):
    a.add_patch(Circle((x,y),.18,fill=False,edgecolor=C['field'],lw=1.5,zorder=5))
    a.add_patch(Circle((x,y),.045,color=C['field'],zorder=5))
def setup(a,title,subtitle):
    a.set(xlim=(0,12),ylim=(0,9));a.axis('off')
    txt(a,.1,8.65,title,16,weight='bold');txt(a,.1,8.05,subtitle,11)
def note(a,s): txt(a,.1,.48,s,10)

def single(a):
    setup(a,'1. Single-pulse induction / squeeze','One winding; no electrodes in the melt. Cross section through the axis.')
    box(a,4,2.7,4,4.4,C['wall']);box(a,4.3,2.7,3.4,4.1,C['liquid']);nozzle(a,6,2.7)
    for y in [3.4,4.25,5.1,5.95]:
        for x in [3.3,8.7]: a.add_patch(Circle((x,y),.25,facecolor=C['copper'],edgecolor='#8c5b2d'))
    arrow(a,6,3.4,6,6.1,C['field']);txt(a,6.3,5.1,'changing B',11,color=C['field'])
    arrow(a,4.5,4.3,5.4,4.3,C['force']);arrow(a,7.5,4.3,6.6,4.3,C['force'])
    txt(a,6,3.05,'squeeze → pressure',11,ha='center',color=C['force'])
    txt(a,.2,6.7,'coil pulse',11);a.plot([.3,.7,1.3,2.1,2.5],[5.8,5.8,6.35,5.8,5.8],color=C['current'],lw=2)
    txt(a,9,6.8,'induced current\nloops around axis',10)
    arrow(a,6,1.25,6,.85,C['force']);drop(a,6,1.05)
    note(a,'Shown on the rising edge. Falling-edge force can differ; ejection needs suitable fluid dynamics.')

def repel(a):
    setup(a,'2. Liquid current + nearby copper return','Opposite parallel currents repel. Rotated here so the outlet points downward.')
    box(a,2,5.3,8,.4,C['copper']);box(a,2,4.6,8,.5,C['insulator'])
    box(a,2,3.7,8,.9,C['liquid']);nozzle(a,6,3.7)
    arrow(a,8.7,5.5,3.3,5.5,C['current']);arrow(a,3.2,4.15,8.7,4.15,C['current'])
    pipe(a,[(10,4.15),(10.5,4.15),(10.5,5.5),(10,5.5)],8)
    a.plot([2,1,1,2],[4.15,4.15,5.5,5.5],color=C['current'],lw=2)
    box(a,.55,4.48,.9,.6,'white');txt(a,1,4.78,'pulse',9,ha='center')
    txt(a,6,6.2,'solid copper return',11,ha='center');txt(a,6,5.03,'electrical insulation',10,ha='center')
    txt(a,3,3.35,'liquid channel',11);arrow(a,7.5,3.65,7.5,2.65,C['force'])
    txt(a,8,2.9,'repulsion',11,color=C['force']);drop(a,6,1.8)
    note(a,'No permanent magnet or coil. Both currents reverse together: force still pushes the same way.')

def metering(a):
    setup(a,'3. Separate actuator chamber + outlet','A hydraulic layout that can use several actuators; not a new electromagnetic force.')
    box(a,.5,5.5,2.7,1.8,C['wall']);box(a,.7,5.7,2.3,1.3,C['liquid'])
    txt(a,1.85,7.6,'reservoir',11,ha='center')
    pipe(a,[(3.2,6),(4,6),(4,4.8),(5.2,4.8)],10)
    txt(a,3.8,6.8,'refill restriction',10,ha='center')
    box(a,5,3.5,4,2.5,C['wall']);box(a,5.2,3.7,3.6,2.1,C['liquid'])
    txt(a,7,5.4,'EM actuator region',11,ha='center');txt(a,7,4.7,'pressure pulse',12,ha='center',color=C['force'])
    nozzle(a,7,3.5);drop(a,7,1.7);arrow(a,7,2.1,7,1.15,C['force'])
    txt(a,9.2,3.3,'short outlet',10);txt(a,9.2,5,'current + magnets,\nreturn trace, or coil',10)
    note(a,'Restriction limits backflow during firing, but must allow refill. No pneumatic supply is implied.')

def loop(a):
    setup(a,'4. Transformer-coupled liquid loop + bias magnet','Concept only: liquid is a closed electrical secondary. Flux linkage is drawn symbolically.')
    pipe(a,[(3,6.5),(10,6.5),(10,3.6),(3,3.6),(3,6.5)],22)
    arrow(a,8,6.5,5.8,6.5,C['current']);arrow(a,4,3.6,6.3,3.6,C['current'])
    arrow(a,10,4.6,10,5.9,C['current']);arrow(a,3,5.5,3,4.2,C['current'])
    nozzle(a,7.5,3.6);drop(a,7.5,1.9)
    box(a,.4,4.8,1.5,1.3,C['wall']);txt(a,1.15,5.45,'core /\nprimary',9,ha='center')
    for y in [4.9,5.15,5.4,5.65,5.9]:box(a,.4,y,.15,.13,C['copper'])
    arrow(a,1.9,5.4,5.2,5.1,C['field'],style='--');dotfield(a,5.5,5.1)
    txt(a,6,5.1,'changing linked flux',10,color=C['field'])
    for x in [6.9,7.6,8.3]:dotfield(a,x,3.65)
    txt(a,4,2.8,'localized static B ⊙',10,color=C['field'])
    arrow(a,8.7,3.4,8.7,2.55,C['force']);txt(a,9.2,2.4,'force',10,color=C['force'])
    txt(a,6.7,7.15,'return leg outside bias field',10,ha='center')
    note(a,'No electrical terminals touching melt. Loop, localized field and transformer reset add complexity.')

def travelling(a):
    setup(a,'5. Cored travelling-field induction pump','Several phased coils move a magnetic pattern along a thin liquid passage.')
    box(a,1.3,5.65,9.2,1.0,C['wall']);box(a,1.3,3.45,9.2,.65,C['wall'])
    box(a,1.3,4.1,9.2,1.55,C['liquid']);nozzle(a,10.2,4.1)
    for i,(phase,label) in enumerate([(2,'A'),(4,'B'),(6,'C'),(8,'A')]):
        box(a,phase,5.7,.65,1.0,C['copper']);txt(a,phase+.33,6.2,label,11,ha='center')
        arrow(a,phase+.3,5.5,phase+.3,4.4,C['field']) if i%2==0 else arrow(a,phase+.3,4.4,phase+.3,5.5,C['field'])
    arrow(a,2,7.15,9,7.15,C['field'],style='--');txt(a,5.5,7.55,'travelling field pattern →',10,ha='center',color=C['field'])
    arrow(a,3,4.85,8.5,4.85,C['force']);txt(a,5.5,4.45,'liquid force / flow',11,ha='center',color=C['force'])
    txt(a,1.3,2.65,'laminated magnetic return path\n(above and below channel)',10)
    drop(a,10.2,2.25)
    note(a,'Induced currents close in 3D. Pumping pressure still needs a chamber/nozzle design for single drops.')

def ehd(a):
    setup(a,'6. Electrohydrodynamic extraction','An electric field pulls the meniscus; no magnetic pumping field.')
    box(a,4.3,5.6,3.4,1.7,C['wall']);box(a,4.6,5.6,2.8,1.4,C['liquid']);nozzle(a,6,5.6)
    txt(a,8,6.6,'high-voltage connection',10);a.plot([7.7,9,9],[6.5,6.5,7.3],color=C['field'],lw=2)
    for x in [4.5,5.2,6.8,7.5]:arrow(a,x,4.8,x,2.1,C['field'],width=1.5)
    arrow(a,6,4.2,6,2.9,C['force']);drop(a,6,3.5);drop(a,6,2.7)
    box(a,2,1.7,8,.25,C['copper']);txt(a,6,1.25,'target / counter-electrode',11,ha='center')
    txt(a,8,3.8,'electric field E',11,color=C['field']);txt(a,3.6,3.4,'pull',11,color=C['force'],ha='right')
    note(a,'Small current, high voltage. Extraction geometry and target spacing affect operation.')

for page, funcs in enumerate([(single,repel),(metering,loop),(travelling,ehd)],1):
    fig,axes=plt.subplots(2,1,figsize=(12,15))
    fig.subplots_adjust(left=.045,right=.98,top=.94,bottom=.06,hspace=.12)
    for ax,fn in zip(axes,funcs):fn(ax)
    fig.suptitle('Conductive-fluid jetting: simplified mechanisms',fontsize=19,y=.982)
    fig.text(.5,.025,'Blue fill = liquid   |   Green = current   |   Purple = field   |   Red = liquid force\nConceptual, not to scale. Walls/seals and return paths simplified; drawn drops indicate intended operation, not simulation results.',
             ha='center',fontsize=10,color=C['text'])
    fig.savefig(OUT/f'mechanisms-{page}.png',dpi=160)
    fig.savefig(OUT/f'mechanisms-{page}.svg')
    plt.close(fig)
