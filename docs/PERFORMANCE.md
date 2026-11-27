# Solver performance

Measurements use an NVIDIA GeForce RTX 3060 Laptop GPU with the Vulkan backend. The reference is commit `58855ca`; the optimized version uses the same geometry, materials, resolution, timestep, iteration limits and residual tolerances. No new full-grid allocations or GPU features are required.

The [uniform-grid pressure benchmark](uniform-grid-pressure-2026-11-28.md) measures component-aware pressure projection and voxel connectivity preparation against commit `c633635`.

The [November 2026 full-assembly replay](solver-replay-2026-11-25.md) measures subsequent pressure, charge, interface and analysis changes against commit `c118ad7`.

## Measured speed

The V6 case has 172×112×124 cells at 0.2 mm spacing, with EM, moving liquid/air and thermal evolution enabled. The common timestep is 2.5 µs. Three alternating reference/optimized runs each advanced two accepted ticks. Values below are medians; CPU step wall excludes project loading and solver initialization. GPU values sum compute passes and exclude copies and idle gaps.

| V6 measurement, two ticks | Reference | Optimized | Speedup |
| --- | ---: | ---: | ---: |
| CPU simulation wall | 12.613 s | 7.292 s | 1.73× |
| GPU compute passes | 11.579 s | 6.681 s | 1.73× |
| GPU pressure solve | 4.829 s | 1.247 s | 3.87× |
| GPU EM solve | 6.554 s | 5.202 s | 1.26× |
| Complete CLI process wall | 13.430 s | 8.407 s | 1.60× |

A separate 12-tick comparison reached the same **30 µs** of physical time. CPU simulation wall fell **65.608 → 42.429 s** (1.55×); GPU compute fell **60.765 → 39.474 s**. This longer run has a different mix of pressure/EM iterations and is a single comparison, rather than a three-run median.

Working buffers remain **1,715.4 MiB** in this case. GPU clock/thermal state and other applications affect timings. The first optimized process also paid additional shader compilation cost; medians over three runs reduce that one-time effect. These measurements establish results on this GPU and these inputs, rather than a hardware-independent speed guarantee.

The included `assets/hotend-cad.mhd.json` example also improved over three alternating two-tick runs: median CPU simulation wall **0.344 → 0.275 s** (1.25×), GPU compute **0.208 → 0.175 s**, and complete CLI wall **0.685 → 0.613 s**. Startup and dispatch overhead matter more on this smaller grid.

## Changes

1. Pressure face mobility and diagonal coefficients are evaluated once per projection. During CG, nonzero cached face weights also certify neighbor validity. The cache reuses the temporarily unused face-velocity buffer, with predictor/checkpoint data retained elsewhere. It is rebuilt after geometry/phase changes through the normal projection path.
2. Strict interior air edges with complete endpoint gauges and unit face reluctivity use the mathematically equivalent scalar seven-point Laplacian. Material/boundary edges retain the full curl/gauge stencil.
3. Both residual-update passes carry their diagonal in the dead operator-product slot for the subsequent search update, saving a repeated coefficient read without adding storage.
4. GPU convergence guards allow larger submissions, reducing CPU readback synchronization. Pressure reports actual GPU iterations, including convergence within a batch; independent physical checks still occur every 64 iterations and at the cap.

The original uncached physical pressure equation and full EM curl/gauge equation remain the final acceptance checks. Tolerances, the common timestep, resolution and configured iteration caps are unchanged. These changes accelerate the existing Jacobi-preconditioned solves; they do not introduce multigrid or a different physical model.

## Correctness and reproduction

GPU operator comparisons cover conductors, permeability contrast, boundaries, phase-dependent mobility, density contrast, walls, disconnected fluid regions and periodic axes. The regular headless suite covers analytic fields, current continuity, motional coupling, heat/volume conservation, refinement, failed-step rollback, history and offscreen rendering.

All **82 regular headless tests passed**, and the published TEAM6 hollow-sphere benchmark passed at 64³ and 96³ with its existing thresholds. At 96³, complex centre-field error was 3.19%, shell-current error 13.34%, mean-heat error 12.55% and mean-force error 23.25%; these remain finite-grid accuracy measurements, not a claim of exact physical prediction.

For the two-tick V6 comparison, final B and velocity maxima differed by about 10⁻⁷ relative; magnetic/kinetic/thermal energy and material volume matched the diagnostic CSV values. RMS divergence differed by about 1.2×10⁻⁷ s⁻¹. At 30 µs, final B maximum differed by 2.84×10⁻⁷ relative, velocity maximum and material volume matched, and RMS divergence differed by 3.7×10⁻⁷ s⁻¹. Magnetic energy differed by about 10⁻⁷ relative and thermal energy by about 10⁻¹⁵ relative. Changed floating-point evaluation order means results need not be bitwise identical.

