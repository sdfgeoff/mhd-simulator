# Stationary electromagnetics

The workbench now computes applied magnetic fields and transient eddy currents in stationary, conducting CAD volumes. It reports magnetic flux density B, induced current density J, Lorentz force density J×B, and Joule heating. The liquid surface and solids remain fixed during these stages.

## Run an example

```sh
./run.sh assets/hotend-cad.mhd.json
# Or initialize and advance five steps automatically:
./run.sh --em-preview assets/hotend-cad.mhd.json 5
```

1. Use **01 / Import**, then **02 / Assign properties** to set materials, temperatures and current sources. Expand each CAD **Curve**, enable **Carry prescribed current**, and set its independent waveform and wire/core radius. Expand any CAD **Solid** to enable its own **Permanent magnet source** and set magnetization. **03 / Simulate → Simulate** builds/rebuilds the domain, synchronizes properties and runs the requested number of induction steps. Click again to continue. The example has two independently driven CAD windings, two permanent magnets, and 10 µs steps. Applied-only previews, initialization and single stepping remain under **Applied field / diagnostic stepping**.
2. Under **04 / View results → Section view**, choose Hatched CAD, CAD outlines, Voxel domain, total/applied/reaction B, induced J, Lorentz force or Joule heating. Field component, colour range and arrow controls sit here too. The choice applies to both viewports. Each viewport samples the resident 3D field buffer at its own pixel size. B is interpolated between cell samples; J/force/heating retain cell values across material interfaces. CAD section contours remain visible. The legend states the colour range and SI unit.
3. **Vector arrows on plane** uses a small GPU gather of the selected vector field. Density defaults to 25×25 samples and is adjustable from 8×8 to 64×64; arrow length scales with sample spacing to limit overlap. The arrows are drawn as geometry in each viewport. The source wire path and its return lead appear in the 3D view. Moving or rotating the plane does not rerun the physics.
4. The right panel provides a nearest-cell probe and time traces. **Force / power / conservation** contains whole-domain and per-component force/heating integrals, plus current/flux divergence diagnostics. Component totals distinguish forces in the molten metal from those in the chamber.
5. The bottom **Timeline** scrubs actual recorded 3D results. First/previous/next/latest buttons navigate retained frames; staying on the latest frame follows new results during a run. Both viewports, arrows, probes, colour range and force/power diagnostics use the selected frame. A white cursor marks its time on the probe trace. Further solver steps always continue from the latest solver state, even while an older result is selected.
6. Under **03 / Simulate → Domain resolution and bounds**, set cell spacing directly or use **2× finer / 2× coarser**, then inspect dimensions and estimated field storage. The amber 3D box previews proposed, outward-rounded bounds before rebuilding, in CAD and field views. **Simulate** applies pending changes automatically; a manual rebuild remains available. Bounds remain separately editable. Refining all three axes by two increases cell count and field memory by eight; the droplet cell-count indicator helps compare resolution.
7. Computation runs in a worker thread. CAD meshes are immutable and shared; material reassignment uses copy-on-write GPU IDs so it cannot mutate an active solve snapshot. **Cancel after current step** stops a batch between steps. Changing geometry/material/source inputs invalidates the displayed solution; initialize again. Unconverged solves are errors, not accepted frames. Accepted results before cancellation remain viewable; reinitialize to resume after a cancelled batch.

Recorded fields stay on the GPU: 64 bytes per cell per frame. Retention uses a frame-count window, default **10**, adjustable beside the timeline. Older frames are discarded and the timeline reports their count and actual memory use. Ten 60³ frames use about 132 MiB; ten 128³ frames use 1280 MiB. Domain controls estimate memory for the selected frame count. A bounded worker queue can temporarily hold two additional frames, plus the worker's current copy and solver allocations. Results and timeline selection are session-only. The generic `Timeline<T>` owns time, selection and retention independently of electromagnetics, ready for other analysis outputs. It stores display results and diagnostics, not restart checkpoints; there is no interpolated or reconstructed history.

