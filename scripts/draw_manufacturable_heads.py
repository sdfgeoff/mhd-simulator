"""Dimensioned architecture sketches, not validated CAD or shop drawings."""
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle,Polygon
out=Path('hardware/manufacturable-head-concepts')
fig,axes=plt.subplots(2,2,figsize=(13,9),gridspec_kw={'height_ratios':[2.2,1]});fig.subplots_adjust(top=.84,bottom=.22,hspace=.35,wspace=.32)
steel='#b9c2cb';copper='#d28b43';liquid='#3199c3';magnet='#a6c2db';mica='#eee1a5'
def rect(ax,x,y,w,h,color,**kw):ax.add_patch(Rectangle((x,y),w,h,facecolor=color,edgecolor='#4b5965',lw=.8,**kw))
def bore(ax,top=11):
 ax.add_patch(Polygon([(-1,top),(1,top),(1,1.08),(.2,.6),(.2,0),(-.2,0),(-.2,.6),(-1,1.08)],facecolor=liquid,edgecolor='#4b5965',lw=.8))
 ax.axhline(0,color='#677481',lw=.6,ls=':');ax.set(xlim=(-13,13),ylim=(-1.5,14),aspect='equal',xlabel='x (mm)',ylabel='z (mm)')
for col in range(2):
 ax=axes[0,col]
 if col==0:
  rect(ax,-1.5,0,3,10.2,steel);rect(ax,-3,10.2,6,2,steel)
  for sign in [-1,1]:
   x=2 if sign>0 else -10
   rect(ax,x,4.8,8,5,copper)
   rect(ax,1.5 if sign>0 else -3.5,1.3,2,2,copper)
   rect(ax,2.5 if sign>0 else -4,2.2,1.5,3,copper)
   rect(ax,4.2 if sign>0 else -8.2,6,4,1.7,'#ba5750')
  ax.text(0,13,'Turned stainless cartridge',ha='center',fontsize=10)
 else:
  rect(ax,-9,5,18,6,steel);rect(ax,-1.5,0,3,5,steel)
  rect(ax,-8,7,5,2,'#ba5750')
  for sign in [-1,1]:
   rect(ax,1.5 if sign>0 else -4.5,1.3,3,2,copper)
   rect(ax,3.5 if sign>0 else -5,2.5,1.5,2.3,copper)
   rect(ax,3.5 if sign>0 else -8,4.8,4.5,.2,mica)
  ax.text(0,11.8,'One-piece stainless heated body',ha='center',fontsize=10)
 bore(ax,12.2 if col==0 else 11);ax.set_title(('A: heated jaws + replaceable cartridge','B: integrated heater/nozzle')[col],fontsize=12,pad=32)
 ax.text(.02,.04,'Current: left jaw → melt → right jaw',transform=ax.transAxes,fontsize=9)
 ax=axes[1,col];rect(ax,-1.5,0,3,8,steel);bore(ax)
 for sign in [-1,1]:rect(ax,2 if sign>0 else -12,-.5,10,5,magnet)
 ax.annotate('',xy=(-2,2.3),xytext=(2,2.3),arrowprops={'arrowstyle':'->','lw':1.5,'color':'#465f9d'})
 ax.text(0,3.1,'B across 4 mm pole gap',ha='center',fontsize=9)
 ax.annotate('Droplet direction',xy=(0,-1.2),xytext=(6,.3),fontsize=9,arrowprops={'arrowstyle':'->'})
 ax.set(xlabel='y (mm)',ylim=(-1.5,5.5),title='Perpendicular section: common magnetic arrangement')
 ax.text(.02,.94,'Jaw blocks/supports lie outside this section',transform=ax.transAxes,fontsize=8)
fig.suptitle('Manufacturing-first direct-contact head concepts — nominal 100 A tests',fontsize=17,y=.975)
fig.text(.5,.925,'Provisional lower cartridge: Ø3 mm outside / Ø2 mm chamber / Ø0.4 mm outlet / 0.6 mm internal land',ha='center',fontsize=11)
fig.text(.5,.075,'Grey: stainless · orange: copper contacts · red: cartridge heaters · blue: melt · light blue: SmCo · pale yellow: insulation\nSections are architecture sketches. Clamp fasteners, insulating shoulder sleeves, sensor bores and magnet carriers need detailed CAD.\nLonger accessible outer stem; active contact band spans nominal z=1.3–3.3 mm. No tiny square boss. Both metal bypass paths must be simulated.',ha='center',fontsize=10)
fig.text(.5,.015,'No predicted jetting performance is assigned to either concept. Field, current sharing, heating and mechanical clearances remain to be checked.',ha='center',fontsize=10)
fig.savefig(out/'concepts.png',dpi=170);plt.close(fig)
