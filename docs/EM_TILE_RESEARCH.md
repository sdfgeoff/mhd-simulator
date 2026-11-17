# EM local-block preconditioner experiment

This branch contains **test-only research**. Production continues to use the
existing EM Jacobi-CG solver. The experiment reuses its outer CG loop, right-hand
side, full curl/gauge operator, physical-residual acceptance and rollback.

## Method and storage

Each edge orientation has disjoint 4³ principal blocks. Same-orientation
couplings form a seven-point stencil: transverse links use face reluctivity,
axial links use the cached nodal gauge, and the diagonal includes conductivity
mass. Cross-orientation couplings remain in the full outer equation.

A fixed number of weighted-Jacobi or Chebyshev steps from zero approximates each
block inverse. Two shared arrays and the existing reduction array need at most
6 KiB per group. Dead `work.w` holds the preconditioned residual; no global field
buffer is added. A fused variant groups four tiles and retains the existing
partial-reduction count, including padded partial tiles.

The Chebyshev target is `[1−cos(π/5), 2]` for the diagonally normalized component
block. The lower value targets a uniform four-cell air block; material blocks
may have smaller eigenvalues. The upper bound follows diagonal dominance.
The resulting polynomial remains positive on `[0,2]`, including below the
lower target. This is a block preconditioner, not a Chebyshev approximation to
the full coupled curl/gauge matrix. [PETSc's Chebyshev documentation](https://petsc.org/release/manualpages/KSP/KSPCHEBYSHEV/)
and [spectral-target notes](https://petsc.org/release/manualpages/KSP/KSPChebyshevSetEigenvalues/)
discuss the importance of positive operators and appropriate upper bounds.

## Measurements

Three alternating warmed triples ran three complete V6 EM/fluid/thermal ticks
at the original settings, with unchanged owner project/GLB hashes. Every GPU
profiling pass is recorded:

| Method | EM iterations, three ticks | Median EM GPU time | Median process time |
| --- | --- | ---: | ---: |
| Existing Jacobi-CG | 858 / 864 / 873 | 21.106 s | 25.923 s |
| Separate eight-step local blocks | 494 / 589 / 592 | 24.151 s | 29.133 s |
| Fused eight-step local blocks | 494 / 589 / 592 | 23.830 s | 28.740 s |

Fewer iterations did **not** produce a speedup. Subsequent single screening runs
optimized bulk-air row construction and tried different polynomials:

| Screening variant | EM GPU time, three ticks |
| --- | ---: |
| Eight Jacobi stages, optimized rows | 20.744 s |
| Four Jacobi stages | 21.682 s |
| Two Jacobi stages | 22.494 s |
| Eight Chebyshev stages | 21.323 s |

These screening numbers lack repeated paired controls and do not establish a
production advantage. Working-buffer accounting stays 1,798,777,792 bytes for
the combined project. All runs meet the original EM residual target of 10⁻⁴.
The development adapter is an NVIDIA GeForce RTX 3060 Laptop GPU.

## Checks and reproduction

GPU checks cover principal-block action versus the full uncached operator,
positive quadratic forms, symmetry, linearity, zero input, independent/fused
execution, preserved live vector lanes and reduction values. They include
conductivity/permeability contrasts, static/dynamic mass, odd and thin grids,
and partial boundary tiles. Analytic cube eigenmodes verify the polynomial
without a second CPU grid solver; the eight-stage Chebyshev relative error is
8.77×10⁻⁸ over 64 modes in each orientation.

The existing regular EM physics and rollback tests pass with the prototype.
The published TEAM 6 refinement has **not** been repeated for this experiment.
It is not selected for production.

```sh
MHD_EM_TILE=1 cargo test --offline --lib em:: -- --nocapture
MHD_EM_TILE=1 MHD_EM_TILE_CHEBYSHEV=1 \
  cargo test --offline --lib em:: -- --nocapture
MHD_EM_TILE=1 MHD_EM_TILE_PAIRS=2 \
  cargo test --offline --lib em:: -- --nocapture
MHD_EM_TILE=1 MHD_BENCH_PROJECT=assets/hotend-cad-v6.mhd.json \
MHD_BENCH_STEPS=3 MHD_BENCH_PROFILE=1 \
  cargo test --offline --lib combined_project_preconditioner_probe -- --ignored --nocapture
```

The V6 project is an owner-local input, not distributed with the repository.
The benchmark defaults to the bundled free-surface hotend if no path is set.
Unset `MHD_EM_TILE` for the Jacobi baseline; setting it to `0` still enables the
prototype because the flag checks presence. `MHD_EM_TILE_PAIRS` selects half the
fixed stage count (default 4). Output CSVs under `artifacts/em-tile-combined*`
are overwritten on subsequent runs; archive them before changing settings.
