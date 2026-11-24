"""Render the audited timestep and allocation inventory; no solver execution."""
from pathlib import Path
import json,textwrap
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import FancyBboxPatch,FancyArrowPatch
R=Path(__file__).parent
D=json.loads((R/'buffer-inventory.json').read_text())
plt.rcParams.update({'font.family':'DejaVu Sans','font.size':12})
fig,ax=plt.subplots(figsize=(18,19));fig.subplots_adjust(left=.025,right=.975,top=.975,bottom=.02);ax.set(xlim=(0,18),ylim=(0,19));ax.axis('off')
ax.text(.2,18.8,'Direct-contact MHD: one timestep and GPU storage',fontsize=23,weight='bold',va='top')
ax.text(.2,18.25,'Audited stopped Concept1 V5 client • uniform grid • inductionless / isothermal • no simulation running',fontsize=12.5,color='#475569')
ax.text(.3,17.7,'TIMESTEP  (accepted αⁿ, uⁿ → accepted αⁿ⁺¹, uⁿ⁺¹)',fontsize=14,weight='bold')
ax.text(9.25,17.7,'DISTINCT LIVE GPU BUFFER BANKS',fontsize=14,weight='bold')
def box(x,y,w,h,title,body,color='#eaf2ff',fs=11.7):
 ax.add_patch(FancyBboxPatch((x,y),w,h,boxstyle='round,pad=.10,rounding_size=.08',facecolor=color,edgecolor='#64748b',lw=1.1))
 ax.text(x+.16,y+h-.16,title,va='top',fontsize=12.5,weight='bold',color='#0f172a')
 ax.text(x+.16,y+h-.53,body,va='top',fontsize=fs,linespacing=1.35,color='#172033')
steps=[
('1  CPU: choose commanded current', 'Evaluate pulse at t + Δt/2.  CPU time is f64; current sent as f32.\nNo coil / induction update inside this timestep.', '#fff4ce'),
('2  GPU: unit-voltage electrical basis  [C, M, F]', 'Update σ from accepted αⁿ; warm-started, Jacobi-PCG solve.\nSolve ∇·J = 0 with 1 V terminals and no motional EMF.\nGPU reductions → CPU residual / terminal-current checks.', '#eaf2ff'),
('3  GPU: motional electrical basis  [C, M, F]', 'Update σ again; PCG solve with uⁿ × B₀ and 0 V terminals.\nSave both potential predictors + low correction words.\nEach basis uses the same matrix, a different right-hand side.', '#eaf2ff'),
('4  CPU + GPU: impose current, compose force  [C]', 'CPU: V = (Icmd − Imotional) / Gunit.\nGPU: J = σ(−∇φ + uⁿ × B₀); F = J × B₀; Q = J²/σ.\ncollect_fields runs THREE times in total: unit, motion, final.', '#eaf2ff'),
('5  GPU: fluid predictor + pressure projection A  [F, P]', 'Save rollback state; add EM, gravity and capillary forces.\nViscous stress → variable-density pressure PCG (+ optional MG).\nPressure changes face velocities to satisfy incompressibility.', '#eaf2ff'),
('6  GPU: move liquid interface + momentum  [S, F, G]', 'Three directional geometric VOF sweeps; ping-pong α buffers.\nTransport momentum consistently; refresh normals / curvature.\nWrite packed moving-material IDs (separate from DC σ array).', '#eaf2ff'),
('7  GPU: update phase + pressure projection B  [F, P]', 'Copy new α / curvature into fluid state; update density / mask.\nA second projection follows transported momentum when pending.\nIf topology changes: GPU mask → CPU anchors → GPU upload.', '#eaf2ff'),
('8  CPU: diagnostics, accept timestep; optional export', 'GPU scalar reductions / small readbacks; f64 energy accounting.\nAdvance t. At saved-frame cadence only: fluid packet → disk\nthrough ONE reusable 32 MiB GPU staging buffer.', '#fff4ce')]
ys=[]
for i,(title,body,color) in enumerate(steps):
 y=15.88-i*1.73;ys.append(y);box(.35,y,8.2,1.40,title,body,color)
 if i:ax.add_patch(FancyArrowPatch((4.45,ys[i-1]-.03),(4.45,y+1.51),arrowstyle='-|>',mutation_scale=15,lw=1.5,color='#334155'))
