# Pressure null-space research

This is a test-only experiment preserved on a separate research branch. Production
on main is unchanged. The current candidate uses mean-free residuals with the
original air-cell pressure reference; the unsuccessful rank-one and mean-gauge
variants below are retained as historical controls. No speedup claim has been
made and adoption remains pending further moving-case and timing checks.

## Initial rank-one hypothesis

The physical closed/periodic pressure graph is a Neumann Laplacian with one
constant null vector per connected region. The current solver pins one cell
per region. Hypothesis: the point constraint adds poorly conditioned modes that
cost more than treating the constant mode globally. The primary
[PETSc null-space documentation](https://petsc.org/release/manualpages/Mat/MatSetNullSpace/)
explains compatibility with the null space and orthogonalized right-hand sides;
it does not describe or validate this implementation.

For exactly one connected region, the first prototype used
`A_g = A + gamma 11^T/N`, where `gamma` is the harmonic mean of the positive
Jacobi diagonals. This is a rank-one gauge term; pressure gradients are unchanged
for a compatible right-hand side. Residual verification still evaluates the
original physical pressure equation, including the former anchor row. The
reported pressure is shifted to the original reference cell afterwards.

GPU reductions compute vector means and the gauge scale, reusing existing scalar
and partial buffers. No volume-sized field is added. Both Jacobi and a Neumann aggregation V-cycle are being tested; the latter
includes the former anchor in every level and removes anchor reactions. Multiple connected regions
are explicitly rejected by the experimental setter. No CPU field solver is added.

The first numerical check projects an analytic periodic pressure-gradient
velocity back to rest, comparing both anchored and floating variants with the
same analytic pressure and physical convergence tolerances. Further operator,
contrast, disconnected-region, rollback and real-case checks are required before
considering adoption. Timing must use otherwise idle GPU runs; the primary
checkout's spatial jet refinement runs concurrently during initial correctness
work.

## Initial results

The uniform periodic-gradient check passes: anchored Jacobi needs 55 iterations,
rank-one Jacobi one, and rank-one multigrid four. Pressure and projected velocity
match the analytic reference. The real 120-step nozzle prefix also passes with
rank-one Jacobi, but still needs about 500 iterations near its endpoint.

Rank-one multigrid fails the real nozzle at step 66, reaching 10,000 iterations
with physical relative residual 1.725e-4 against the unchanged 1e-5 target. It is
not eligible for production. A likely conditioning issue is that the gauge
scale was matched to Jacobi while the V-cycle has a much larger constant-mode
response. This remains a hypothesis to test, rather than a reason to relax the
physical acceptance criterion.

## Preconditioner-aware gauge calibration

The next candidate retains the same physical operator and acceptance tests but
sets `gamma = N / (1^T M 1)` for the fixed multigrid preconditioner `M`.
The rank-one part of the symmetrically preconditioned matrix then has its sole
nonzero eigenvalue equal to one. This matches that artificial gauge scale to the
actual V-cycle instead of its fine-grid Jacobi diagonal. It does not imply that
the full preconditioned matrix has condition number one.

A GPU application of the existing V-cycle to the constant vector obtains the
scalar denominator once per projection. The physical residual and initial
search vector are then rebuilt. The gauge is fixed during each CG solve and its
restarts; no volume buffer or CPU field solve is added.

The periodic analytic test still passes, and the 120-step nozzle prefix that
failed in the original multigrid experiment now passes at unchanged tolerances.
Twelve manufactured projections pass across a 7000:1 density contrast in both
orientations, using odd periodic, closed and thin connected domains; disconnected
setups are rejected. The calibrated prefix has endpoint fraction relative L1
difference 4.19e-9 and velocity relative L2 difference 7.33e-7 from anchored
multigrid.

However, the full stronger-pulse release experiment fails after its 100-step
snapshot and before its 120-step snapshot: the surface-update projection reaches
10,000 iterations with physical relative residual 4.858e-5 against the unchanged
1e-5 target. Thus the short-prefix result did not establish robustness. Neither
rank-one variant is eligible for production. No speedup is claimed.

## Mean-free residual candidate

A subsequent candidate removes the rank-one term entirely, projects each
residual into the constant-orthogonal subspace, and centers pressure increments
by their global mean. Preconditioning induces `P M P` on that quotient space;
the physical residual norm remains unprojected for final acceptance. The
analytic periodic check passes with the same 1/4 floating Jacobi/MG iterations.
The stronger-pulse run again fails between saved steps 100 and 120, reaching
relative physical residual 4.846e-5 after 10,000 iterations. This rules out the
rank-one term as the sole explanation. This version is also excluded from main.

## Reference-gauge mean-free candidate

Keeping the same mean-free residual algorithm but referencing each pressure
increment to the original air-cell search value completes the full 2 ms,
7e7 N/m³ release run. Search lanes are read-only during that update; individual
component stores avoid a cross-invocation read/write race at the reference cell.
This is a gauge choice on the Neumann solution, not a pinned operator row.

The original reference improves floating-point conditioning in this high-density-
contrast case. The earlier mean choice can give air potentials an unnecessary
large common offset. Cancellation is a supported explanation from this control,
not yet an independent error decomposition.

The periodic analytic, twelve density-contrast projections, six Neumann V-cycle
symmetry/positivity checks and four anchored/floating Jacobi/MG failure-and-retry
controls pass. The V-cycle preserves the live work lanes. Disconnected graphs
remain explicitly unsupported by the research method.

Compared with main's anchored multigrid release trajectory:

1. First saved release times match at both fraction cutoffs. Primary core volume
   changes by less than 7.1e-7 relative and fitted speed by less than 2.8e-6.
2. Maximum saved fraction relative L1 difference is 7.60e-5. Final whole-domain
   velocity relative L2 difference is **1.73%**, and the liquid-weighted value is
   **0.773%**. Primary-drop agreement does not mean every local field agrees;
   differences grow late in the trajectory with the unresolved fragment.
3. Peak-normalized kinetic-energy and expelled-volume differences are 3.90e-5
   and 1.70e-5. Maximum sampled speed differs by 3.84% of the reference peak.
4. Both use the original physical relative/absolute acceptance criterion. The
   new method avoids the pinned row's extra internal stopping factor; final RMS
   divergence is 9.68e-4 versus 1.07e-4 s^-1, both below the configured 1e-3
   absolute floor. This difference in excess solve accuracy is explicit.

No runtime comparison is valid yet: another fine-grid job runs concurrently.
Before adoption, compare with a simpler control that checks the existing pinned
solver's physical residual more frequently, then test additional moving cases.

## Cleanup and simpler control

The reference-gauge candidate no longer needs search-vector means. Those
reductions and their scalar bookkeeping have been removed, retaining only the
residual projection and its reduction per iteration. Analytic, Galerkin,
contrast, symmetry and rollback/retry checks still pass. The frozen test binary
for the next paired comparison is `artifacts/floating/meanfree-clean-test`.

`MHD_PRESSURE_CHECK_STRIDE=8|16|32|64` is a research-only control of how often
the existing independent physical check runs; default 64 preserves main's
schedule. It changes no physical acceptance criterion or iteration cap. The
anchored 16-iteration control completes the 2 ms release experiment. Its wall
time overlaps other GPU jobs and is not a performance comparison.

The cleaned candidate's fine-grid prefix and 1.5-period 64³/96³ drop oscillations
are currently running. A separate benchmark driver waits for the offset study
and an explicit idle gate before alternating warmed timing comparisons.

## Coupled V6 and cleanup checks

The cleaned solver completes the 25 µm nozzle prefix: 340 steps through
0.200167 ms, at unchanged physical tolerances. Its timestep is 0.588726 µs,
slightly different from the full fine release run, so this is a scalability
check rather than a matching-frame comparison. Its full coarse 2 ms release
run has **all 122 saved fraction/flow snapshots byte-identical** to the preceding
reference-gauge candidate. Removing unused reductions does not change those
physical fields.

A new ignored `floating_coupled_project_probe` loads a project through the normal
loader, constructs the normal `RunState`, and optionally enables research pressure
before stepping. It saves diagnostics and final fluid/interface fields. It uses
the existing automatic multigrid threshold unless a test explicitly overrides it.
The original prototype's accidental default forced-Jacobi behavior was removed.

The actual V6 project completes three coupled 10 µs ticks with both original and
mean-free pressure, preserving the input files. The 172×112×124 graph is connected.
Pressure iteration totals change 128/128/106 to 45/41/42; EM totals remain
858/864/873. Working memory is identical at 1,809,708,416 bytes after lazy MG
allocation. These are correctness runs overlapping other jobs, not timings.

At the final tick:

1. Velocity relative L2 difference is 1.10e-5, maximum component difference
   2.99e-7 m/s. Pressure relative L2 difference is 3.67e-6.
2. Liquid fraction relative L1 difference is 1.47e-10, with maximum fraction
   difference 5.96e-8. Reported material volume is identical at every tick.
3. EM force relative L2 difference is 6.82e-8; heating and active phase flags
   are identical. Thermal energy differs by less than 1.1e-9 J.
4. Unweighted curvature relative L2 differs **1.95%**, driven by nearly pure
   cells; weighting by `alpha(1-alpha)` gives 2.02e-7. Raw local curvature is
   not claimed identical. The volume-correction lane's maximum absolute change
   is 4.50e-9 in fraction units; its near-zero reference makes relative norms
   uninformative.
5. Final RMS divergence is 8.96e-5 versus 3.16e-6 s^-1, both satisfying the same
   configured 1e-4 absolute floor. The larger relative residual is allowed by
   that physical absolute criterion, which is unchanged.

Reproduce the test-only coupled probe using an existing project:

```sh
MHD_FLOATING_PRESSURE=1 MHD_RESEARCH_PROJECT=/path/to/project.mhd.json \
  MHD_RESEARCH_OUTPUT=artifacts/floating/combined-test \
  cargo test --lib floating_coupled_project_probe -- --ignored --nocapture
```

Omit `MHD_FLOATING_PRESSURE` for the original solver. Optional
`MHD_RESEARCH_PROFILE=1` enables the existing GPU profiler;
`MHD_RESEARCH_TICKS` defaults to three. The complete 64³ moving-drop check also
passes (frequency error 1.58%); the 96³ check is still underway.
