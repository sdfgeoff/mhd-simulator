"""Static source-traced activity diagrams. Does not import/run the solver."""
from pathlib import Path
import json,textwrap
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import FancyBboxPatch,FancyArrowPatch
R=Path(__file__).parent
COLS={'cpu':(.45,4.5),'gpu':(5.65,7.1),'data':(13.5,8.0),'disk':(22.25,3.2)}
C={'cpu':'#fff3c9','gpu':'#e8f0ff','data':'#edf7f0','disk':'#f0eafa'}
BLUE='#2563eb';ORANGE='#c45a14';GREEN='#16806a';GRAY='#64748b'
def row(cpu='',gpu='',data='',disk='',tx='',direction='h2d',kind='data'):return dict(cpu=cpu,gpu=gpu,data=data,disk=disk,tx=tx,direction=direction,kind=kind)
pages=[]
def page(stem,title,subtitle,rows,notes,loops=[]):pages.append(dict(stem=stem,title=title,subtitle=subtitle,rows=rows,notes=notes,loops=loops))
page('01-timestep-activity','1  Full timestep — activity, residency and transfers','Stopped direct-contact V5 path • read top to bottom • all arrays stay on GPU unless an arrow explicitly says otherwise',[
row('Host clock t, Δt and pulse\nIcmd = pulse(t+Δt/2)\n(time f64, Icmd f32)','Configure unit-voltage basis','Resident at start: B₀; σ/flags; potential predictors;\nMAC velocity; fluid packet; α/curvature; masks',tx='H→D: 80 B DC uniform'),
row('Residual / current checks\n(small mapped readbacks)','UNIT electrical solve → detail 2\nupdate_phase → initial_guess → PCG','R: accepted α, σ, terminal flags, predictors\nW: σ, DC work, partial reductions, scalar buffer',tx='D→H: 64 B / check',direction='d2h'),
row('Read unit-basis port currents','collect_fields(1 V) → fields_reduce\nsave_unit','R: B₀, solved potential, σ\nW: 64N DC packet; unit high/low predictor',tx='D→H: 64 B scalar packet',direction='d2h'),
row('Configure motional basis;\nthen residual/current checks','MOTIONAL electrical solve → detail 2\nupdate_phase again → initial_guess → PCG','R: same α/σ matrix, B₀, old fluid u\nW: DC work / motion predictor / reductions',tx='H→D config; D→H checks',direction='both'),
row('Read motional-basis port currents','collect_fields(0 V) → fields_reduce\nsave_motion','W: 64N DC packet AGAIN; motion predictor\nUnit / motion packets will be overwritten.',tx='D→H: 64 B scalar packet',direction='d2h'),
row('V = (Icmd − Im) / Gunit','collect_fields(V) → fields_reduce','W: final 64N packet = B, J, J×B, Q, applied B\nThis allocation is ALIASED as fluid force input.',tx='H→D: 80 B uniform; D→H: 64 B',direction='both'),
row('Get force/speed/validity bound','update_force: fields → diagnostics\nNo full force-buffer copy (shared handle).','R: DC force packet, MAC velocity, phase\nW: 64N fluid packet + diagnostic reductions',tx='D→H: 48 B diagnostics',direction='d2h'),
row('No host field transfer','Save accepted state (GPU→GPU)\npredict → viscosity → add forcing','D2D: velocity → previous (16F bytes)\nD2D: fluid packet → checkpoint (64N bytes)\nR: u, α, κ, force; W: face scratch / velocity'),
row('Iterative convergence control','Pressure projection A → detail 3\nfields → diagnostics','R/W: pressure work, face cache / scratch, MG\nW: projected velocity + fluid packet',tx='D→H: 32 B checks; 48 B diagnostics',direction='d2h'),
row('Record work integrals','energy_collect → energy_reduce','R: current / previous faces, EM / gravity / capillary\nW: GPU work/energy scalar reductions',tx='D→H: 32 B energy packet',direction='d2h'),
row('VOF controls + validation','Interface + momentum transport → detail 4\n3 directional sweeps → normal / curvature','D2D backups; ping-pong α and face momentum\nW: new α / κ; packed moving material IDs',tx='H→D: 48 B / axis; D→H: 32 B',direction='both'),
row('Read phase flag; CPU anchors\nONLY if active topology changes','update_surface → phase_faces\nPressure projection B if momentum pending','R: new α / κ, static IDs; W: phase / mask / faces\nW: pressure correction, new fluid packet',tx='D→H: 4 B flag; 32 B / solve check',direction='d2h'),
row('f64 energy / Joule ledger\nAccept t ← t+Δt; append host CSV strings','fields/diagnostics refreshed\nLast accepted u and α feed next DC solve.','Resident: accepted αⁿ⁺¹ and uⁿ⁺¹\nNo re-solve of current with the new velocity this step.',tx='D→H: 48 B diagnostics',direction='d2h'),
row('At save cadence ONLY:\nmap 32 MiB chunk, write, unmap','Copy accepted fluid packet, one chunk at a time\nGPU→staging; wait/map; reuse same staging.','R: 64N fluid packet\nTemporary staging ≤32 MiB, not 64N extra','fluid.bin (64N bytes)\nmetadata.json last\nCSV snapshots',tx='D→H: ≤32 MiB / chunk',direction='d2h')
],['N = cell slots; F = (nx+1)(ny+1)(nz+1). All dense arrays include masked slots. Precision: f32 fields, u32 masks/IDs; CPU clocks/ledgers f64.',
   'Magnet B₀ is computed once at startup. No per-step induction/self-field or thermal solve in this client. CPU/GPU control waits are not full-field transfers.',
   'Solid arrows: explicit transfers. Green link: GPU-resident reads/writes. Chronological row order includes conditional branches detailed on sheets 2–4.'])
