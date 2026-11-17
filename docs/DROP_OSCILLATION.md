# Three-dimensional capillary drop oscillation

This benchmark checks the shape dynamics of a freely moving liquid/ambient interface, beyond the planar standing-wave test. A short restoring-shape check is in the regular suite; the longer grid-refinement test runs for 1.5 oscillation periods.

## Reference and measurement

For a small radial deformation in spherical harmonic mode l, potential flow inside the drop varies as rˡ and the exterior potential as r⁻ˡ⁻¹. Matching normal velocity at radius R gives interface potentials proportional to R/l and −R/(l+1). The unsteady pressure jump and linearized Laplace curvature then give

ωₗ² = l(l−1)(l+1)(l+2) σ / [R³ ((l+1)ρliquid + lρambient)].

For l=2 this is **ω² = 24σ/[R³(3ρliquid+2ρambient)]**. In the negligible-ambient-density limit it reduces to Rayleigh's 8σ/(ρR³). [NASA's oscillating-drop measurement presentation](https://ntrs.nasa.gov/api/citations/20210011992/downloads/SanSoucie%20-%20UAH%20Presentation.pdf) describes the surface-tension/frequency method; [Yang et al. (2014)](https://doi.org/10.1063/1.4901823) discusses the small-amplitude frequency and viscous-damping reference.

The unit cube contains a volume-preserving ellipsoid with axes R/√(1+ε), R/√(1+ε), R(1+ε), using R=0.2 m and ε=0.03. Initial cell fractions integrate ellipsoid chords; initial velocity, gravity and both viscosities are zero. Density ratio is 7000:1 and σ=1 N/m. These dimensions define a numerical benchmark, not an apparatus-sized drop. The surrounding box is impermeable; its influence on the infinite-ambient reference is small at this density contrast, but not assumed negligible for arbitrary density ratios.

A central second-moment quadrupole measures the shape, removing centre-of-mass drift. Its normalized value initially equals one. The reference radius is the volume-equivalent radius of the initialized fractions, avoiding initial quadrature-volume bias without changing or renormalizing those fractions. Frequency comes from the first and third alternating zero crossings.

## Earlier seven-cell-stencil measurements

These measurements precede extended height columns and bulk MC reconstruction.

| Grid | Cells across initial diameter | Measured ω, rad/s | Frequency error |
| --- | ---: | ---: | ---: |
| 48³ | 19.2 | 0.384901 | 1.84% |
| 64³ | 25.6 | 0.382886 | 1.31% |

The reference is approximately 0.377947 rad/s. Relative volume drift stays below 2×10⁻⁷ over the long checks, and the measured quadrupole amplitude does not exceed its initial value. These are frequency/conservation checks, not an energy-conserving or viscous-damping result.

Coarse grids show why resolution matters. At 0.35 reference periods, the normalized shape should be −0.588:

| Grid / timestep | Measured normalized shape |
| --- | ---: |
| 24³, 9.6 cells across diameter | +0.743 |
| 32³, 12.8 cells across diameter | −0.260 |
| 32³, timestep divided by four | −0.252 |
| 48³, 19.2 cells across diameter | −0.574 |

The coarse 24³ drop has not crossed its spherical state. Reducing the 32³ timestep fourfold barely changes its substantial shape error, indicating spatial resolution is the main limitation in this probe. The resolved-grid frequency trend does not establish a universal cell-count threshold for breakup.

## Extended heights and limited bulk reconstruction

The current scheme extends incomplete height columns from seven to nine cells, with a common origin between lengths, and filters transverse 3D height second differences. Momentum uses MC–Hancock reconstruction in homogeneous phase stencils and upwind velocity near density jumps. Both geometry and momentum matter; the isolated shear accuracy gain alone did not fix capillary behavior.

| Grid | Cells across initial diameter | Measured ω, rad/s | Frequency error |
|---|---:|---:|---:|
| 48³ | 19.2 | 0.383455 | 1.46% |
| 64³ | 25.6 | 0.387979 | 2.65% |
| 80³ | 32.0 | 0.387836 | 2.62% |
| 96³ | 38.4 | 0.380570 | 0.69% |

The coarse results are nonmonotonic. The substantial reduction at 96³ does not establish a clean convergence order. The default diagnostic includes all four grids, retains the 3% frequency bound at each grid, and requires the final error to be smaller than the first. A separate 48³/64³-only run still fails that refinement assertion.

At 64³, shifting the initial drop by half a cell along all three axes gives 2.32% frequency error, compared with 2.65% when centered. The shifted case completes three reference periods without mode-amplitude growth or volume drift above 6×10⁻⁸. Its final quadrupole is only 0.663 of the initial value, compared with a linear inviscid reference of one: appreciable numerical damping and phase error remain. Completion of the run does not establish energy conservation. Halving the centered 64³ timestep changes frequency error from 2.65% to 2.61%, leaving the spatial bias largely intact.

## Run it

    cargo test --offline three_dimensional_drop_restores -- --nocapture
    cargo test --offline rayleigh_lamb_drop_oscillation -- --ignored --nocapture

Optional investigation controls:

    MHD_DROP_GRIDS=24,32,48 MHD_DROP_PERIODS=0.35 cargo test --offline rayleigh_lamb_drop_oscillation -- --ignored --nocapture
    MHD_DROP_GRIDS=32 MHD_DROP_PERIODS=0.35 MHD_DROP_DT_FACTOR=0.1 cargo test --offline rayleigh_lamb_drop_oscillation -- --ignored --nocapture

Default refinement uses 48³/64³/80³/96³ grids and 0.4 of the capillary timestep bound. This is an explicit long GPU diagnostic. Other controls are MHD_DROP_DEFORMATION and MHD_DROP_DENSITY_RATIO. MHD_DROP_OFFSET_CELLS shifts the centre by up to ±0.5 cell along every axis. Short probes report shape/conservation; runs of at least 1.3 reference periods must complete three half-period crossings and pass the frequency check. Insufficiently resolved cases can fail that check.

CSV files artifacts/drop-oscillation-N.csv contain the quadrupole, analytic trace, volume/centroid drift, speed, divergence, fraction bounds and the initial centre. Accepted trajectory rows are also saved if a subsequent solver step fails.

## Inspecting height coverage

An optional GPU diagnostic reports which mixed cells have complete height stencils and saves artifacts/height-coverage-N.csv:

    MHD_HEIGHT_GRID=64 MHD_HEIGHT_OFFSET=0.5 cargo test --offline drop_height_function_coverage -- --ignored --nocapture

MHD_DROP_DUMP_GEOMETRY=1 saves raw accepted geometry every 100 steps and at completion. Set MHD_HEIGHT_INPUT to one of those artifacts/drop-geometry-N-STEP.bin files to inspect an evolved interface while retaining its recorded curvature. The coverage CSV also reports a hypothetical eleven-cell stencil for research; production still uses seven/nine cells.

## Limits

The default diagnostic uses analytically integrated fractions. The CAD option below measures imported-model initialization separately. The benchmark does not yet establish accurate local velocities, physical viscous damping, nozzle pinch-off or satellite sizes. Curvature fallback, arithmetic face density, upwind transport near density jumps and splitting still introduce errors; further refinement and additional observables are needed for those claims.

## CAD initialization option

Set MHD_DROP_CAD=1 to initialize the diagnostic from a tessellated CAD ellipsoid through the production GPU fraction initializer. Its output is artifacts/drop-oscillation-cad-N.csv. The default diagnostic retains independently integrated analytic fractions so geometric initialization error can be assessed separately. Both paths use the same fluid/surface kernels.

    MHD_DROP_CAD=1 MHD_DROP_GRIDS=48 MHD_DROP_PERIODS=1.5 cargo test rayleigh_lamb_drop_oscillation -- --ignored --nocapture

The following CAD refinement measurements precede the height-stencil update. MHD_DROP_CAD_LATITUDES controls the diagnostic mesh: longitude divisions are twice the latitude count. Its default of 64 produces 16,128 triangles. With that fixed mesh, frequency errors are 1.52% at 48³ and 2.45% at 64³: the paired-grid refinement assertion fails. Good volume conservation does not establish curvature accuracy.

At 64³, doubling to 128 latitude divisions (65,024 triangles) reduces frequency error to 1.31%, with relative volume drift below 4×10⁻⁸ and no measured quadrupole amplification. A further refinement to 256 latitude divisions (261,120 triangles) gives 1.24% error, also without quadrupole amplification. These are close to the 1.31% analytic-initialization frequency error at that grid. Refining the fluid grid alone while holding a coarse curved CAD tessellation fixed can therefore give a misleading convergence trend.

See [CAD initialization](CAD_INITIALIZATION.md) for static fraction measurements and the remaining wall/tessellation limitations.

For nonzero liquid viscosity, [the viscous-drop diagnostic](VISCOUS_DROP.md) uses the full linear viscous frequency and reports decay fits over several time windows. It also documents the remaining damping error.