```sh
cargo test
./run.sh --run-check assets/hotend-cad.mhd.json 2
# Published conducting-sphere benchmark:
cargo test published_team6_hollow_sphere -- --ignored --nocapture
# Optional synthetic-vector EM kernel throughput profile:
MHD_BENCH_PROJECT=YOUR_PROJECT.mhd.json cargo test benchmark_em_kernels -- --ignored --nocapture
```

`--run-check` writes `artifacts/run-performance.csv` and `artifacts/run-diagnostics.csv`. Use the same project and accepted physical time for comparisons, retain both CSVs, and alternate several reference/optimized runs. Separate checkouts and target directories keep build artifacts for the compared revisions independent. The V6 CAD/project used above are local user files and are not bundled in the repository.

The optional kernel microbenchmark uses fixed synthetic vectors to identify dispatch/memory costs; it is not a simulation-accuracy benchmark. EM operator/vector passes still dominate the optimized V6 run. Improved preconditioning is the next substantial algorithmic opportunity. Direct applied-source summation was about 0.04 s across the two V6 ticks, so adding per-coil field caches would give a much smaller speed benefit here while consuming more memory.

## Viscosity stability synchronization

After the symmetric-stress implementation, a global material bound can prove a single viscosity step safe. This skips one GPU compute pass (two bound kernels) and one scalar readback per tick. Stiffer cases still measure the actual GPU bound and choose stable substeps. The proof includes the allowed interface-fraction roundoff range; the existing phase-classification readback also reports invalid fractions, so validation adds no new synchronization.

Three alternating baseline/candidate runs on this machine gave:

| Complete common-run check | Baseline median CPU step time | Optimized median | Diagnostic CSV |
| --- | ---: | ---: | --- |
| Hotend, 24 ticks at 2.5 µs | 4.436 s | 4.362 s | Byte-identical in all three pairs |
| Fluid-only periodic annulus, 240 ticks at 10 µs | 6.411 s | 5.996 s | Byte-identical in all three pairs |

The baseline is commit 3c957f0; both versions use the same symmetric stress, timesteps, geometry, tolerances and iteration limits. The fluid-only case uses the bundled periodic-advection project with its common timestep reduced to 10 µs, preserving material properties and initial velocity. Its final physical time is 2.4 ms. It exercises the guaranteed-one-step path; the example's usual 1 ms timestep can require the measured-bound fallback.

The fluid-only median improves about **6.5%**, with 240 fewer GPU compute passes over 240 ticks. The hotend median improves about 1.7%, but run-to-run timing variation is significant and EM/pressure work dominates. These measurements do not support a large general speedup claim. V6's more conservative global bound still selects the measured GPU path.

The [viscous-stress guide](VISCOUS_STRESS.md) gives the bound, numerical safeguards and fallback behavior. Regular GPU tests compare the proof with the measured stencil for both density orderings, periodic/no-slip boundaries and fractions at the accepted roundoff limits.

## Stabilized viscous diffusion

Stiff moving-interface cases use second-order Runge–Kutta–Chebyshev stages instead of many Euler substeps, retaining the same symmetric stress operator. The small-work path is unchanged. Initial face values/operator data temporarily occupy display lanes that are rebuilt before frame publication; no additional volume buffer is required. A uniform coefficient table adds about 49 KiB at 256-byte device alignment. See [viscous stresses](VISCOUS_STRESS.md) for stability, memory and the work cap.

Three alternating 48³ drop probes on the RTX 3060 Laptop GPU used liquid ν=2×10⁻⁴ m²/s, density ratio 7000:1, inviscid ambient, 17 ticks and the same 0.664981 s physical duration:

| Method | Process wall times, s | Median | Maximum stress stages per tick |
|---|---|---:|---:|
| Euler subcycling | 16.347, 16.290, 16.384 | 16.347 s | 123 |
| RKC2 | 3.649, 3.707, 3.724 | 3.707 s | 20 |

The median speedup is **4.41×**, including initialization. Maximum quadrupole difference between methods is 4.22×10⁻⁷ of its initial value over this short probe. These are algorithm comparisons with different truncation errors, not byte-identical trajectories.

A full 48³, 1.5-period RKC2 trajectory completes in 113.6 s and differs from the Euler trajectory by at most 0.0341% of the initial quadrupole. Zero-crossing frequency differs by 0.0011%, and fitted late-window decay rates by about 0.1%. A 64³ RKC2 trajectory takes 436.9 s. Pressure iterations and repeated stress-stencil evaluation still matter; the method does not eliminate spatial or splitting error. [Damping measurements](VISCOUS_DROP.md) show unresolved excess decay on the finer grid.

