# Floating pressure gauge research

This is an uncommitted, test-only experiment in a separate checkout. Production
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
and partial buffers. No volume-sized field is added. Jacobi is used for this
initial experiment; existing multigrid is disabled. Multiple connected regions
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
