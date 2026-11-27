# Uniform-grid pressure and connectivity benchmarks

The comparison uses the 0.6 mm outlet assembly on the RTX 3060 Laptop, with the same 224 × 204 × 259 voxel grid, three 3 µs timesteps, electrical drive, GPU precision and physical acceptance criteria. The reference is `c633635`. Timings are summed from the native iteration-costs CSV and exclude loading, shader compilation and exports. The trajectory covers the first 9 µs.

## Full-assembly replay

| Measurement over three steps | Reference | Revised | Speedup |
|---|---:|---:|---:|
| Coupled stepping | 47.90 s | 33.18 s | 1.44× |
| Both pressure projections | 29.84 s | 14.62 s | 2.04× |
| Charge solve | 17.15 s | 17.60 s | 0.97× |
| Full fluid step | 30.75 s | 15.58 s | 1.97× |

Reference stepping times were 46.55 s and 49.25 s. Revised times were 33.34 s and 33.02 s. Each replay started from the original assembly. The benchmark compressed each initial field after publication, which could overlap stepping, and compressed the final field after termination. Repeat exports matched byte for byte within each build.

Pressure iterations went from 640, 576 and 610 to 236, 220 and 247. At step three the relative L2 differences across the exported field were 0.00317% in velocity and 0.0000327% in pressure. Maximum absolute liquid-fraction difference was 1.17 × 10⁻¹⁰ or less, and solid masks matched. Both builds passed their physical acceptance gates. Full-pulse speed, late breakup and performance on AMD remain unmeasured.

## Where the work is

1. The initial state has 6,246,748 electrical unknowns (52.8% of the box). They occupy 30,768 of 46,232 electrical workgroups. Packing their lanes perfectly would shrink the already scheduled inner stencil from 7,876,608 lanes to 6,246,748, a factor of 1.26 before indexing costs.
2. Pressure includes the gas. Its 5,543,488 fluid cells form five six-face-connected components of sizes 5,534,160, 4,032, 4,032, 632 and 632. The previous solver pinned one pressure row in each because the domain was disconnected. The new solver projects the largest component's residual to zero mean and retains a separate pressure reference in every component. This choice follows the voxel connectivity of each assembly and refreshes when phase connectivity changes.
3. The size of the largest pinned region bounds the contribution of the omitted reference rows. Only their part of the residual pays the additional bound for the omitted rows. An independent check of the original physical equation at every fluid cell decides acceptance. The solver also checks at a batch boundary when its recursive norm first reaches the nominal tolerance and when that norm subsequently halves. The pressure method is described in [Pressure subspaces](PRESSURE_SUBSPACE.md).
4. Each intermediate multigrid level initializes its first Jacobi correction while restricting the residual. Prolongation reads the preceding smoother's result directly. This eliminates twelve full-level kernel dispatches per V-cycle on this grid. In isolation the change produced a byte-identical final export.

## CPU preparation

Component labelling now fills contiguous x runs and checks their overlapping neighbours in y and z, including periodic seams. The packed cell mask holds the reference labels during traversal. This saves the 45.1 MiB auxiliary root array on this assembly. Six alternating comparisons on the identical full-assembly mask gave median labelling times of 0.375 s for voxel-by-voxel flood fill and 0.087 s for the span fill. Every cell's component and reference agreed. Another 240 comparisons covered thin and odd grids, sparse and dense masks and all eight combinations of periodic axes. This gain applies at setup or a geometry connectivity change.

## Experiments discarded

1. About 94% of the initial electrical unknowns had six matching material neighbours. A specialized interior matrix stencil increased three-step charge time from 16.78 s to 17.58 s in the trial. The general electrical stencil remains in use.
2. Doubling the multigrid coarse correction increased pressure iterations to 832, 696 and 682, compared with 640, 576 and 610 on the reference. The retained cycle uses its existing correction weight.

## Validation

All Cargo targets compiled. Twenty-five targeted fluid, pressure, multigrid and connectivity tests passed, including physical pressure projection on disconnected domains, changes in the largest component, periodic seams, high density contrast and rollback on failure.

## Reproduction

Build `conduction_flow` with the same offline Cargo dev profile in both checkouts. The replay harness takes the two executable paths and a fresh output directory. Select the same Vulkan adapter for both. Four uncompressed replays need about 5.7 GiB for their fields.

```sh
python3 scripts/benchmark_conduction_flow.py BASELINE_BINARY CANDIDATE_BINARY OUTPUT_DIRECTORY --steps 3 --pairs 2
cargo test --offline --lib fluid:: -- --test-threads=1
```
