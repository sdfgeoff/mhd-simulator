# MHD Workbench

A native Rust/wgpu workbench for preparing a 3D molten-metal jetting simulation. It imports CAD-exported meshes, builds a persistent **rasterized 3D material grid**, and lets you compare its voxel sections against the original geometry with a six-axis plane gizmo.

**Electromagnetic, thermal and fluid solvers are not implemented yet.** Materials, initial temperatures and prescribed currents are project configuration; the displayed colours are geometry/material IDs.

## Run and inspect

```sh
./run.sh
./run.sh path/to/assembly.glb
./run.sh path/to/project.mhd.json
```

1. Load or drop a `.gltf`, `.glb` or `.stl`. STEP/STP currently provides metadata and import diagnostics only. glTF metres are converted to displayed millimetres; STL is interpreted as millimetres. Static triangle meshes and assembly placements are supported.
2. Under **Simulation properties**, assign each component a material, optional initial temperature in °C, and optional prescribed current in A. **Show CAD** only affects display. **Include in domain** controls voxelization. Most named materials initially have unset conductivity, permeability and density; enter values in **Material library**. The 316 stainless preset has documented 20 °C density/conductivity defaults (see below).
3. Under **Simulation domain**, expand **Bounds and cell spacing**, set the physical box and uniform cell size, then click **Build / rebuild domain**. Bounds round outward to whole cells; the inspector reports the actual built bounds. Changes to domain/material settings leave the last built grid visible with an explicit stale-grid notice until rebuilt.
4. Select **Voxel domain** for actual cube/plane intersection faces, with optional **Subtle cell edges**. The edges are antialiased geometry on cell boundaries, not dark bands in a label image; fine grids fade out and mipmaps filter the 3D view. **Colour by material ID** switches component colours to material IDs. Hover the section to inspect voxel IDs. Select **CAD**, then **Hatched CAD** or **Outlines**, for geometric sections. Both views use the same rendered image and style; the flat panel is not permanently locked to voxels.
5. Drag the plane arrows to translate along world X/Y/Z, or the rings to rotate about those axes at the pivot. Drag the background or right-drag to orbit; scroll to zoom. Numeric transforms, reset and cut-side flip are available. Moving the plane samples the resident volume; it does not rebuild the domain.
6. Set a filename ending in `.mhd.json` and use **Save project** / **Open project**. The project stores domain settings, material library and assignments, inclusion/visibility, temperatures, currents and plane pose. Geometry is referenced, not embedded; keep the source mesh and its external glTF buffers available. Paths within the project directory are saved relatively. Component name/count checks catch incompatible re-exports.

The included original `assets/hotend.gltf`/`.glb` fixture has a 6 mm OD / 3 mm ID chamber with a tapered nozzle, a separate metal volume, a coil envelope and two offset magnet components. It is illustrative geometry, not a validated design. The coil envelope currently has no electrical winding model.

## Domain construction

1. For each included closed component, hardware rasterization projects all surface crossings onto XY cell-centre columns. A stencil parity operation at each Z cell-centre layer identifies the solid interior, including cavities. A fullscreen stencil-tested pass writes the layer occupancy framebuffer. This uses every crossing rather than nearest-depth rendering.
2. Compute passes merge components into GPU-resident body masks and material IDs. ID **0** is air/void; **255** marks overlap. Overlapping components are reported even when they share a material. A separate GPU property table stores material scalars plus flags identifying unset values.
3. The persistent grid uses two `u32` values per cell, plus the small property table: about **128 MiB at 256³**. Build scratch includes an occupancy texture array, stencil target, parameters and mesh uploads. Future physical fields will require additional memory. Allocation limits and estimated build memory are checked before building.
4. Voxel ID inspection uses constant-time grid lookups. For visualization, a GPU pass enumerates a thin slab of cells intersecting the plane, visiting at most four candidate cells per projected grid column. Only those cell records return to the CPU for exact cube/plane polygons; the full volume stays on the GPU. Mesh/plane CAD contours are computed separately. A shared 4× MSAA section renderer supplies both the flat panel and the 3D cut face, with mipmaps for minification.

