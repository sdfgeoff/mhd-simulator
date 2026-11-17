# Extending curvature height columns

Curvature now tries eleven- and thirteen-cell columns when the existing seven-
and nine-cell columns fail. This is integrated into the ordinary moving-interface
solver. PLIC transport normals retain their seven/nine-cell stencil, isolating
this change to curvature and the resulting force.

## Construction

Each column still needs opposing endpoint fractions above 0.99 / below 0.01,
monotone occupancy to within 10⁻⁵, and the same orientation across the 3×3 height
patch. A successful shorter column is unchanged. Incomplete, multiple-crossing
or out-of-domain columns retain the normal-divergence fallback. Periodic columns
use the existing wrapped indexing. All lengths subtract their radius from the
occupied sum, preserving a common height origin.

No volume buffers, settings, timesteps or solver tolerances change. The wider
columns only run after shorter columns fail. A direct GPU regression checks a
thirteen-cell-only crossing in both orientations, common origins, pure phases
and rejection of extra crossings. The complete-column motivation is discussed
in [Basilisk's height construction](https://basilisk.fr/src/heights.h); this
implementation reuses this solver's existing column and Hessian formulas.

The [read-only diagnostic](CAPILLARY_ENERGY.md#read-only-height-column-investigation)
identified coverage, rather than endpoint truncation, as the useful target.
With the longer production columns, initial relative curvature RMS error over
all mixed cells on the R=0.2 m sphere changes as follows:

| Grid | Centered, before → after | Half-cell shift, before → after |
| --- | ---: | ---: |
| 48³ | 3.95% → 2.13% | 3.07% → 1.73% |
| 64³ | 2.46% → 1.63% | 3.39% → 0.839% |
| 96³ | 1.28% → 0.267% | 1.11% → 0.278% |

## Dynamic measurements

The [capillary-energy setup](CAPILLARY_ENERGY.md) uses density 7000:1, zero
viscosity and gravity, and unchanged pressure acceptance. Over half a reference
period, initially static drops give:

| Grid / placement | Peak kinetic energy before | After |
| --- | ---: | ---: |
| 48³ centered | 22.93 µJ | 6.296 µJ |
| 64³ centered | 2.783 µJ | 0.9784 µJ |
| 64³ half-cell shift | 4.145 µJ | 1.228 µJ |
| 96³ centered | 0.9862 µJ | 0.02728 µJ |

Full 1.5-period moving-drop checks pass the existing 3% frequency, bounded-mode
and volume criteria at 64³/96³. Frequency errors change **2.65% → 1.59%** and
**0.668% → 0.0134%**, respectively. The very small latter error is a single
benchmark result, not a general accuracy guarantee. Local velocity noise,
area-estimation uncertainty and spatial bias remain. These measurements do not
validate energy conservation, pinch-off or jetting thresholds.

## Short regression tradeoff

The original five-tick R=0.25 m test required the maximum cell-centre speed to
fall between 24³ and 48³. The new stencil fails that particular trend:
24³ stays 2.642 µm/s, while 48³ rises **2.396 → 2.850 µm/s**. Its pressure-jump
error also changes **0.587% → 0.638%**, still below the 24³ error of 2.07%.
These increases are retained as measured limitations. More accurate initial
fractions and a tighter absolute pressure tolerance do not remove the reversal.

However, 48³ density-weighted kinetic energy falls **8.522×10⁻¹² →
6.628×10⁻¹² J**, and is 94% below the 24³ value. The regression now checks energy
refinement together with a 10 µm/s absolute speed bound on both grids; the
pressure-error refinement and exact-curvature balance checks remain. This
replaces a local-peak trend with a measure of total numerical motion rather
than claiming that every velocity error improved.

A new regular 48³/64³ static-drop test follows the first quarter period, requires
peak kinetic energy to refine, and bounds it below 10⁻⁴ of the analytic equilibrium
surface energy `σ 4πR²`. It uses the existing GPU kinetic-energy diagnostic, not
either evolving area proxy. This is a bound on numerical motion, not a verified
surface-energy balance.

```sh
cargo test --offline --lib surface::height_diagnostics -- --nocapture
cargo test --offline --lib initially_static_drop_kinetic_energy -- --nocapture
MHD_DROP_GRIDS=64,96 MHD_DROP_PERIODS=1.5 \
  cargo test --offline --lib rayleigh_lamb_drop_oscillation -- --ignored --nocapture
```

## Combined-run cost

Three alternating warmed executable pairs ran three EM/fluid/thermal ticks of
the local V6 hotend, preserving the original project and GLB hashes. Median
interface/heat GPU time changes **229.60 → 230.08 ms**; process time is
**26.031 → 25.814 s**. These small differences do not establish an overall
speedup. EM remains dominant at about 20.9 s. Every profiling pass is recorded;
startup/warmup runs are excluded from these medians. Working storage remains
1,715.4 MiB, with no additional volume buffer.

The new curvature intentionally changes fluid trajectories: the maximum
recorded-speed difference in this short probe is 6.86 mm/s. Both paths satisfy
the existing physical pressure criterion; material-volume differences stay
below 5.7×10⁻¹⁴ m³ and maximum-B differences below 1.3×10⁻⁸ T. This is a combined
solver smoke/performance check, not a calibrated hotend-velocity validation.
The development adapter is an NVIDIA GeForce RTX 3060 Laptop GPU.

The complete headless suite passes 135 tests (112 library and 23 application),
with strict all-target Clippy clean. The source-equation residuals, failure
rollback, coupled thermal/material transport and offscreen rendering checks
remain in that suite.
