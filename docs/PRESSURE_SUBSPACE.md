# Connected pressure nullspace

A connected, closed or periodic pressure graph has one constant null vector.
The solver now treats that mode explicitly with mean-free residuals. It keeps
the original reference cell to choose a pressure gauge, while retaining every
physical equation in the connected graph. Disconnected domains keep their
independent pinned references.

## Equation and iteration

For the weighted graph Laplacian `A`, let `P = I - 11ᵀ/N`. Each CG residual is
projected with `P`; Jacobi or the existing fixed symmetric multigrid cycle `M`
induces the positive preconditioner `P M P` on the constant-orthogonal subspace.
Search vectors can carry a constant because `A` annihilates it and the residual
is orthogonal to it. Pressure increments use `p - p[reference]`, keeping the
solution's reference value fixed without replacing a row of `A`.

The existing reference is preferable to a large common pressure offset in the
light ambient phase. The separate research study found real-case failures when
centering the solution by its global mean, despite passing uniform analytic
checks. Its successful reference-gauge formulation and failed rank-one variants
are [preserved together](https://github.com/sdfgeoff/mhd-simulator/blob/research/pressure-nullspace-20261118/docs/FLOATING_PRESSURE_RESEARCH.md).

The multigrid hierarchy includes the reference cell and omits point-anchor
reactions in connected mode. Its existing restriction, prolongation, smoothing
and fixed coarse solve are unchanged. For multiple components it restores the
original anchor exclusion and reactions. The graph is rebuilt for each projection,
so a topology change can switch modes using the same allocation.

## Physical acceptance and storage

The user-facing relative tolerance, absolute RMS-divergence floor, iteration cap
and every-64-iteration physical check are unchanged. Acceptance still evaluates
the original uncached mobility/Laplacian equation at **all** fluid cells. In
particular, a removed residual mean cannot hide incompatible physical data.
Recursive convergence triggers an independent physical refresh before acceptance.

The connected solve no longer needs the pinned system's additional internal
cell-count stopping factor. It can therefore stop closer to the configured
physical floor. This removes excess solve accuracy; it does not promise identical
local fields or identical final divergence below that floor.

The residual mean and projection reuse existing reduction, work and scalar
buffers. No volume-sized buffer is added. The pressure-mode bit in the multigrid
scalar packet is initialized by `pressure_guess` before hierarchy construction;
field statistics may reuse it only after projection. The original multigrid
selection threshold and live workspace checks remain in place.

## GPU checks

1. Analytic periodic-gradient projection compares original pinned Jacobi,
   mean-free Jacobi and mean-free multigrid against the same scalar reference.
2. Cached versus uncached GPU operators retain their original error bound across
   walls, periodicity and phase-dependent mobilities, using each intended gauge.
3. Galerkin graph checks cover both connected modes; density-contrast projections
   include 7000:1 in both orientations. Neumann V-cycles pass symmetry/positivity
   checks and preserve live vector lanes.
4. A connected → disconnected → connected test checks physical projection and
   reuses the same multigrid bytes. Disconnected graphs retain pinned references.
5. Forced late pressure failure and retry check packet restoration for both
   gauges with both preconditioners.

The research study also includes full nozzle trajectories, refined prefixes,
1.5-period 64³/96³ drop oscillations and coupled V6 comparisons. Production
integration is being checked separately; research timings are not automatically
production timing claims.

## Application measurements

Three alternating warmed application-binary pairs ran three coupled 10 µs ticks
on the actual V6 project, on an RTX 3060 Laptop GPU. All profiling passes were
recorded and both input file hashes remained unchanged.

| Quantity | Original pinned solve | Connected mean-free solve |
| --- | ---: | ---: |
| Median process time | 25.6778 s | 24.2620 s |
| Median pressure GPU time | 2,290.30 ms | 889.70 ms |
| Median EM GPU time | 20,342.69 ms | 20,210.50 ms |

This is **5.5% lower process time** and **61.2% lower pressure GPU time** on this
short case. The small EM timing difference is not an EM algorithm improvement.
No volume buffer is added. The initial buffer total is unchanged at 1715.4 MiB;
after lazy MG allocation the same total is 1725.9 MiB in both methods.

Material-volume diagnostics and maximum B are identical in all eight runs.
Maximum recorded-speed differences are 2e-8 m/s; kinetic-energy differences are
at most 1.12e-16 J, and thermal-energy differences are below 1.1e-9 J. These
compare diagnostics; the research report separately includes full-field norms
and late unresolved-fragment differences. The original physical acceptance
criterion remains unchanged, with less excess accuracy below its absolute floor.

One cold candidate warmup took 54.96 s, versus 24.35 s for the already cached
original. Warmups are excluded from the medians; first-use shader startup can
therefore be substantially longer than the warmed measurements.

The integrated code passes **154 headless tests** and strict Clippy. Full moving
controls of this production implementation are still being completed before
merging, in addition to the successful research trajectories.

The same V6 project fails its original EM iteration cap at tick five with either
pressure method. A separate diagnostic allowing more EM work, without changing
its tolerance, reaches that tick in 1,057 iterations. The three-tick timing probe
avoids that known limit; this change does not claim to fix it.
