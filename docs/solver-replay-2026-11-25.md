# Coupled solver replay on the RTX 3060 Laptop

The workload advances the same 11,835,264-cell assembly through three accepted 3 µs steps. The baseline is commit `c118ad7`. Both executables use the offline Cargo dev profile, the same Vulkan driver, CAD, source quadrature, waveform, pressure preconditioner, iteration caps and acceptance tolerances. Each process starts from the initial state and writes the initial and final fields. This measures the beginning of the current rise. A whole pulse and the larger AMD case have different workloads.

## Benchmark results

Two warmed repetitions per build ran in baseline, candidate, candidate, baseline order. Values are means per three steps. Solver wall time excludes startup and exports. GPU pass time comes from timestamp queries.

| Measurement | Baseline | Candidate | Speedup |
|---|---:|---:|---:|
| Coupled stepping, wall | 50.751 s | 45.532 s | 1.11× |
| Electrical solve, wall | 20.625 s | 16.787 s | 1.23× |
| Fluid step, wall | 30.126 s | 28.745 s | 1.05× |
| Pressure, GPU passes | 26.885 s | 25.568 s | 1.05× |
| Interface and heat transport, GPU passes | 0.608 s | 0.535 s | 1.14× |
| Complete process, wall | 58.196 s | 52.950 s | 1.10× |

The warmed step totals were 50.58 s, 50.92 s for baseline and 45.56 s, 45.50 s for the candidate. Sampled peak GPU memory was 4,038 MiB versus 4,084 MiB. In the first instrumented pair the candidate process took 66.17 s versus 57.40 s for baseline, despite faster stepping. Startup and export therefore matter for short jobs.

Within the warmed candidate runs the initial pressure projection averaged 25.09 s summed over three steps and the post-transport projection 2.77 s. The unit-voltage and motional bases took 2.76 s and 13.98 s respectively.

## Changes and isolated measurements

1. **Charge.** Prepare the conductivity diagonal once per accepted phase field. Both potential solves reuse it. Build a list of 256-cell groups containing unknowns, then dispatch the inner CG stencils over that list when fewer than three quarters of the groups are active. Each group writes its original reduction slot. Groups repopulate when liquid moves. A 129 × 128 × 128 uniform-bar benchmark isolated this scheduling change. A 12 × 12 conducting cross section used 84 of 8,256 groups and sped up 3.39×. A 48 × 48 section sped up 1.69×. The full cross section used the dense path and took about 0.27 s in either mode. Each pair produced identical currents, true residuals and iteration counts, and the currents agreed with the analytic resistance.

2. **Pressure and reductions.** Fuse the finest multigrid correction copy with its dot product, use direct neighbour strides on closed boxes and reduce large partial arrays in two stages in pressure and charge. Periodic boundaries retain their existing neighbour handling. The second pressure reduction uses an existing scratch bank. The new electrical scratch accounts for most of the additional GPU memory. The relative cost of pressure across repeated replays is in the table above.

3. **Large dispatches.** At 85,215 useful groups the electrical solver launches 85,216 groups, and the other volume kernels launch 85,248. Both formerly launched 131,070. A GPU regression exercised the volume kernel across row boundaries and the last partial group at this size. The counts measure dispatch coverage. The assembly timing uses a smaller grid.

4. **Interface transport.** Each directional sweep builds one lower-face flux per cell, including displacement, heat and the invalid-CFL flag. Occupancy, enthalpy and momentum share the result. The next occupancy packet and the normal buffer provide scratch in successive passes, with zero additional field storage. The final native fields agreed byte for byte with a staged version that reused only the liquid flux in momentum. Periodic transport, wall closure, conservation and rollback have regression coverage.

5. **Viewing and reports.** Timeline scrubbing uploads the selected physics field and includes fluid when needed for the moving surface overlay. Full recordings stay available in RAM. Offline reports cache each accepted row and its central sections. Cache keys cover the field identity and timestamps, metadata, exit plane, analysis code and library versions. On two real 723 MiB exports the plot and report took 3.85 s cold and 0.88 s warm. The complete JSON agreed with the previous reader. On a synthetic 128³ field with 238 detached regions, bounding each label lookup reduced median component-analysis time from 1.378 s to 0.036 s across five alternating trials. Results matched exactly. A single-region control took 0.0265 s before and 0.0257 s after.

## Numerical checks and rejected experiment

The new reduction tree changes floating-point summation order. The baseline electrical iteration counts were 992, 1888 and 1856. The retained version took 704, 1888 and 1888. Baseline pressure iterations were 640, 576 and 612, compared with 640, 576 and 610. Each run passed its physical residual and port-current checks. Across the complete final export, relative L2 differences were 0.258% for velocity and 0.00110% for pressure. Maximum absolute liquid-fraction difference was 8.77 × 10⁻⁹ or less, and the solid masks were identical. Early-step agreement leaves later breakup behaviour unmeasured.

A separate experiment rescaled the preceding motional potential to minimize the initial residual along that direction. It reduced the third electrical solve by 64 iterations, but across three steps the solver took 46.14 s, within the range of the retained version. The experiment was dropped. Stage timings locate the main remaining cost in the first pressure projection and, once the fluid moves, the motional charge solve.

## Reproduction

Build the baseline and candidate with `cargo build --offline --example conduction_flow` in their respective checkouts. Supply distinct executable paths and a fresh output directory. The harness alternates execution order on successive pairs and records solver and process wall time. The optional NVIDIA sampler records an observed per-process peak approximately every half second.

```sh
python3 scripts/benchmark_conduction_flow.py BASELINE_BINARY CANDIDATE_BINARY OUTPUT_DIRECTORY --pairs 2 --sample-nvidia
cargo test --offline --example conduction_probe sparse_and_dense_conductor_timing -- --ignored --nocapture --test-threads=1
```
