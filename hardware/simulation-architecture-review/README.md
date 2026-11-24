# Direct-contact simulation architecture audit

Audit requested in topic25 replies1050/1051. This documents the stopped Concept1 V5 research client, not the separate induction/coil simulation. Source inspected at85d10db. No physics experiment or solver modification was performed for this audit.

## 1. Attached diagrams

1. [One timestep and GPU buffer banks](timestep-and-buffers.png), also [SVG](timestep-and-buffers.svg).
2. [Detailed buffer inventory](buffer-inventory.png), also [SVG](buffer-inventory.svg).
3. [Exact allocation formulas and source references](buffer-inventory.json). [Renderer](render_diagrams.py).

N is the number of cell slots, including solid and empty parts of the rectangular domain. F=(nx+1)(ny+1)(nz+1) is the shared rectangular MAC face-grid slot count; each vec4 carries all three staggered velocity components at their respective face positions. E is the EM edge count. Coarse pressure MG levels use smaller grids as a preconditioner for the same fine-grid equation; this is not spatially adaptive physical resolution.

## 2. Timestep path

1. CPU evaluates the prescribed current at the timestep midpoint. Clock and accumulated energy are f64; current and solver parameters are transferred as f32.
2. GPU updates conductivity from the last accepted liquid fraction. A warm-started Jacobi-preconditioned conjugate-gradient (PCG) solve finds the one-volt electrical basis. CPU reads GPU reductions to check the actual residual and current conservation. Previous high/low potential words predict the solution; a fresh solve still verifies it.
3. GPU solves the motional basis for u×B with zero terminal voltage, using the same conductivity matrix. The conductivity update currently executes again. This second basis is required for current-controlled operation with motional EMF.
4. CPU computes terminal voltage from the two basis port currents. GPU combines the basis potentials and writes current, Lorentz force and Joule heating. The generic `collect_fields` kernel currently executes for unit, motion and final fields; the first two full output writes are overwritten by the third.
5. Fluid integration saves its accepted state, predicts face velocities with EM/gravity/capillary forces, diffuses viscous stress and projects velocity onto the incompressible constraint. Pressure uses PCG with optional existing aggregation multigrid. Both air and liquid are part of this variable-density pressure domain; CAD solids are masked out, but dense kernels still launch over the rectangular grid.
6. Three directional geometric VOF sweeps move the interface and momentum consistently. Normal/curvature fields are refreshed. A generic moving-material-ID grid is also written.
7. Fluid phase/alpha/curvature fields are updated. Momentum transport normally leaves a pending projection, so `update_surface` invokes another pressure projection. This is not simply an accidentally repeated identical solve: it projects the transported momentum and changed mixture coefficients. If the active fluid topology changes, the mask is read back, CPU pressure anchors are rebuilt, and the mask is uploaded. In this fixed-solid isothermal air+liquid case, switching between air and liquid does not normally change the active mask.
8. GPU reductions and CPU f64 ledgers collect diagnostics and accept the step. At saved-frame cadence, one reusable32MiB staging allocation streams the64-byte-per-cell fluid packet to disk; metadata is published after the field is complete. Current/timing CSVs are flushed at saved frames and termination; detailed profiler CSV is written at termination.

Electrical force uses accepted alpha and velocity from the start of the step. There is no within-timestep implicit iteration between the electrical and fluid solvers. Permanent magnet B0 is computed once at startup and held fixed. There is no per-timestep induction/reaction/self-field solve or thermal evolution in this research client; Joule heating is counted, not applied to evolving temperature.

## 3. Precision and ownership