The bundled free-surface hotend's 24 ticks and the local V6 model's two saved ticks both use one stress stage. Their complete diagnostic CSVs remain byte-identical to the preceding implementation; no acceleration is claimed for those one-stage cases. V6 storage still rounds to 1,715.4 MiB.

To compare the short probes, alternate the following commands after building once. Use the reported internal wall times or invoke the compiled test executable directly to exclude compilation:

    MHD_DROP_LIQUID_NU=0.0002 MHD_DROP_GRIDS=48 MHD_DROP_PERIODS=0.04 MHD_DROP_EULER_VISCOSITY=1 cargo test --offline rayleigh_lamb_drop_oscillation -- --ignored --nocapture
    MHD_DROP_LIQUID_NU=0.0002 MHD_DROP_GRIDS=48 MHD_DROP_PERIODS=0.04 cargo test --offline rayleigh_lamb_drop_oscillation -- --ignored --nocapture

Both commands overwrite the generated 48³ viscous-drop CSV. Archive long diagnostic traces before running short probes.

## Shared stress evaluation

A timestamped 64³ stiff-drop probe attributed **93.5%** of GPU compute time to viscous stress evaluation. CPU wall time mostly charged the asynchronous queue wait to pressure, illustrating why stage wall time alone cannot identify the expensive kernel.

The stabilized path caches cell viscosity once per macro-step and computes each stress tensor component once per stage, sharing it between adjacent momentum volumes. It borrows the existing pressure-work vector and temporary display lanes, with no additional buffer storage. Boundary edges without a cache entry retain direct evaluation. The [storage and stencil details](VISCOUS_STRESS.md) describe which lanes remain live.

Three alternating before/after executable comparisons at 64³, liquid ν=2×10⁻⁴ m²/s, density ratio 7000:1, 13 ticks and 0.332491 s physical duration gave:

| Method | Process wall times, s | Median |
|---|---|---:|
| RKC2, repeated direct stress evaluation | 6.063, 6.059, 6.040 | 6.059 s |
| RKC2, shared stresses | 1.753, 1.739, 1.747 | 1.747 s |

This is **3.47×** faster for this probe, after one warmup per executable. The new executable's first warmup took 18.0 s versus 1.75 s subsequently, so the table excludes substantial cold driver/shader-cache initialization. Both methods use at most 21 stabilized stages per tick. Maximum quadrupole difference is 2.33×10⁻⁹ of its initial value over the short comparison.

A separate timestamped comparison recorded every compute pass. Viscous GPU time fell **4,865.7 → 524.3 ms** across the same 13 ticks (9.28×); pressure took about 303 → 284 ms. Total process speedup is smaller because initialization and other solver work remain.

The reference executable was built from a6043c0. Executables were copied to separate artifact paths and identified by their test inventories before measurement. Cargo outputs must be isolated or rebuilt when comparing worktrees: sharing a target directory can leave an executable from the other checkout even when Cargo reports it fresh.

To reproduce the current profiled probe:

    MHD_DROP_PROFILE=1 MHD_DROP_LIQUID_NU=0.0002 MHD_DROP_GRIDS=64 MHD_DROP_PERIODS=0.02 cargo test --offline rayleigh_lamb_drop_oscillation -- --ignored --nocapture

It writes artifacts/drop-performance-64.csv with CPU/GPU stages, accepted status and timestamp coverage. Short probes overwrite the generated drop trajectory; archive long traces first. This optimization does not change the open damping-accuracy findings in [the viscous-drop diagnostic](VISCOUS_DROP.md).

## Incomplete height columns

Height integration now returns immediately when its endpoint fractions cannot bracket a single interface. Those columns were already marked invalid, and their summed height was unused. Valid columns retain their original sum and monotonicity check; buffer storage and solver algorithms are unchanged.

Three alternating executable pairs compared this change with 9660f91 on the seven-cell-height baseline, using an inviscid 64³ drop, density ratio 7000:1, 32 ticks and 0.831227 s physical duration. Each executable had one warmup. GPU timestamps covered every compute pass. Median interface/heat-stage GPU time fell **53.83 → 49.68 ms (7.7%)**. Process medians were **2.277 → 2.285 s**: this does **not** demonstrate an overall speedup. Pressure remained dominant at about 0.86 s of GPU time.

The complete diagnostic CSV and final accepted fraction/curvature packet were byte-identical in all eight runs, including warmups. Reproduce the profiled probe with:

    MHD_DROP_PROFILE=1 MHD_DROP_LIQUID_NU=0 MHD_DROP_GRIDS=64 MHD_DROP_PERIODS=0.05 MHD_DROP_DT_FACTOR=0.4 cargo test --offline rayleigh_lamb_drop_oscillation -- --ignored --nocapture

