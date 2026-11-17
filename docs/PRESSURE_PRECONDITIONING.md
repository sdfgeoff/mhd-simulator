# Experimental GPU pressure preconditioning

This investigation changes the pressure preconditioner, retaining the fine pressure equation, pressure gauge, tolerances and independent physical-residual check. It is currently compiled only for tests. The application continues to use diagonal-preconditioned conjugate gradients (Jacobi-CG).

## Operator and cycle

For each free pressure cell, the fine operator is the weighted graph Laplacian assembled from cached MAC-face mobilities. Connections to a pressure anchor contribute a diagonal reaction; the anchor itself has zero correction. Walls have no connecting edge.

1. Aggregate adjacent cells in blocks of up to 2×2×2. Piecewise-constant prolongation, `P`, excludes anchored fine cells; restriction is its transpose, `R = Pᵀ`, using sums rather than averages.
2. Construct the coarse graph as `A_c = R A P`. Sum edges crossing aggregate boundaries and accumulate anchor reactions separately. Internal edges cancel analytically. Later levels also sum the existing reactions. This preserves a seven-point graph without subtracting large internal-edge sums.
3. Use a fixed symmetric V-cycle: one weighted-Jacobi sweep before and after each coarse correction, with weight 2/3. The first sweep starts from zero, so it is a diagonal-only operation. The post-sweep uses a separate scratch lane and then commits its result.
4. Stop when every dimension is at most four. Apply eight fixed weighted-Jacobi steps from zero within one workgroup. Equal pre/post smoothing and fixed coarse work preserve the symmetric positive preconditioner required by CG. Empty coarse rows produce zero correction.

Periodic neighbors wrap before aggregate indexing, including odd domain sizes. Coarse self-edges are omitted. The shader uses the existing unsigned periodic-wrap helper; signed negative remainder produced incorrect results on the Vulkan toolchain used for this investigation.

The intended algebra is symmetric in exact arithmetic. GPU tests check bilinear symmetry and positive quadratic forms in floating point. These tests supplement the original physical-equation acceptance check; they do not replace it. Symmetric smoothing for CG is also discussed in the [HYPRE primary documentation](https://hypre.readthedocs.io/en/stable/solvers-boomeramg.html).

## Storage and lifetime

The fine correction uses `work.w`; Jacobi ping-pong uses `predicted.w`. During projection, accepted velocity remains in the existing checkpoint/previous buffers, candidate velocity uses `predicted.xyz`, and the cached diagonal remains available in `velocity[cell_index].w`. The preconditioner preserves `work.xyz`, candidate velocity components, display pressure and all interface/temperature/phase lanes.

Only coarse levels allocate new volume storage: two vec4 packets per coarse cell, plus small immutable level uniforms. At 256³, the hierarchy contains 2,396,736 coarse cells and its two data buffers need 76,695,552 bytes (73.14 MiB). Thin odd domains require their actual level sum: a 33×1×1 domain has 34 coarse cells, so the usual three-dimensional `N/7` estimate is inappropriate there.

The prototype checks coordinate/index arithmetic, individual device buffer limits and the live driver memory budget before allocation. Production adoption still needs optional-allocation fallback and integration with displayed working-memory accounting. No fixed memory budget is introduced.

## Verification

The headless probes cover:

1. GPU coarse action versus the restricted GPU child action at every level, plus the prototype fine action versus the existing GPU pressure stencil.
2. Density contrast 7000:1 in both orientations; odd periodic, closed, disconnected, thin and small domains.
3. Preconditioner symmetry and positivity, along with unchanged live vector lanes and display packets.
4. Manufactured gradient projection using the existing physical-residual acceptance check.
5. Fused coarse iteration versus the same fixed Jacobi sequence, and late pressure failure after stabilized viscosity with byte-for-byte restoration and a successful retry.

Run the regular probes with:

```sh
cargo test --offline --lib fluid::multigrid -- --nocapture
cargo test --offline --lib failed_projection_after_stabilized -- --nocapture
```

The opt-in coupled diagnostic uses the same drop setup and timestep as the reference path:

```sh
MHD_DROP_GRIDS=96 MHD_DROP_PERIODS=0.02 MHD_DROP_PROFILE=1 \
MHD_DROP_PRESSURE_MG=1 \
cargo test --offline --lib rayleigh_lamb_drop_oscillation -- --ignored --nocapture
```

Omit `MHD_DROP_PRESSURE_MG` for Jacobi-CG. Omit `MHD_DROP_PROFILE` when measuring process time without timestamp instrumentation. Short runs check pressure acceptance, volume and bounded shape; they do not establish long-time oscillation accuracy.

## Measurements and remaining limits

The initial two-pre/two-post-sweep prototype reduced iterations but gave no coupled speedup at 64³. Three alternating warmed pairs measured median process time **1.175 → 1.189 s** and pressure GPU time **261 → 268 ms** over the same 13 ticks. This motivated cheaper smoothing and dispatch work.

The current one-pre/one-post candidate uses eight coarse sweeps and batches the outer kernels into one compute pass per CG batch. Three alternating warmed pairs give:

| Quantity | Jacobi-CG | Prototype |
| --- | ---: | ---: |
| 64³ median process time, profiling off | 1.190 s | 1.152 s |
| 64³ median pressure GPU time, separate profiled runs | 261.39 ms | 267.14 ms |
| 96³ median process time, profiling off | 5.826 s | 5.249 s |
| 96³ median pressure GPU time, separate profiled runs | 2,286.02 ms | 1,785.00 ms |

The 64³ run contains 13 ticks and the 96³ run contains 24 ticks, each spanning the same 0.02 drop periods. Every profiling pass was recorded. Driver-cache warmup was excluded. The small 64³ process-time difference does not establish a pressure GPU improvement; 96³ shows approximately 9.9% lower unprofiled process time and 21.9% lower pressure GPU time.

Combining compute passes preserves the previous eight-sweep candidate's entire short 96³ physical CSV byte-for-byte. Its final quadrupole is 0.0297699759853 versus 0.0297699758589 for Jacobi-CG. These short synthetic measurements on the development GPU are not a general hardware or project-wide speedup claim. Longer coupled checks remain necessary.

The manufactured benchmark is also sensitive to the right-hand side. A smooth 64³ gradient failed the unchanged relative target of 1e-5 in **both** methods even after 20,000 iterations: true relative residuals were approximately 2.91e-5 and 3.79e-5. A binary triangular potential reached that target with both methods and strongly favored multigrid, but does not resolve the smooth-case precision limit. The benchmark keeps the target intact; the coupled path must continue to pass the physical check.

A default-method decision needs complete solve costs, unprofiled timings, representative project geometries and longer coupled trajectories. A large isolated iteration-count improvement is insufficient evidence.
