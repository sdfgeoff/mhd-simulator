# MHD Workbench

A native Rust/wgpu workbench for preparing a 3D molten-metal jetting simulation. It imports CAD-exported meshes, builds a persistent **rasterized 3D material grid**, and lets you compare its voxel sections against the original geometry with a six-axis plane gizmo.

**Applied magnetic fields and transient induction are implemented.** Inspect B, induced J, Lorentz force and Joule heating on the movable section plane. Fixed-geometry fluid velocity and pressure are also implemented, with shared animated vector overlays and two-way EM/velocity coupling on one clock. Thermal conduction, Joule heating, latent heat and phase-driven fluid mobility are implemented; moving liquid/air surfaces are the next stage. [Thermal guide](docs/THERMAL.md). [Setup and unified-run workflow](docs/RUNS.md). [Fluid guide and validation](docs/FLUIDS.md). [Electromagnetics guide, equations and validation](docs/ELECTROMAGNETICS.md).

## Run and inspect

```sh
./run.sh
./run.sh path/to/assembly.glb
./run.sh path/to/project.mhd.json
./run.sh --em-preview assets/hotend-cad.mhd.json 5
```

1. Load or drop a `.gltf`, `.glb` or `.stl`. STEP/STP currently provides metadata and import diagnostics only. glTF metres are converted to displayed millimetres; STL is interpreted as millimetres. Static triangle meshes and assembly placements are supported.
2. In **Setup → Assign properties**, assign each component a material, optional initial temperature in °C. **Show CAD** only affects display. **Include in domain** controls voxelization. Named materials include editable conductivity, permeability and density defaults; **Unassigned** remains a placeholder. References and reference conditions appear in **Material library**. Objects reference shared materials; select **Edit assigned material** to edit the entity used by all its assigned objects. Create custom materials in the same editor.
3. In **Setup → Domain resolution and bounds**, set **Uniform cell spacing** (or use **2× finer / 2× coarser**), expand **Domain bounds (mm)** to set the physical box, then switch to **Simulation** and click **Simulate** to rebuild and run. Bounds round outward to whole cells; the inspector reports the actual built bounds. Changes to geometry inclusion, bounds or cell spacing leave the last built grid visible with an explicit stale-grid notice until rebuilt. Material assignments/scalars and temperature edits update their own GPU data automatically, without voxelization, while geometry is current. Invalid edits show an error and keep the previous valid input tables.
4. Select **Voxel domain** for sections sampled directly from the resident 3D grid, with optional **Subtle cell edges**. **Colour by material ID** switches component colours to material IDs. Hover the section to inspect voxel IDs. Under **04 / View results**, the same **Section view** selector offers **Hatched CAD**, **CAD outlines** and electromagnetic quantities, with adjacent field/component/range/vector controls. Both views use the same renderer and shader, drawing fills, hatching and world-space voxel edges at their own viewport pixel size. There is no intermediate section image or fill-resolution control.
5. **Vector display** offers GPU-animated trails/dots, arrows or Off. Trails follow the on-plane projected field; their visual speed and pause controls are independent of solver time. Both viewports share the generated geometry.
6. Drag the plane arrows to translate along world X/Y/Z, or the rings to rotate about those axes at the pivot. Drag the background or right-drag to orbit; scroll to zoom. Numeric transforms, reset and cut-side flip are available. Moving the plane samples the resident volume; it does not rebuild the domain.
7. The bottom **Timeline** navigates recorded field results in both viewports, probes and diagnostics. New steps advance from the latest solver time; a configurable frame window (default 10) bounds the number of retained results; memory estimates scale with the domain.
8. Include winding centrelines as glTF/GLB line strips, loops or connected lines in the **main CAD assembly**. Under each **Curve** in Assign properties, enable **Carry prescribed current**, choose its independent waveform and wire/core radius. Under each **Solid**, enable **Permanent magnet source** and set its magnetization vector independently. There is no one-wire/one-magnet UI limit. Induction requires each current path to include its return lead. The default `assets/hotend-cad.mhd.json` demonstrates two CAD windings and two magnets. STEP remains inspection-only.
9. Set a filename ending in `.mhd.json` and use **Save project** / **Open project**. The project stores domain settings, material library and assignments, inclusion/visibility, temperatures, currents and plane pose. Geometry is referenced, not embedded; keep the source mesh and its external glTF buffers available. Paths within the project directory are saved relatively. Component name/count checks catch incompatible re-exports.

