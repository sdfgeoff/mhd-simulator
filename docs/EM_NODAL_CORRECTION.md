# Gradient correction for long induction solves

Long induction solves use a matrix-free GPU gradient preconditioner for the
existing edge-vector-potential equation. It does not change conductivity, the curl/gauge
operator, the physical right-hand side or the true-residual acceptance criterion.
Induction steps first use the original Jacobi preconditioner. If a solve remains
unfinished after 1,024 iterations, it restarts from the true residual with this
additive correction. Both phases share the original iteration cap and RHS norm.
The threshold controls numerical effort, not a physical material cutoff.

Applying the correction from the first iteration can worsen some cases: a local
V6 control then failed its 1,000-iteration cap. The fallback preserves that
ordinary Jacobi path exactly. Workspace is allocated only when switching; short
solves retain their original working-buffer bytes. Allocation rejection after a
partial solve rolls back reaction/source history and leaves accepted fields and
time unchanged.

Let `A` be the existing edge operator, `D_A` its diagonal, and `G` the oriented
node-to-edge incidence gradient with zero scalar values on the outer boundary.
The fallback applies

```
B r = D_A^-1 r + G diag(G^T A G)^-1 G^T r.
```

The exact incidence identity `C G = 0` removes the curl term from the nodal
diagonal. Its remaining contributions are the incident conductive masses and
`36*air(node) + sum(air(neighbour))` from the existing air-only gauge. This is a
single additive diagonal correction in gradient space, **not a multilevel AMS
implementation**. Auxiliary-gradient methods for curl–curl/mass problems are
described in the primary [hypre AMS documentation](https://hypre.readthedocs.io/en/latest/solvers-ams.html).

The correction is symmetric positive semidefinite; adding it to the strictly
positive edge Jacobi inverse gives a symmetric positive preconditioner. Two
compute dispatches form the nodal scalar and add its gradient before the existing
PCG reductions. The dead operator-product slot carries the preconditioned
residual. A separate bind group keeps each entry point below eight storage
bindings. The extra persistent storage is one f32 per grid node. Node count,
adapter buffer limits and live available memory are checked before allocation.
No duplicate CPU field solver or float atomics are used.

The regular GPU checks cover field agreement with original Jacobi, bilinear
symmetry/positivity, changed timesteps and failed-step rollback/retry. All
convergence refreshes, including weak-field scaling and prediction fallback,
apply the same preconditioner before restarting CG.

For a reproducible coupled investigation, run the ignored project probe with
`MHD_RESEARCH_PROJECT`, `MHD_RESEARCH_TICKS`, `MHD_RESEARCH_OUTPUT`,
`MHD_EM_PREDICT=1` and `MHD_EM_NODAL=1`. Omit the last variable for the original
preconditioner. `MHD_EM_NODAL_AFTER` overrides the research switch threshold;
production uses 1,024 iterations. The probe writes per-step diagnostics and final
fields and uses the normal combined run engine. These controls are test-only, not GUI options.

## Recreate the stronger-drive control

From the repository root (Python is only preparing JSON here):

```sh
python3 - <<'PY'
import json
from pathlib import Path
root = Path.cwd()
p = json.loads((root / 'assets/nozzle-coil.mhd.json').read_text())
p['geometry'] = str(root / 'assets/nozzle-coil.gltf')
p['domain']['min_mm'][2] = -3.0
p['domain']['max_mm'][2] = 4.0
for wire in p['electromagnetics']['wires']:
    wire['current'].update(peak=500.0, hold_s=0.0015)
out = root / 'artifacts/strong-coil.mhd.json'
out.parent.mkdir(exist_ok=True)
out.write_text(json.dumps(p, indent=2))
PY
```

Run the production application headlessly, or use this project path with the
coupled probe:

```sh
./target/debug/mhd-workbench --run-check artifacts/strong-coil.mhd.json 150 \
  --output artifacts/strong-coil-output --field-every 50
```

The original production solver accepts through 60.5 µs and rejects 61 µs at its unchanged 4,000-iteration cap.
The target remains true relative residual 10⁻⁴. This is a solver stress case with
ideal current sources and uncalibrated liquid properties, not a proposed drive
circuit or a validated jetting threshold.

## Strong-drive control

The fallback completes **150 ticks to 75 µs**, with the same 0.5 µs common
timestep and 4,000-iteration cap. Maximum EM iterations are **2,820**; maximum
relative material-volume drift is **6.52×10⁻⁸**. The accepted fields come from the
normal combined run engine.

At the original failed **61 µs** step, the fallback uses **2,410 total
iterations**, including the first 1,024 Jacobi iterations, and reaches true
relative residual **9.59×10⁻⁵** against the unchanged **10⁻⁴** target. Extra
working storage is **3,005,556 bytes** (751,389 scalar nodes).

Over the shared accepted prefix to 60.5 µs, maximum B, maximum speed and kinetic
energy differ from the original run by respectively **6.18×10⁻⁷**, **2.60×10⁻⁷**
and **1.66×10⁻⁷**, each normalized by that diagnostic's original peak. Material
volume diagnostics are identical. The small-grid field-agreement check compares
B, J, force and heating separately; maximum relative L2 difference is below
**6×10⁻⁶**.

The test remains a solver stress case. It does not establish convergence through
the full 1.5 ms current hold, later breakup, or a coil-driven droplet. Iteration
counts are still high; the diagonal gradient correction does not replace a
stronger multilevel preconditioner for general cases. No wall-time speedup is
asserted from this stress run.

## Ordinary-case and failure checks

The local V6 ten-tick control remains **byte-identical** to original Jacobi in
final EM, fluid and interface buffers and every recorded diagnostic. It uses at
most **858 iterations** under its original 1,000-iteration cap; the fallback does
not allocate, so working-buffer bytes are unchanged. No runtime improvement is
claimed from this comparison.

The full suite passes **167 regular headless tests** (20 longer checks remain
explicitly ignored), with strict Clippy clean. Tests force an early switch to
check changed-timestep field agreement, rejection/rollback of an unindexable
workspace, capped-step failure and successful retry. A short-solve control checks
byte-identical results without workspace allocation. The weak-drive test forces
the gradient correction at both original and 10⁻⁸ source amplitude, checks the
physical residual inequality and linear/quadratic field scaling.

The native 100 A application control also completes **400 ticks to 200 µs**,
including switch-off. Maximum EM iterations are **467**, so the fallback remains
inactive. Final exported EM/fluid packets are finite; maximum material-volume
drift is **6.52×10⁻⁸**. This is a continuation check, not a droplet prediction.
