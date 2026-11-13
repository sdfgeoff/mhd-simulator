# MHD Workbench — geometry prototype

A native Rust/wgpu workbench for exploring the geometry of a molten-metal jetting tool. This first milestone imports meshes, displays an orbitable clipped model, and computes solid occupancy on an arbitrary cut plane using a WGSL compute shader.

**There is no electromagnetic or fluid solver yet. The cross-section colours identify bodies, not field strengths.**

## Try it

From this directory:

```sh
cargo run -- assets/hotend.gltf
```

1. Drag in a `.gltf`, `.glb` or `.stl`, or enter its path and click **Load file**.
2. Drag the central viewport to orbit; scroll to zoom.
3. Use **YZ / XZ / XY**, then adjust the plane angles and offset. The right panel shows GPU-classified occupancy at pixel centres.
4. Toggle individual bodies and assign their intended roles. These role labels are session-only in this prototype.

The included original hotend fixture has a 6 mm OD / 3 mm ID chamber with a tapered nozzle, a separate metal volume, a coil envelope and two offset magnet bodies. It is illustrative geometry, not a proposed validated design. The glTF contains positions in metres; the workbench displays millimetres.

STL has no declared units: this preview interprets it as millimetres and displays the dimensions for checking. glTF scene hierarchy and transforms are applied. Only static triangle meshes are accepted; no skinning or morph targets.

STEP/STP currently loads **metadata and an import report only** through pure-Rust `step-io`; B-rep tessellation is still outstanding. Export glTF/GLB or STL to view solids. STEP inspection never creates substitute geometry.

## Verify without a window

```sh
cargo test -- --nocapture
cargo run -- --inspect assets/hotend.gltf
cargo run -- --inspect vendor/step-loupe/sample/nist-ctc05.step
cargo run -- --gpu-check assets/hotend.gltf
```

The final command writes `artifacts/section.ppm`. Tests require a working wgpu compute adapter; missing GPU support fails explicitly. GPU tests compare an imported annulus against its analytic cross-section at three resolutions and an oblique plane, check cavity preservation and area convergence, and reject an open surface. There is no duplicate CPU solver.

## Scope and limits

1. Geometry preparation is on the CPU; section classification uses wgpu compute; the egui viewport uses the wgpu renderer. The prototype creates a separate compute device and reads back a small label image on plane changes. A full solver should keep fields resident and share rendering resources.
2. Section classification uses ray parity per visible body, up to 31 bodies. Pink pixels report overlapping bodies. It samples a plane directly; it does not yet allocate a complete 3D simulation grid.
3. Positional edge welding and edge-incidence counts detect open and nonmanifold meshes, and nonfinite/degenerate triangles are rejected. These checks do not prove freedom from self-intersections or all geometric defects. Near-edge ray ambiguity remains a numerical limitation.
4. The 3D preview uses clipped triangles sorted by depth, with open cut faces. The section panel supplies solid interiors. Complex intersecting geometry may show painter-order artefacts; a depth-buffered solid viewport is planned.
5. No physical materials, current waveform, magnetization, magnetic field, time stepping or droplet dynamics are implemented. Role assignments do not affect classification.
6. No CUDA and no C/C++ CAD kernel. Platform graphics/window dependencies still come through wgpu/eframe.

## Source and next milestones

`vendor/step-io` is the upstream pure-Rust STEP reader at commit `2d7bd2a9d55fe9a4c8b12e3a8f39bcafe2575300` (0.3.0); its MIT/Apache licences are retained. `vendor/step-loupe` is an upstream reference checkout (also MIT/Apache), used to inspect the example viewer and provide its NIST sample for import checks; it is not linked into the app.

1. Validate the import/viewer workflow on the user's exported assembly; add saved scene roles/settings.
2. Evaluate robust pure-Rust STEP face tessellation, preserving cavities, body identities, units and placements.
3. Uniform-grid electromagnetic fields and force visualization with GPU analytic benchmarks.
4. Capillary free-surface dynamics, then coupled metal jetting with convergence checks.

Regenerate the original test meshes with `python scripts/make_fixtures.py` (development fixture generation only; Python is not required to run the app).