1. Bulk GPU fields and arithmetic are32-bit float, not64-bit. IDs/masks/indices are u32. There are no bulk f16 fields in this path.
2. Electrical potentials use two f32 words with explicit low corrections. This improves small potential differences in high-conductivity metal; it is not a general f64 solver. The four predictor lanes hold unit high, motion high, unit low and motion low. Separate PCG work holds the currently iterated potential and vectors.
3. Static material and body tables mix vec4<f32> values with vec4<u32> flags. Moving material IDs pack the ID, a23-bit quantised alpha and a moving flag into one u32. Electrical terminal flags optionally carry16-bit material group tags in their low bits.
4. The retained B0 packet is64bytes/cell because it uses the generic16-float EM record, although this client mainly needs B.xyz. The final DC record is another64bytes/cell, including another copy of B and applied B, J, force and heating.
5. The DC result allocation is shared directly with the fluid force input. It is not a second frozen-force copy. Static and evolving volumes share body masks and input tables. Pressure MG reuses fine-level fluid work/velocity/scratch components. These aliases are counted once.
6. The fluid packet's name includes “display”, but it is also physical coupling state: conduction reads velocity/alpha from it, surface/pressure read phase and curvature, and accelerated viscosity can temporarily reuse its velocity/force lanes for RKC stage state. It cannot simply be deleted as rendering data.
7. Full-cell and face-grid rollback/advector checkpoints are distinct from disk exports. Their lifetimes overlap in some phases and alternate in others; any reuse must follow those lifetimes rather than equate every backup with wasted storage.

## 4. Memory totals and the6GB correction

| Case | Cell slots | Major distinct logical arrays | Whole-device observation |
|---|---:|---:|---:|
| Stopped AMD85µm V5 |21,814,800|9.051GiB,445.5bytes/cell|approximately11.5GiB VRAM+GTT during integration|
| Previous NVIDIA100µm assembly |11,835,264|approximately4.91GiB with the same allocation formulas|5137MiB, approximately5.02GiB|

The21.8million-cell case did **not** fit in6GB. AMD used shared system-memory-backed GPU storage. The6GiB board was the smaller NVIDIA case. Capacity divided by cell count is also not the same as allocated bytes per cell.

The inventory sums declared major buffer sizes, including face grids, reductions and optional coarse MG, with aliases counted once. It excludes small uniforms/tables/scalars, profiler storage, temporary readbacks, driver allocations and caches. Whole-device sysfs counters include allocations outside this logical inventory and are not an exact process-only live-allocation census. The roughly2.4GiB gap on AMD has not been assigned to individual causes; it should not be presented as a measured amount of “driver overhead”.

Startup has a different peak: the general applied-EM constructor allocates edge work(52E bytes) and two64N cell packets plus reductions/sources. After B0 is computed, the client retains only one64N packet and drops the other EM workspace. The edge-source buffer32E also has an individual binding-size limit independent of total memory. This was one of the failed AMD startup constraints. CAD rasterization/fraction initialization also have temporary buffers which are not persistent timestep storage.

## 5. Obvious candidates before variable physical resolution

These are code-audit opportunities, **not implemented optimizations or measured speedup promises**. Savings below use the21.8million-cell case and are allocation arithmetic, not measured driver-memory reductions.

| Priority | Candidate | Why it helps / limits |
|---|---|---|
|1|Narrow the fixed B0 record and fluid-force output contract while keeping f32|B0 can plausibly become vec4<f32>; a force+heat-only packet can also become vec4<f32>, reconstructing diagnostic J/B records only when needed. Two64→16byte records would save96N=1.95GiB. Requires changing shader/binding/export contracts, not dropping physical precision.|
|2|Avoid materialising full unit/motional output records|The first two `collect_fields` passes only supply basis port currents to `solve_current`; their full64N outputs are overwritten. A terminal-current-only reduction could avoid two wide full-grid output writes and unnecessary field calculations. Final physical fields still need computation.|
|3|Prepare conductivity/matrix data once per accepted alpha; improve electrical preconditioning|Both electrical bases share the matrix. The phase update runs twice; neighbour conductances/diagonals are recomputed extensively. Cache/reuse requires invalidation on alpha/contact changes. Charge PCG currently has diagonal/Jacobi preconditioning, unlike pressure's MG. Stronger charge preconditioning is a plausible compute win, not a guaranteed factor.|
|4|Audit headless checkpoint lifetimes and split state from output packets|A fail-fast CLI may not need the generic interactive full-field rollback contract. Replacing a64N field checkpoint with only its16F advector requirement would save about0.97GiB, if the failure/export semantics permit it. Other packets can potentially separate needed state from derived diagnostics. Previous face velocity is also used in work/energy accounting, so it is not just a redundant backup.|
|5|Remove unused generic moving-material update in this specific path|With induction and thermal evolution disabled, external conduction reads alpha from the fluid packet, not the packed moving-ID array. The run-private4N material grid and its update appear unnecessary to this path after checking all public inspection/export contracts. Saving is only0.081GiB but also avoids a full-grid write.|
|6|Compact active electrical degrees of freedom and tune synchronization|Electrical kernels scan dense N even where sigma is zero. An active-conductor index/graph can preserve uniform physical resolution while reducing wasted work, but is more involved than narrowing packets. GPU reductions already run on GPU; CPU readbacks and the large-grid watchdog fences still impose synchronization. Tune submission chunks per GPU without restoring timeout-sized batches.|