The included original `assets/hotend.gltf`/`.glb` fixture has a 6 mm OD / 3 mm ID chamber with a tapered nozzle, a separate metal volume, a coil envelope and two offset magnet components. It is illustrative geometry, not a validated design. The coil envelope is visual geometry. The separate EM example supplies a closed, prescribed-current wire path and illustrative material inputs.

## Domain construction

1. For each included closed component, hardware rasterization projects all surface crossings onto XY cell-centre columns. A stencil parity operation at each Z cell-centre layer identifies the solid interior, including cavities. A fullscreen stencil-tested pass writes the layer occupancy framebuffer. This uses every crossing rather than nearest-depth rendering.
2. Compute passes merge components into GPU-resident body masks and material IDs. ID **0** is air/void; **255** marks overlap. Overlapping components are reported even when they share a material. A separate GPU property table stores material scalars plus flags identifying unset values.
3. The persistent grid uses two `u32` values per cell, plus the small material/component input tables: about **128 MiB at 256³**. Build scratch includes an occupancy texture array, stencil target, parameters and mesh uploads. Future physical fields will require additional memory. Allocation limits and estimated build memory are checked before building.
4. Each voxel fragment performs a constant-time lookup in the resident 3D material/body storage buffers; the buffers are indexed as a uniform 3D grid. Domain building and rendering share one wgpu device. CAD mesh/plane intersections produce hole-aware cap triangles and contours, rendered directly into each viewport with procedural hatching. Both views use 4× MSAA and the same WGSL shading, including screen-space derivatives for voxel edge width and fading. Moving the plane does not read back cell slabs or rasterize a reusable section image. Only a single label is read back on hover for the inspector.

## Simulation core and units

1. The library (`src/lib.rs`) exposes `gpu`, `simulation`, `inputs` and `domain` independently of the app/window. `GpuContext::headless()` and `GpuDomain::from_context()` use the same device limits as the native app. `project` handles persistence and retains compatible JSON names/units.
2. `SimulationSnapshot::from_model()` is the explicit solver boundary: CAD/domain distances in mm become metres in `GridSi`; initial °C become kelvin. Conductivity is S/m, density kg/m³, permeability is relative; source waveform currents use A. Unknown values remain `None`; GPU tables carry presence flags. `GpuInputs::solver_tables(required, include_air)` rejects missing required properties, including air when requested; explicitly specified zero conductivity is distinct from unknown.
3. `FieldLocation` describes cell-centre, axis-normal face and axis-parallel edge samples. `GridSi::sample_dims()` and `position_m()` give each layout's dimensions and locations in metres. Flat grid indexing is X-fastest: `x + nx * (y + ny * z)`. Electromagnetics uses edge A, face B and cell-centred display/force samples; future fluid layouts can use the same descriptors.
4. Built body masks stay unchanged during input updates. Material reassignment runs a separate compute pass over those masks and replaces the ID buffer, keeping active solver snapshots unchanged. Material and initial-condition revisions are separate from geometry identity; future `FieldState` buffers live separately and record the input versions they were computed from. Pending geometry edits or input updates invalidate results. Invalid updates cannot replace the last valid tables. Geometry/grid/inclusion changes are rejected by the input-update API until a rebuild.
5. CAD triangles are immutable after import; replacing a body gives it a new geometry identity. Each viewport caches its CAD surface vertex buffers by geometry identity and winding. Camera/plane motion, visibility and simulation-property edits reuse those buffers. Section caps/contours remain dependent on the plane.