page('02-electrical-basis-loop','2  Electrical solve — expanded PCG and data transfers','This subactivity executes TWICE per timestep: unit-voltage basis, then motional basis; same matrix, different RHS',[
row('Set mode / voltage / target\nwrite_params()','update_phase','R: fluid α (from 64N fluid packet)\nW: σ in DC properties (8N bytes total)',tx='H→D: 80 B uniform'),
row('Submit initial solve stages','initial_guess → initialize → initial_reduce','R: prior high/low predictors, σ/flags, B₀/u as needed\nW: work[φ,r,p,Ap], partials → scalars'),
row('Initial residual: zero RHS → exit;\nalready converged → gate row 09;\notherwise → row 04','GPU scalar packet copied to MAP_READ staging','R: 64 B DC scalar buffer\nCPU gets a 64 B host vector, NOT the potential field.',tx='D→H: 64 B + map/wait',direction='d2h'),
row('Enqueue 32 CG iterations\nKeep accepted-field data on GPU.','apply_operator: Ap = A p\nworkgroup dot products','R: σ, flags, neighboring p\nW: work.Ap, partial p·Ap'),
row('No host scalar transfer\ninside these 5 kernels','alpha_reduce → update_residual\n→ beta_reduce → update_search','GPU reductions set CG coefficients.\nR/W: φ high/low, r, p, Ap; scratch / scalars'),
row('After 32 iterations: below target?\nNo → row 04; yes → row 07','Copy scalar buffer to staging','R: partial-reduction scalars only\nNo full potential/field download.',tx='D→H: 64 B + map/wait',direction='d2h'),
row('If candidate-converged:\nrequest independent residual','true_residual → initial_reduce','R: σ, neighbor potential high/low, motional RHS\nW: true-residual dot products → scalar buffer'),
row('True residual passes?\nElse tighten target and continue.','Copy scalar buffer to staging','R: true-residual scalars only',tx='D→H: 64 B + map/wait',direction='d2h'),
row('Current-conservation test\nUnit relative gate; motion mixed gate','terminal_flux → terminal_flux_reduce','R: terminal faces / neighbors, σ, φ high/low, u×B₀\nW: net and absolute terminal-flux sums'),
row('Accept basis? Else tighten → row 04.\nCap / nonfinite → error.','Copy scalar buffer to staging','R: flux scalars only. Residual and current gates\nare both required; changing precision is not free.',tx='D→H: 64 B + map/wait',direction='d2h'),
row('Read physical basis port current','collect_fields → fields_reduce','R: σ, potential, B₀, u\nW: full 64N DC output packet + power/port sums',tx='D→H: 64 B + map/wait',direction='d2h'),
row('Return basis scalar results','save_unit OR save_motion','R: current potential high / correction low\nW: predictor vec4<f32> (16N bytes)'),
row('After BOTH bases: compute V\nThen collect_fields(V) once more.','Final current / force / heating assembly','Final 64N packet overwrites unit/motion packets.\nNo new third PCG solve; only field reconstruction.',tx='H→D: 80 B; D→H: 64 B',direction='both')
],['Core work = vec4<f32> × N; predictors = vec4<f32> × N with separate unit/motion low words. σ + packed flags = vec2<f32> × N.',
   'Current V5 large-grid path: each ≤5-command chunk is submitted and fenced. Fence waits have NO payload; scalar readbacks happen only at explicit checks.',
   'Smaller-grid path batches commands without those per-chunk fences. Residual polling still occurs after 32 CG iterations. Cap/failure exits the research run.'],[(5,3,'not converged → next 32'),(9,3,'failed gate → continue')])