box(9.3,15.63,8.25,1.48,'G  Geometry / material bank  •  12N bytes', 'Body mask: u32 × N. Static material ID: u32 × N.\nMoving ID + quantised α: packed u32 × N (23-bit fraction).\nSmall material / body tables: f32 values + u32 flags.', '#f1f5f9')
box(9.3,13.83,8.25,1.48,'M  Fixed magnetic field  •  64N bytes', 'B₀ is computed ONCE from the full CAD magnets at startup.\nRetained generic packet: 16 × f32 per cell, although the DC\nsolver mainly needs B₀.xyz. No per-step magnetic PDE here.', '#f1f5f9')
box(9.3,11.19,8.25,2.30,'C  Electrical bank  •  104N bytes + reductions', 'σ / terminal tags: 2 × f32 = 8N bytes (tags partly bit-packed).\nPCG work: 4 × f32 = 16N (φ, residual, search, operator).\nPredictors: 4 × f32 = 16N (unit / motion high + low words).\nOutput: 16 × f32 = 64N (B, J, F / Q, applied-B packet).\nHigh + low compensation is NOT general f64 precision.\nDC output is shared with fluid force: counted ONCE.', '#eef2ff')
box(9.3,7.53,8.25,3.32,'F  Fluid bank  •  148N + 48F bytes + reductions', 'Mask / pressure anchors: u32 × N = 4N bytes.\nFace velocity, face scratch, previous velocity: 3 × 16F.\nEach face-grid record is vec4<f32>; .w is reused by pressure.\nPressure PCG: vec4<f32> × N = 16N.\nFluid state/output: 16 × f32 × N = 64N.\n  u.xyz/speed; pressure/divergence; force; phase/α/curvature.\nState/advector checkpoint: 64N, reused for two lifetimes.\nThis “display” packet feeds physics too; not all is disposable.', '#eef2ff')
box(9.3,5.10,8.25,2.08,'S  Interface bank  •  64N bytes + reductions', 'Two α / curvature ping-pong buffers: 2 × vec4<f32> × N.\nOne normal / reconstruction buffer: vec4<f32> × N.\nOne accepted-interface checkpoint: vec4<f32> × N.\nα packet also carries sweep / divergence corrections.\nNo thermal field arrays in this isothermal experiment.', '#edf7ef')
box(9.3,3.35,8.25,1.41,'P  Pressure multigrid (MG)  •  optional', 'Coarse coefficients + coarse correction/work: 2 × vec4<f32>.\nAllocated for Σ coarse cells (~N/7); ~0.094 GiB in this case.\nFine-level MG reuses F work / velocity / scratch storage.', '#edf7ef')
box(9.3,1.43,8.25,1.58,'Synchronization and submission boundaries', 'PCG uses batched checks, GPU dot-product reductions and\nCPU readbacks. Large-grid watchdog workaround fences short\nDC / fluid / MG submissions; same equations, more CPU waits.\nRollback arrays support rejected steps; they are not disk saves.', '#fff4ce')
ax.text(.4,1.65,'Feedback to next step: electrical force uses the previous\naccepted α and velocity (explicit, one-step-lagged coupling).',fontsize=11.5,color='#334155')
ax.text(.4,.87,'N = 21,814,800 cells; F = 22,052,464 face-grid slots\nMajor logical arrays ≈ 9.05 GiB = 445.5 bytes per cell.\nMeasured whole-device AMD memory ≈ 11.5 GiB (VRAM + GTT).',fontsize=11.5,weight='bold')
ax.text(.4,.05,'Startup only: temporary applied-EM edge arrays (52E bytes) + cell workspace; only B₀ survives. Driver caches / staging / small buffers are extra.\nGPU arithmetic/storage: f32 and u32; no bulk f16 or f64. Memory figures count shared allocations once. Detailed inventory and source references accompany this diagram.',fontsize=10.7,color='#475569',va='bottom')
for ext in ['png','svg']:fig.savefig(R/f'timestep-and-buffers.{ext}',dpi=160)
plt.close(fig)
# Full inventory: numbers computed from actual allocation strides, not profiler estimates.
fig,ax=plt.subplots(figsize=(19,15));fig.subplots_adjust(left=.025,right=.975,top=.94,bottom=.10);ax.axis('off')
ax.set_title('GPU buffer inventory — stopped 85 µm Concept1 V5\nN = 21,814,800 cells; F = 22,052,464 face-grid slots; shared buffers counted once',fontsize=17,pad=20)
cols=['Bank / buffer','Contents','Storage precision','Bytes/record','GiB']
rows=[]
for b in D['buffers']:
 rows.append([b['group']+' / '+b['name'],'\n'.join(textwrap.wrap(b['contents'],49)),'\n'.join(textwrap.wrap(b['precision'],30)),str(b['stride_bytes']),f"{b['GiB']:.3f}"])
t=ax.table(cellText=rows,colLabels=cols,colWidths=[.23,.36,.27,.075,.065],loc='center',cellLoc='left');t.auto_set_font_size(False);t.set_fontsize(10.2)
for (i,j),c in t.get_celld().items():
 c.set_height(.040 if i else .035);c.set_edgecolor('#cbd5e1')
 if i==0:c.set_facecolor('#153653');c.set_text_props(color='white',weight='bold')
 else:c.set_facecolor('#f1f5f9' if i%2 else 'white')
fig.text(.03,.065,f"Major arrays: {D['major_logical_bytes']/2**30:.3f} GiB • {D['major_logical_bytes']/D['N']:.1f} bytes / cell (including face grids and coarse MG)",fontsize=13,weight='bold')
fig.text(.03,.025,'Not a complete driver allocation census: add uniforms, small scalar / table buffers, profiler, temporary readbacks and driver caches.\nAt export only: reusable 32 MiB staging buffer. Startup EM edge work is separate and released before fluid integration.\nGeneric field packets contain replicated / diagnostic values; checkpoints and solver work have different lifetimes. Exact formulas and source files: buffer-inventory.json.',fontsize=10.5)
for ext in ['png','svg']:fig.savefig(R/f'buffer-inventory.{ext}',dpi=160)