A second pressure projection should not just be removed: it follows momentum transport and must preserve mass/divergence/coupling accuracy. Likewise, zero commanded current does not imply zero motional current, so electrical work after the pulse cannot simply be skipped. Blind f16 conversion is not the first recommendation: the conductivity contrast and tiny copper potential differences already required compensated f32 arithmetic. Remove unused/repeated quantities before reducing their precision.

For reference, recorded AMD V5 first250steps spent approximately38% of measured charge+fluid wall time in the external electrical solve and62% in `RunState::step`(fluid, surface, phase and accounting). This is a coarse CPU wall-time split, not a kernel breakdown; the detailed profiler CSV was not flushed before the external stop. Timing benefit must therefore be measured rather than inferred from saved bytes alone.

## 6. Source map

| Area | Source / entry point |
|---|---|
|Research outer loop, startup B0, pulse and electrical/fluid timing|[examples/conduction_flow.rs](../../examples/conduction_flow.rs), `main`|
|Two electrical bases, predictor state and buffer allocation|[examples/conduction/gpu.rs](../../examples/conduction/gpu.rs), `new_with_groups`, `solve_current`, `solve`|
|Electrical kernels and compensated potentials|[examples/conduction/solver.wgsl](../../examples/conduction/solver.wgsl)|
|Coupling order and conditional thermal/EM branches|[src/run.rs](../../src/run.rs), `RunState::construct`, `step`|
|MAC buffers, pressure projection and phase update|[src/fluid/mod.rs](../../src/fluid/mod.rs), constructor, `step`, `project`, `update_cells`|
|Physical meaning of fluid packet lanes|[src/fluid/solver.wgsl](../../src/fluid/solver.wgsl), `fields`, pressure coefficients|
|Pressure aggregation hierarchy / reused buffers|[src/fluid/multigrid.rs](../../src/fluid/multigrid.rs), [shader](../../src/fluid/multigrid.wgsl)|
|VOF buffers, advection and momentum|[src/surface.rs](../../src/surface.rs), [src/surface.wgsl](../../src/surface.wgsl), [src/surface_momentum.wgsl](../../src/surface_momentum.wgsl)|
|Packed moving material grid|[src/surface_ids.wgsl](../../src/surface_ids.wgsl)|
|Persistent CAD masks and material IDs|[src/domain.rs](../../src/domain.rs), [src/inputs.rs](../../src/inputs.rs)|
|Accepted field export and bounded staging|[src/run/export.rs](../../src/run/export.rs)|

Both diagrams were rendered and visually inspected. Buffer arithmetic was cross-checked against the source strides; no GPU test/run was launched. All simulations remain stopped.

## Expanded activity diagrams

The [four detailed activity sheets](activity/README.md) expand the timestep, electrical PCG, pressure/MG, interface/momentum and export paths, with residency, copy sizes and CPU/GPU synchronization. Prepared for board reply1056.