page('03-pressure-projection-loop','3  Pressure projection — PCG, multigrid and transfers','Called first after forcing; called again after momentum/interface transport when projection is pending',[
row('Set pressure gauge / parameters\nConnected domain: mean-free; otherwise anchors.','pressure_warm (if enabled)\npressure_coefficients → pressure_guess','R: face candidate, old pressure, α/density, mask\nW: face coefficient cache and pressure work',tx='H→D: 128 B fluid uniform'),
row('Submit residual preparation','pressure_refresh; optional residual-mean\n/subspace kernels; initial_reduce','R: candidate divergence / pressure matrix\nW: r, initial p and scalar norms'),
row('Initial status: converged → row 13;\notherwise optional MG / batch','Convergence scalars → staging','R: 32 B scalar range (residual / stop / iteration).',tx='D→H: 32 B + map/wait',direction='d2h'),
row('If needed and memory permits:\nallocate optional coarse hierarchy ONCE','MG build → initial V-cycle\n→ initial_direction → initial_reduce','Coarse coeff + correction buffers: vec4<f32> each.\nFine cache/work reuse fluid arrays; no fine copy.'),
row('Choose iteration batch:\n8 with MG, 32 without; stop at 64 boundary.','pressure_apply → alpha_reduce → update','R: cached mobility, pressure search direction\nW: Ap, αCG, pressure solution / residual'),
row('No host field transfer','If mean-free: residual_mean\n→ residual_subspace','GPU reductions/projectors keep the pressure\nconstant-nullspace out of the Krylov solve.'),
row('No host field transfer','MG option: restrict residual down levels;\ncoarse_solve; prolong + smooth upwards','R: fine residual / cache, coarse graphs\nW: preconditioned residual (fine work.w)\nJacobi option skips the V-cycle.'),
row('No host scalar transfer\ninside this iteration sequence','preconditioned_dot (MG) → beta_reduce\n→ preconditioned_search (MG) / search','R/W: fine r and search p; GPU dot-product sums\nRepeat sequence until the chosen batch is encoded.'),
row('Read residual / stop / iteration\nNot converged → row 05','Convergence scalars → staging','R: 32 B scalar range only; not pressure array.',tx='D→H: 32 B + map/wait',direction='d2h'),
row('If GPU claims converged:\nverify refreshed physical equation.','pressure_refresh + mean-free stages\n→ initial_reduce','Independent residual refresh; may restart MG\npreconditioned direction if checks fail.'),
row('Every 64 iterations / cap:\nphysical equation check if still iterating.','pressure_check → check_reduce','R: original uncached mobility / pressure equation\nW: true residual / divergence criteria'),
row('Accept → row 13; failed gate → row 05.\nCap / nonfinite → error','Convergence scalars → staging','Relative residual AND physical divergence criterion\ncontrol acceptance; cap/nonfinite causes error.',tx='D→H: 32 B + map/wait',direction='d2h'),
row('No host velocity transfer','correct → pressure_commit (warm-start path)','R: pressure solution + candidate faces\nW: projected MAC velocities / accumulated pressure'),
row('Read flow diagnostics','fields → diagnostics','W: full fluid packet + maxima / kinetic sums\nCPU gets 48 B of diagnostics, not the full packet.',tx='D→H: 48 B + map/wait',direction='d2h')
],['MG V-cycle = diagonal-start + restriction down levels; 8 coarse relaxation sweeps; prolongation and smoothing back up. Matrix/physics remain on the uniform fine grid.',
   'V5 large-grid workaround fences EACH fluid/MG stage; waits transfer no fields. Normal smaller-grid MG encodes 8 CG iterations in a batch.',
   'A phase-topology change can trigger CPU anchor rebuilding (sheet 4). In the present fixed-solid air+liquid case, moving α normally leaves the active fluid mask unchanged.'],[(8,4,'not converged → next batch'),(11,4,'verification fails → continue')])