The bundled example uses the default material references and approximations in [MATERIALS.md](MATERIALS.md). These are not calibrated operating-temperature properties. The coil envelope is nonconducting alumina; currents follow the CAD centrelines. The two magnet solids use the permanent-magnet material preset and independent magnetization vectors.

To regenerate the example project:

```sh
cargo run --example make_multi_source
```

## CAD centreline import

1. Export winding **centrelines in the main CAD assembly** as glTF/GLB `LINE_STRIP`, `LINE_LOOP`, or unbranched connected `LINES`, alongside triangle solids. Node/assembly transforms and glTF metre units are applied as for solids. All curves appear as geometry even before current is assigned. Curves remain separate CAD items; selecting them does not create conducting solid voxels.
2. Under **Assign properties**, expand any **Curve · name**, enable **Carry prescribed current**, assign its own constant/sine/pulse/table waveform in A, and set wire/core radius. Table waveforms expose editable time/current samples directly in the curve properties. Repeat for as many curves as needed. **Reverse current direction** reverses the CAD point order for that source. Unchecked curves carry no current. All enabled currents and all magnets contribute together to the 3D field and induction solve.
3. Under each **Solid · name**, enable **Permanent magnet source** and set its own world-space magnetization vector in A/m. Each magnet has independent surface refinement and smoothing settings. Enabling a magnet on an Unassigned body selects the permanent-magnet material preset; an existing material assignment is preserved. Magnetization remains a source property, separate from density/conductivity/permeability.
4. Include each physical return lead as part of its CAD curve for induction. Open curves show a warning and remain open. Disconnected `LINES` become separate curves; branched or duplicated segments are rejected as ambiguous. Ordered strips/loops retain their direction. These are sampled polylines: export curved CAD geometry with sufficient chord resolution and compare refinement. STEP remains inspection-only; STL surface triangles cannot carry a centreline curve.
5. New projects store each source's CAD curve index, direction and waveform, and guard the saved CAD curve names/coordinates. A changed/reordered export is rejected rather than silently reattaching currents. Source points come from the loaded CAD geometry. Geometry is referenced, so keep the assembly and its external buffers available.
6. Run `./run.sh` or `./run.sh assets/hotend-cad.mhd.json` for the main-CAD example: **two separately driven winding curves and two magnetized solids**. Its geometry lives in `assets/hotend-cad.gltf`; all four sources are editable through the CAD item properties. Regenerate the example with `cargo run --example make_multi_source`.

[Material defaults and sources](MATERIALS.md) describe the starting conductivity, permeability and density values, including approximations and temperature limits.

## Sources and saved settings

1. Wire sources are arbitrary 3D polylines with a positive core/smoothing radius. The vector potential uses the analytic line integral of a softened 1/R kernel. This avoids resolving wire cross-sections in the material grid, but does not model skin/proximity redistribution or coil heating. Induction requires closed paths including their return leads.
2. A wire can use its own current waveform in amperes, or a dimensionless waveform multiplied by a linked component's prescribed current. Constant, sine, finite-rise pulse and piecewise-linear table waveforms are supported. Table times are seconds; path coordinates are millimetres. The UI displays pulse times in microseconds. Advanced JSON exposes the complete source/settings schema.
3. Permanent-magnet sources assign uniform magnetization M (A/m) to a closed CAD body. Oriented surface-current patches approximate K = M×n, with adjustable subdivision and smoothing. Refine both to check near-surface accuracy. In induction mode, a magnetized body must have μr=1; other bodies can have linear permeability contrast. Sources are independent of CAD visibility and material-grid inclusion: excluding a body removes its material response, not an explicitly configured source. Nonlinear saturation, hysteresis, demagnetization evolution and magnetization-dependent mechanical force are outside this version.
4. Uniform time-dependent applied B sources are available through the JSON schema, including for verification. The applied-only mode explicitly assumes vacuum permeability and no conductive reaction. Induction uses the assigned scalar conductivity and relative permeability; unset required values are rejected. Background material 0 is explicitly vacuum (σ=0, μr=1), and incompatible values are rejected.
5. Project JSON has an optional `electromagnetics` field containing sources, timestep, tolerance and iteration limit. Older projects without this field default to an empty source configuration. Computed fields and traces are transient results, not saved into the project. Temperature values remain configuration: this solver uses the conductivity/permeability explicitly supplied, without deriving them from temperature.