## Diagnostics and limits

1. The inspector reports grid dimensions, cell size, actual bounds, persistent GPU memory, build time/serial, occupied/overlapping cells, and per-component occupied volumes.
2. Warnings identify components with no occupied cell centres, large mesh/voxel volume discrepancies, small axis-aligned extents, clipped geometry, and surface patches without nearby occupied cells. These are resolution diagnostics, not a guarantee that every local thin feature survives. Compare CAD contours and refine the grid when needed.
3. Occupancy is sampled at cell centres. Geometry below the cell spacing can disappear. Physical boundary fractions, face apertures, signed distances and adaptive refinement are not present; those will be needed for more accurate solver boundary treatment.
4. Mesh closure/nonmanifold checks and nonfinite/degenerate-triangle rejection catch common import errors, but do not prove geometric validity or absence of self-intersections. Included open meshes are rejected. Ambiguous CAD section loops fall back to outlines with a warning.
5. The current grid supports up to 31 components. Raster target layers, buffer sizes and memory are checked against the GPU limits. The UI's memory estimate covers grid working storage; uploaded mesh vertices add memory.
6. Each current waveform is assigned directly to a CAD centreline. Material properties are scalar inputs, not temperature-dependent property curves. Temperature remains configuration; the EM solve uses explicitly supplied conductivity/permeability. No CUDA or C/C++ CAD kernel is used; wgpu/eframe supply platform graphics/window integration.

## Material presets

The solder presets also include an illustrative liquid viscosity of 0.002 Pa·s for starting fluid experiments; replace it for the operating temperature.

The shared library includes aluminium 6061-T6. All named presets have conductivity, relative permeability and density values. **Unassigned** stays unset so forgotten material assignments are reported. [Material defaults, assumptions and sources](docs/MATERIALS.md) records the values: most are room-temperature/bulk references, with explicit unit-permeability and insulating approximations. Temperature inputs do not automatically modify these scalars. Existing project values are preserved; missing built-in library entries are added automatically. Thermal/phase properties can be stored and edited, but temperature does not yet determine the simulated fluid region.

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

Automated tests run headlessly, including GPU rendering readbacks and synthetic egui pointer interaction; they do not open or focus desktop windows.

Tests cover analytic hollow-cylinder volume convergence, rotated solids, a 256³ build, overlap detection, visibility/inclusion independence, lost subcell components and patches, open-mesh rejection, slicing without rebuilds, material labels, project round-trips and startup, all six gizmo drags, hole-aware CAD caps, GPU-rendered hatching/outline readback, direct voxel fragments versus GPU grid labels at 256px and 512px, material colours and analytic grid edges, matching CAD face pixels in the 2D and 3D renderers, SI conversions and staggered sample positions, required-property validation, GPU table/assignment updates without voxelization, stale-result invalidation, reuse of CAD surface buffers, analytic magnetic-field and diffusion cases, current conservation, source persistence, field rendering and worker cancellation. The longer TEAM 6 verification runs explicitly; see the electromagnetics guide. GPU tests serialize context lifetime/teardown on the shared adapter. There is no duplicate CPU physics solver.

## Source and next milestones

`vendor/step-io` is the upstream pure-Rust STEP reader at commit `2d7bd2a9d55fe9a4c8b12e3a8f39bcafe2575300` (0.3.0). `vendor/step-loupe` is an upstream reference checkout, not linked into the app. Their licences/notices are retained; see `THIRD_PARTY.md`.

1. Moving liquid–air interfaces, balanced surface tension and improved fractional wall treatment.
2. Time-dependent coupling to the moving liquid, then thermal evolution as required.
3. Tighter spatial/time convergence and faster EM preconditioning for larger grids.
4. Robust pure-Rust STEP face tessellation.

Regenerate the original mesh fixtures with `python scripts/make_fixtures.py`; Python is only used for development fixture generation.