page('04-interface-copies-and-export','4  Interface / momentum transport, checkpoints and output','Explicit GPU copies, CPU/GPU validation paths and disk writes after the first fluid projection',[
row('Host supplies Δt / density parameters','energy_collect → energy_reduce\n(begin accepted fluid-work accounting)','R: current/previous MAC faces, force, α / curvature\nW: energy sums, NOT a full field snapshot',tx='D→H: 32 B work packet',direction='d2h'),
row('No host field transfer','begin_momentum_transport / backups','D2D: MAC velocity → checkpoint subrange (16F).\nD2D: α packet → interface checkpoint (16N).\nNo second allocation for the advector checkpoint.'),
row('Select sweep axis and first/last flags\nRepeat the following rows THREE times.','Set VOF config for this axis','R: fixed advector faces; current α input\nW: 48 B surface uniform only',tx='H→D: 48 B uniform'),
row('No host field transfer','reconstruct → sweep','R: input α, fixed face advector, wall mask\nW: normal/plane reconstruction; output α packet\nα and momentum use matching mass fluxes.'),
row('Advance axis counter;\nsweeps remain → row 03','momentum kernel → swap α ping-pong index','R: old/new α, advector, carried face velocities\nW: transported face momentum (scratch / velocity)\nNext sweep uses the alternate α / momentum side.'),
row('Read interface validity / volume','normal → curvature → reduce','W: α / curvature packet, normal buffer, stats\nCFL / fraction-range / nonfinite checks run here.',tx='D→H: 32 B + map/wait',direction='d2h'),
row('If invalid: return error\nGeneric engine restores accepted state.','After valid transport: commit carried momentum','D2D: carried scratch → MAC velocity (16F)\nOn failure: α and velocity restored from backups.'),
row('No host field transfer','update_materials (generic path)','R: original material IDs, accepted α\nW: packed moving-material IDs (4N bytes).'),
row('Clear topology flag / set phase config','surface phase classifier','R: static IDs, α / κ\nW: fluid.reserved, mask if active status changes; flag',tx='H→D: 32 B config + 4 B flag reset'),
row('Read flag. If topology changed:\ndownload mask; CPU connected components / anchors.','Copy mask ONLY on changed active topology','D→H: 4 B flag normally. Changed branch: 4N mask.\nH→D: rebuilt 4N mask. Air↔liquid alone is not\na change of active topology in this isothermal path.',tx='D→H: 4 B; conditional 4N round-trip',direction='both'),
row('Set updated mixture / gauge parameters','phase_faces; momentum pending?\npressure projection B (sheet 3); fields / diagnostics','R/W: new-density face coefficients and pressure work\nW: final MAC velocity + fluid packet',tx='H→D: 128 B; D→H: 32 B / 48 B',direction='both'),
row('f64 ledger; accept t+Δt\nKeep scalar diagnostic CSV strings in host RAM.','No further field transfer on an ordinary step','α / u remain on GPU for the next electrical solve.\nThermal arrays / thermal transport are absent.'),
row('Save cadence only:\nmap chunk → write → unmap; repeat','copy_buffer_to_buffer: fluid → staging\nmap_async + wait for each completed chunk','R: accepted 64N fluid packet\nOne reusable MAP_READ buffer ≤32 MiB\nNo full-field staging allocation.','Write raw fluid.bin\nthen metadata.json\nCSVs also flushed',tx='D→H: ≤32 MiB / chunk',direction='d2h')
],['Checkpoints preserve accepted solver state for error handling; disk files are separate saved observations. No disk read/write is required inside CG iterations.',
   'Data precision: GPU packets f32; masks/flags u32; CPU ledgers f64. Every GPU→CPU readback uses a GPU staging copy + map/wait + host vector.',
   'Both the force refresh and the two fluid projection completions materialise fluid packets/diagnostics. None of those resident packets move to CPU merely because they are called “display fields”.'],[(4,2,'next axis (3 sweeps total)')])