## Diagnostics and limits

1. The inspector reports grid dimensions, cell size, actual bounds, persistent GPU memory, build time/serial, occupied/overlapping cells, and per-component occupied volumes.
2. Warnings identify components with no occupied cell centres, large mesh/voxel volume discrepancies, small axis-aligned extents, clipped geometry, and surface patches without nearby occupied cells. These are resolution diagnostics, not a guarantee that every local thin feature survives. Compare CAD contours and refine the grid when needed.
3. Occupancy is sampled at cell centres. Geometry below the cell spacing can disappear. Physical boundary fractions, face apertures, signed distances and adaptive refinement are not present; those will be needed for more accurate solver boundary treatment.
4. Mesh closure/nonmanifold checks and nonfinite/degenerate-triangle rejection catch common import errors, but do not prove geometric validity or absence of self-intersections. Included open meshes are rejected. Ambiguous CAD section loops fall back to outlines with a warning.
5. The current grid supports up to 31 components. Raster target layers, buffer sizes and memory are checked against the GPU limits. The UI's memory estimate covers grid working storage; uploaded mesh vertices add memory.
6. Temperature and current values are saved but do not generate fields yet. A current value alone does not define a winding path/direction or waveform. Material properties are scalar inputs, not temperature-dependent property curves. No CUDA or C/C++ CAD kernel is used; wgpu/eframe supply platform graphics/window integration.

## Material presets

The 316 / 1.4401 preset uses density **8,000 kg/m³** and conductivity **1.333×10⁶ S/m** (converted from resistivity 0.75 Ω·mm²/m), referenced at **20 °C** in the [manufacturer datasheet](https://www.thyssenkrupp-materials.co.uk/stainless-steel-316-14401.html). Relative permeability stays unset because material condition matters. Override scalars for the operating temperature. The reference note/link is visible in the material editor. Older projects gain the presets without replacing custom material IDs.

**Alumina** is available with its grade/purity unspecified. Its numeric properties stay unset until supplier-specific values are chosen.

## Verify

```sh
cargo test -- --nocapture
cargo fmt --check
./run.sh --inspect assets/hotend.gltf
./run.sh --inspect vendor/step-loupe/sample/nist-ctc05.step
./run.sh --gpu-check assets/hotend.gltf
./run.sh --gpu-check assets/hotend.gltf 0.1
```

The last argument to `--gpu-check` optionally sets cell spacing in mm. It reports a real volume build and writes `artifacts/section.ppm`. GPU tests fail explicitly if no working adapter is available.

Tests cover analytic hollow-cylinder volume convergence, rotated solids, a 256³ build, overlap detection, visibility/inclusion independence, lost subcell components and patches, open-mesh rejection, slicing without rebuilds, material labels, project round-trips and startup, all six gizmo drags, hole-aware CAD caps, GPU-rendered hatching/outline readback, exact cube-plane footprint geometry, complete oblique-cell enumeration, and shared-image style changes. GPU tests serialize context lifetime/teardown on the shared adapter to avoid concurrent driver cleanup contention. There is no duplicate CPU physics solver.

## Source and next milestones

`vendor/step-io` is the upstream pure-Rust STEP reader at commit `2d7bd2a9d55fe9a4c8b12e3a8f39bcafe2575300` (0.3.0). `vendor/step-loupe` is an upstream reference checkout, not linked into the app. Their licences/notices are retained; see `THIRD_PARTY.md`.

1. Prescribed-current wire paths and validated magnetic-field visualization.
2. Transient induction in metal and conductive chamber walls.
3. Accurate fluid boundary treatment, capillary dynamics, then coupled jetting.
4. Robust pure-Rust STEP face tessellation.

Regenerate the original mesh fixtures with `python scripts/make_fixtures.py`; Python is only used for development fixture generation.