## Discretization

Geometry/import/display coordinates remain millimetres. Solver coordinates are metres, time is seconds, A has units T·m, B is T, J is A/m², force density is N/m³ and heating density is W/m³. Input conversion is the existing `SimulationSnapshot` boundary.

The low-frequency model neglects displacement current. In temporal gauge, electric field is E = −∂A/∂t. Magnetic flux density is B = curl A. The stationary constitutive law is J = σE; no velocity-induced u×B term is used. The vector-potential formulation and temporal gauge are described in the [openCFS magnetic PDE documentation](https://opencfs.gitlab.io/userdocu/PDEExplanations/Singlefield/MagneticPDE/).

A is stored along oriented grid edges and B on faces. Let C be the dimensionless oriented edge-to-face incidence matrix, h the uniform cell spacing, Σ the diagonal edge conductivity, and N the face inverse relative permeability. Conductivity is the mean of the four adjacent cell values at an edge; inverse permeability is averaged across a face. All are positive-semidefinite/positive as required by the solve. C and its exact transpose are shared by all material interfaces; no fake air conductivity is introduced.

Split A = As + Ar, where As is the prescribed free-space source potential and Ar the material reaction. For a backward-Euler step, the increment δAr satisfies

```
[Cᵀ N C + μ0 Σ h²/Δt] δAr
    = − μ0 Σ h²/Δt · ΔAs
      + Cᵀ(1 − N) C As_new
      − Cᵀ N C Ar_old.
```

Then Ar_new = Ar_old + δAr. Solving the increment avoids adding many small PCG corrections directly to a larger accumulated potential. Initial conditions use a magnetostatic solve with zero induced current. Thus a constant source at t=0 starts already equilibrated; use a waveform initially at zero to model switch-on. A sine waveform may require several periods to reach periodic steady state.

The discrete current is −Σ(δAr + ΔAs)/Δt. Discrete divergence of Cᵀ is zero, which supplies current conservation in the conducting network and insulating interfaces, up to the solved residual and floating-point error. Divergence of face B is zero by the corresponding face-to-cell incidence identity. Cell fields are reconstructed for display/force integration. Joule heating uses the mean of squared edge E values, not the square of their mean. Magnetic-energy diagnostics use squared face fields with material weighting.

The matrix-free Jacobi-preconditioned conjugate-gradient solver, dot products and reductions run on the GPU. The CPU reads small convergence/status buffers between batches. A true residual is recomputed before acceptance; further iterations restart from it if necessary. The requested tolerance is relative to the initial residual of that time step. Failed solves restore the previous reaction/source state and accepted field buffer, and do not advance time. Tight tolerances can reach the f32 noise floor; failures are reported explicitly.

Source geometry remains resident; time steps update amplitudes. Direct summation is split into 32-element dispatches to bound individual kernel duration. Very large source/grid combinations are rejected above 50 billion edge–source evaluations; lower source refinement/segment count or coarsen the grid. Source acceleration remains a future performance improvement.

The kernels use at most eight storage bindings and 16 KiB workgroup storage. Field allocation, reduction scratch, source tables and a rollback checkpoint are checked against adapter binding limits and a 3 GiB combined grid/EM budget. There is no CUDA dependency or duplicate CPU numerical solver.

## Outer boundary and interpretation

1. Production runs set the tangential **reaction** potential to zero at the finite outer box. The prescribed source field is still evaluated analytically there; its normal component is not forced to zero. This is a truncated-air-domain approximation. Increase air padding and compare outputs at fixed h. Conductors touching the outer boundary are rejected.
2. Occupancy and material interfaces are still voxelized. Very thin conductors and skin layers require spatial refinement. Current conservation and a small algebraic residual do not guarantee that those features are resolved. Backward Euler is stable but first-order in time: decrease Δt and compare pulse response.
3. The reported mechanical quantity is **Lorentz J×B**. It includes the total magnetic field acting on induced conduction current. It is not a full magnetization/Maxwell-stress force on permanent magnets or ferromagnetic solids. No nozzle pressure, free-surface motion, droplet breakup or thermal evolution is predicted yet.
4. The pool will later have a free upper surface exposed to air, replenished by dipped feedstock. For stationary electromagnetics, its current CAD shape simply defines the initial conductor/air interface. The geometry does not imply a sealed liquid chamber.

## Verification

```sh
cargo test -- --nocapture
# Longer published 3D reference case (normally excluded from the fast suite):
cargo test published_team6_hollow_sphere -- --ignored --nocapture
# Additional refinement:
MHD_TEAM6_GRIDS=128 cargo test published_team6_hollow_sphere -- --ignored --nocapture
# Reproducible device run and CSV output:
./run.sh --em-check assets/hotend-cad.mhd.json 20
```

The CLI writes `artifacts/em-probe.csv` at [1,0,4] mm and `artifacts/em-body.csv` for component integrals. The benchmark writes `artifacts/team6.csv`. These are runtime artifacts, ignored by git.

1. Fast GPU checks cover a loop against its analytic field, constant-B/zero-eddy-current equilibrium, a magnetized prism against its axial analytic field, a linear-permeability sphere, pulse response/current conservation, an exact discrete diffusion eigenmode and timestep refinement, failed-step/nonfinite-field rollback and stale-input rejection. Separate tests check source/project round-trips, worker completion/cancellation and matching native-resolution field pixels in both views.
2. TEAM 6 uses the published conducting spherical shell: radii 50/55 mm, σ=5×10⁸ S/m, μr=1, uniform 1 T at 50 Hz. The analytic radial solution is evaluated using scaled spherical Hankel functions. This is an analytic test reference, not a CPU field solver. See [TEAM 6 specification and analytic solution](https://abaqus.uclouvain.be/English/SIMACAEBMKRefMap/simabmk-c-team6spherical.htm).
3. The reference run supplies analytic initial and outer reaction-boundary data to isolate the discretization and time integration from box truncation and long startup transients. Production runs use the zero-reaction boundary described above. The full 3D grid box is ±72 mm; there are 96 steps per 20 ms period, and the second period is used to extract complex fields/current and cycle means. Shell probes are compared at their actual nearest-cell centres.

Measured on an NVIDIA RTX 3060 Laptop GPU:

| Grid | Central complex B error | Local complex Jy error | Local mean heating error | Local mean Lorentz Fx error |
| --- | ---: | ---: | ---: | ---: |
| 64³ (2.25 mm cells) | 3.76% | 38.69% | 81.72% | 25.96% |
| 96³ (1.50 mm cells) | 3.19% | 13.35% | 12.55% | 23.25% |
| 128³ (1.125 mm cells) | 3.59% | 9.34% | 12.98% | 15.00% |

These results demonstrate the coarse-wall error and improvement in local current with refinement; they do not establish high-accuracy force prediction. Central-field and local heating errors do not decrease monotonically at fixed first-order timestep. Further spatial/time refinement and interface treatment are needed when tighter accuracy is required. The benchmark does not validate dynamic TEAM 28 motion.

The current 60×40×40 multi-source example took approximately 0.12–0.17 s per step in a three-step headless run on the same GPU. The 128³ reference took approximately 0.9 s per step. Background rendering or other GPU work can increase these times. Field-memory estimates are shown in the UI.
