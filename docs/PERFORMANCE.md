# Solver performance

Measurements use an NVIDIA GeForce RTX 3060 Laptop GPU with the Vulkan backend. The reference is commit `58855ca`; the optimized version uses the same geometry, materials, resolution, timestep, iteration limits and residual tolerances. No new full-grid allocations or GPU features are required.

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