# Expand fluid prediction explicitly; no hidden full-field host copies.
pages[0]['rows'][7:8]=[
 row('No host field transfer','Save accepted state (GPU→GPU)',
     'D2D: velocity → previous (16F bytes)\nD2D: fluid packet → checkpoint (64N bytes)'),
 row('Check viscous bound if no safe analytic shortcut',
     'predict → candidate face scratch\nOptional viscosity_bound → viscosity_bound_reduce',
     'R: old face velocity; W: predicted scratch (16F)\nIf needed: reduction produces one f32 viscous bound.',
     tx='Conditional D→H: 4 B bound',direction='d2h'),
 row('Set viscous substep / stage parameters',
     'Diffuse stress: Euler substeps or RKC stages',
     'Euler: stress_diffuse → D2D velocity→scratch (16F).\nRKC: coeff/tensor/stages; reuse fluid packet as stage state.\nNo new physical grid or full host-field transfer.',
     tx='H→D: uniform / stage coefficients'),
 row('No host field transfer','viscous kernel adds EM + gravity + capillary forcing',
     'R: diffused scratch, α, κ, force; W: face velocity.\nD2D: forced velocity → candidate scratch (16F)\nbefore pressure projection A.')]

def draw(P):
 rows=P['rows'];n=len(rows);dy=1.16;top=n*dy+3.0;bottom=2.0
 fig,ax=plt.subplots(figsize=(26,top+1));fig.subplots_adjust(left=.012,right=.988,bottom=.015,top=.985);ax.set(xlim=(0,26),ylim=(0,top+1));ax.axis('off')
 ax.text(.4,top+.62,P['title'],fontsize=23,weight='bold',va='top');ax.text(.4,top+.10,P['subtitle'],fontsize=12,color='#475569')
 for key,label in [('cpu','CPU / HOST RAM'),('gpu','GPU EXECUTION'),('data','GPU-RESIDENT DATA'),('disk','DISK')]:
  x,w=COLS[key];ax.add_patch(FancyBboxPatch((x-.06,bottom-.1),w+.12,n*dy+.55,boxstyle='round,pad=.03',fc=C[key],ec='#cbd5e1',alpha=.43));ax.text(x+.06,top-.55,label,fontsize=14,weight='bold')
 if not any(r['disk'] for r in rows):
  ax.text(22.5,top-1.5,'No disk I/O\ninside this solve',fontsize=12,color='#64748b',va='top')
 centers=[]
 for i,r in enumerate(rows):
  y=top-1.12-i*dy;centers.append(y-.36)
  for key in COLS:
   txt=r[key]
   if not txt:continue
   x,w=COLS[key];h=.86
   ax.add_patch(FancyBboxPatch((x,y-h),w,h,boxstyle='round,pad=.035,rounding_size=.06',fc=C[key],ec='#94a3b8',lw=.8))
   ax.text(x+.11,y-.11,txt,va='top',fontsize=11.1,linespacing=1.25)
  ax.text(.08,y-.05,f'{i+1:02}',fontsize=9,color='#64748b',ha='left')
  if r['gpu'] and r['data']:
   x1=12.81;x2=13.41;yy=y-.64
   ax.add_patch(FancyArrowPatch((x1,yy),(x2,yy),arrowstyle='<->',mutation_scale=10,color=GREEN,lw=1.3))
  if r['tx']:
   yy=y-.68;x1=5.00;x2=5.59
   color=ORANGE if r['direction']=='d2h' else BLUE
   style='<-' if r['direction']=='d2h' else '<->' if r['direction']=='both' else '->'
   ax.add_patch(FancyArrowPatch((x1,yy),(x2,yy),arrowstyle=style,mutation_scale=11,color=color,lw=1.7))
   # Transfer annotation below CPU box, placed away from node text.
   ax.text(.61,y-.99,r['tx'],fontsize=9.4,color=color,va='top')
  if r['disk']:
   # The disk writer is the CPU. Do NOT imply a GPU-direct-storage path.
   yy=y-1.09
   ax.plot([4.8,4.8,23.8],[y-.80,yy,yy],color='#7c3aed',lw=1.25)
   ax.add_patch(FancyArrowPatch((23.8,yy),(23.8,y-.88),arrowstyle='->',mutation_scale=11,color='#7c3aed',lw=1.4))
   ax.text(15.0,yy+.025,'Host RAM → filesystem (not GPU-direct)',fontsize=9,color='#7c3aed',va='bottom')
  if i and rows[i-1]['gpu'] and r['gpu']:
   ax.add_patch(FancyArrowPatch((9.2,top-1.12-(i-1)*dy-.90),(9.2,y+.035),arrowstyle='-|>',mutation_scale=9,color='#64748b',lw=.8))
 for fr,to,label in P['loops']:
  y1=centers[fr]-.12;y2=centers[to]+.10;x=.24
  ax.plot([.43,x,x,.43],[y1,y1,y2,y2],color='#64748b',linestyle='--',lw=.9)
  ax.annotate('',xy=(.43,y2),xytext=(x,y2),arrowprops=dict(arrowstyle='->',color='#64748b',lw=.9))

 ax.text(.45,1.55,'BLUE → H→D upload     ORANGE ← D→H readback     GREEN ↔ GPU-resident R/W     D2D = GPU→GPU copy     ↓ chronological execution',fontsize=11.3,weight='bold')
 for j,note in enumerate(P['notes']):ax.text(.45,1.13-j*.34,note,fontsize=10.6,color='#475569')
 ax.text(.45,.10,'Source-traced stopped V5 code • no new simulation • sizes refer to allocations / payload, not measured memory-bus traffic • detail and symbols in README',fontsize=9.7,color='#64748b')
 for ext in ['png','svg']:fig.savefig(R/(P['stem']+'.'+ext),dpi=145)
 plt.close(fig)
for p in pages:draw(p)
(R/'activity-data.json').write_text(json.dumps(pages,indent=2)+'\n')
