# CAD liquid fractions at startup

Enable **Moving liquid / air interface** to initialize the moving material from its included CAD solids. The unified run computes cell volume fractions before initializing electromagnetic conductivity, fluid density and thermal phase. This happens automatically when starting a new run; the Setup voxel preview remains a cell-centre occupancy view.

The mesh must be closed and consistently oriented. Reversing every face of a body is supported. A cavity must have the opposite orientation to the enclosing exterior. Incorrectly oriented adjacent faces are rejected, and the GPU checks for invalid integrated lengths. Bodies must have disjoint interiors; this is not a general Boolean union or a complete mesh-intersection validator. Fixed CAD walls use their cell-centre mask and take precedence over liquid in those cells. Liquid fractions are projected into that voxel wall domain as described below. Subcell wall apertures and prescribed material contact angles remain absent.

## Raster integral

For a closed outward-oriented solid, its occupied length along a vertical ray in the cell interval [z₀,z₁] is

    L = Σ sign(n_z) clamp(z_intersection − z₀, 0, h).

Here h is the cell spacing and the sum is over mesh faces crossed by the ray. Entry faces subtract and exit faces add. Contributions outside the cell cancel or vanish. Hardware rasterization supplies the XY crossings and interpolated intersection heights. Vertical faces have zero projected area.

Each cell averages L/h over a fixed 16×16 midpoint quadrature in XY. Z integration is exact for a ray through the planar mesh faces, up to interpolation and fixed-point roundoff; XY quadrature and CAD tessellation still introduce error. This is not an analytic cell/polyhedron intersection.

Fragment shaders accumulate signed contributions with 20 fractional bits in integer atomics. Integer addition makes accumulation independent of fragment arrival order and avoids optional floating-point blending features. Per-ray crossing counts bound rounding tolerance. Small roundoff excursions are clamped locally; no global mass renormalization is performed.

A streamed tile covers at most 64×64 coarse cells in XY. Its 1024² sampling buffer uses eight bytes per ray plus a one-byte raster attachment: about 9 MiB. This workspace is reused across Z slices and released after initialization. Geometry and aligned parameter rows add to that amount. There is no finer 3D simulation grid. Device limits and available-memory checks apply to these temporary resources.

## Mapping liquid into the voxel wall domain

Let `L_i` be the sampled liquid CAD fraction and `S_i` the sampled fraction of
nearby fixed solids. In a cell considered open by the binary wall mask, the
initial liquid fraction is `α_i=L_i/(1−S_i)`. A masked wall cell stays empty of
moving liquid. Without nearby solid intrusion, this gives the original CAD
fraction unchanged.

This maps the phase fraction of the geometrically available space into the
existing full-cell fluid domain. It prevents part of a filled reservoir wall
from appearing as an air sliver solely because liquid was integrated as a
fractional volume while the solid was represented as a whole voxel.

**This changes the approximated initial volume.** It does not conserve the
source CAD liquid volume across the geometry conversion, and it is not a
cut-cell flow solver. Subsequent conservation is measured from the initialized
voxel-domain state. Refine the walls and check initialized volume against source
geometry before making quantitative nozzle predictions. Solid face apertures
and fractional control volumes remain future work.

Included solid bodies whose bounds lie within one cell of a liquid body's
bounds use the same streamed raster integration. The spare surface component
holds their temporary fractions; no additional volume-sized allocation is
needed. Unresolved near-zero open fractions and sampled liquid/solid overlap
fail explicitly. Intersections are not silently converted into a Boolean union.

## Temperature and phase

For each contributing liquid body, the same raster fractions weight its initial material enthalpy, including latent heat. The summed enthalpy is divided by the same available fraction `1−S_i` as liquid occupancy. Thermal initialization then recovers temperature and phase from total enthalpy divided by occupied fraction. Specific temperature is preserved during the geometry projection; total initial energy corresponds to the projected voxel-domain mass. Bodies of one material can share a cell with their volume-weighted initial energy. Empty cells hold no moving-material enthalpy. Fixed solid voxels retain their own thermal state.

Initial enthalpy is the energy baseline, not deposited heat. Requested nonzero fluid velocity is projected after the initialized density/phase field is known. The current one-moving-material restriction still applies.

## Checks and measured accuracy

An imported 16,128-triangle ellipsoid has these errors. Volume error is relative to the tessellated mesh volume; fraction L1 is relative to an independently integrated analytic ellipsoid and includes mesh faceting.

| Grid | Cell-centre fraction L1 | Raster fraction L1 | Raster relative volume error |
|---|---:|---:|---:|
| 32³ | 10.77% | 0.1024% | −3.85×10⁻⁶ |
| 48³ | 7.50% | 0.1010% | −3.07×10⁻⁶ |
| 64³ | 5.66% | 0.1000% | +6.83×10⁻⁶ |

The nearly constant raster fraction error is consistent with mesh faceting dominating at these resolutions; it does not establish convergence for arbitrary imported geometry.

Regular headless checks cover a thin sheet missed by cell-centre occupancy, a hollow solid, globally reversed winding, inconsistent-face rejection, an oblique wedge, mixed initial temperatures, latent heat in partial cells, fixed-solid preservation, and unified initialization/advancement. Additional projection checks verify an analytic wall/free-surface intersection, unchanged liquid temperature, rejection of overlap missed at cell centres, and byte-identical isolated-liquid fractions. The thin-sheet check uses an 8 mm sheet in 125 mm cells as a numerical geometry test, not a resolved fluid geometry claim.

Earlier unconditioned startup measurements on an RTX 3060 Laptop GPU were 44 ms / 5.30 MiB for the bundled free-surface hotend and 268 ms / 9.42 MiB for V6. With wall projection, the V6 geometry-only control uses 9.62 MiB and changes initialized liquid volume **+0.874%**. These are startup measurements, not steady-state speedup claims.

    cargo test domain::fractions::tests -- --nocapture
    cargo test imported_drop_initialization_diagnostic -- --ignored --nocapture
    ./run.sh --run-check assets/hotend-free-surface.mhd.json 24

The diagnostic writes artifacts/cad-initialization.csv. For dynamic drop checks using imported mesh fractions, see [drop oscillation](DROP_OSCILLATION.md).
