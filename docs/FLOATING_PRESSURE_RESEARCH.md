# Floating pressure gauge research

This is a test-only experiment preserved on a separate research branch. Production
on main is unchanged. No performance or general-correctness claim has been made.

The physical closed/periodic pressure graph is a Neumann Laplacian with one
constant null vector per connected region. The current solver pins one cell
per region. Hypothesis: the point constraint adds poorly conditioned modes that
cost more than treating the constant mode globally. The primary
[PETSc null-space documentation](https://petsc.org/release/manualpages/Mat/MatSetNullSpace/)
explains compatibility with the null space and orthogonalized right-hand sides;
it does not describe or validate this implementation.

For exactly one connected region, the prototype uses
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