As above, copy distinct compiled executables before alternating versions and archive generated trajectories before short probes. The measurement isolates the early exit; wider height columns and higher-order momentum reconstruction are separate numerical changes.

## Pressure preconditioning

Grids with at least 96³ cells use an optional GPU aggregation preconditioner, with Jacobi-CG retained for smaller grids or when the workspace cannot fit. Allocation is lazy, checked against device limits and available memory, and included in working-memory reporting. The fine equation and physical-residual acceptance check are unchanged.

Three alternating warmed 96³ drop probes give median unprofiled process time **5.826 → 5.249 s** and pressure GPU time **2,286 → 1,785 ms** in separate profiled runs. Three combined EM/fluid/thermal ticks on the local V6 hotend give **29.045 → 25.782 s** process time and **4,954 → 1,977 ms** pressure GPU time; EM then dominates at about 20.8 s. Every profiling pass is recorded. Original project/GLB hashes are unchanged.

[Method, full-duration checks, memory costs and measured limits](PRESSURE_PRECONDITIONING.md), including a smooth manufactured case that still fails convergence in both methods. These measurements do not establish a general speedup for other hardware or geometries.

## Skipping unusable height stencils

A height column accepted by the curvature method has orientation +1 or −1. It cannot match a zero local normal, so those cells now retain the existing divergence estimate without querying heights. Once any column fails, the remaining columns are also skipped because the height Hessian cannot be used. These are exact pruning rules; height values and curvature formulas are unchanged.

Three alternating warmed pairs per grid, using inviscid 7000:1 drops over 0.02 periods, give:

| Grid | Interface-stage GPU median, before → after | Process median, before → after |
| --- | ---: | ---: |
| 64³, 13 ticks | 84.64 → 82.70 ms | 1.175 → 1.187 s |
| 96³, 24 ticks | 679.88 → 666.68 ms | 4.996 → 5.169 s |

Every profiling pass was recorded. The interface stage improves about 2%, while these process timings do **not** demonstrate an overall speedup. All 20 comparison runs preserve the corresponding trajectory CSV and final accepted fraction/curvature packet byte-for-byte, including shifted and viscous controls. The 28 regular surface/interface tests pass. The change adds no buffers.

## Curvature-only longer columns

[Eleven/thirteen-cell curvature fallback columns](HEIGHT_CURVATURE.md) improve
static-drop motion and oscillation checks without allocating another volume
buffer. On three alternating warmed V6 three-tick pairs, interface/heat GPU
medians are **229.60 → 230.08 ms** and process medians **26.031 → 25.814 s**.
All passes are recorded and owner input hashes are unchanged. The small process
difference is not presented as a speedup; EM still accounts for about 20.9 s.
Fluid velocities intentionally differ because the curvature changes. The
validation guide records the short local-peak-speed tradeoff and the revised
energy-based regression explicitly.

## Direct air neighbor loads

Strict scalar-air eligibility already proves all six same-component neighbors
are valid and excludes constrained boundary edges. The operator now loads them
by flat lattice stride, avoiding repeated coordinate/bounds work and leaving
coordinate decoding for material/boundary edges. The x/y/z addition order is
preserved. The full uncached curl/gauge equation remains the acceptance check;
CG iterations, tolerances, timestep, resolution and working storage are unchanged.

Three alternating warmed executable pairs ran three combined V6 ticks at 10 µs:

| Quantity | Previous indexing | Direct loads |
| --- | ---: | ---: |
| Median process time | 26.396 s | 25.067 s |
| Median EM solve GPU time | 21,316.63 ms | 19,817.81 ms |
| Median pressure GPU time | 2,022.63 ms | 2,024.59 ms |

This gives **5.0% lower process time** and **7.0% lower EM GPU time** in this
probe. Every profiling pass is recorded. All eight trajectory/energy diagnostic
CSVs, including warmups, are byte-identical; EM iterations remain 858/864/873.
Owner project/GLB hashes and the 1,715.4 MiB working-storage total are unchanged.
The full 135-test headless suite and strict all-target Clippy pass.

A separate synthetic-vector operator microbenchmark has overlapping ranges:
**3.80–5.13 ms** before and **4.25–4.90 ms** after. Its short dispatch groups do
not establish an isolated throughput gain. The combined-run comparison above is
the performance evidence; these RTX 3060 Laptop GPU measurements are not a
hardware-wide guarantee.

The earlier local-block preconditioner trials remain on a
[separate research branch](https://github.com/sdfgeoff/mhd-simulator/blob/research/em-tile-preconditioner-20261118/docs/EM_TILE_RESEARCH.md).
They reduced iterations but did not establish an overall advantage, so the
production EM method remains Jacobi-CG.
