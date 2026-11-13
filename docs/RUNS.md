# Setup and unified simulation

1. In **Setup**, click **Load file…** to browse folders and open glTF/GLB/STL geometry or an `.mhd.json` project. Drag/drop remains available. An edge-only Blender coil must be exported with **Loose Edges** enabled; an empty exported object is reported in Import notes.
2. Click a solid or centreline in the 3D view, or select it in the object list. Picking uses the nearest visible surface, respects the clipping plane and section caps, and prefers a nearby visible centreline within seven screen pixels. Hidden solids are skipped. Dragging still orbits the camera; move/rotate handles take priority. The selection has a yellow bounding box or highlighted centreline.
3. Assign a material to each solid. Objects hold a material ID referencing one project material entity. **Edit assigned material** selects that same entity in the material editor; editing it changes all its users. **Create material** makes a new entity. Built-ins have defaults immediately, including **Aluminium 6061 (T6)**. Opening an older project adds missing library entries without replacing its saved material values or IDs.
4. Set initial temperatures and current/magnet sources on selected objects. A material can store solidus, liquidus, heat capacity, thermal conductivity and latent heat. Enable Thermal / phase evolution in Simulation to evolve temperature and let phase control flow; see [thermal workflow and validation](THERMAL.md). Objects may also prescribe temperature or volumetric heat.
5. Set domain resolution/bounds and inspect the voxel preview. Geometry masks are cached. Changing assigned material or scalar properties updates property/ID buffers without rasterizing geometry again. Geometry, inclusion or domain changes require rebuilding.
6. Switch to **Simulation**, choose one timestep and step count, and click **Simulate**. EM participates when sources are configured; fluid motion can run without any electrical source. Enable **Thermal / phase evolution** for conduction, Joule heat and melting/freezing; Fluid motion then selects its material from phase data. Without thermal evolution, a manual fixed-region diagnostic is available. Enable **Moving liquid / air interface** for conservative material motion and capillarity; see [the surface guide](SURFACES.md). Thermal-only runs are supported.
7. Choose any available EM, fluid or thermal result in **Section view**. Both viewports and probes use the same selected physical time. One bottom timeline retains complete output frames, default ten. Changing the viewed quantity does not switch solvers. Repeating Simulate continues the latest accepted state even when an older frame is selected. Changed inputs restart the run; **Stop after step** keeps the last complete tick.

## Coupling

For each common step from `tⁿ` to `tⁿ⁺¹`:

1. Project the lagged `uⁿ × Bⁿ` from fluid cells to oriented EM edges, weighted by the surrounding conductivities. Non-fluid cells contribute zero velocity. The existing edge mass coefficient uses the same conductivity averaging.
2. Advance the implicit induction solve with the current waveforms evaluated at `tⁿ⁺¹`. Its constitutive law becomes `J = σ (−∂A/∂t + uⁿ×Bⁿ)`. The motional term enters both the linear-system right-hand side and the current/heating evaluation; it is not merely added to a displayed force. The same discrete curl/adjoint system enforces current continuity.
3. Copy the resulting `Jⁿ⁺¹ × Bⁿ⁺¹` into the fluid solver on the GPU, refresh its force/CFL bound, then advance velocity and pressure.
4. If moving interfaces are enabled, transport material occupancy and mixture enthalpy with the projected velocity, updating the private EM material grid. Advance thermal conduction/Joule/boundary heat, then update phase and fluid mobility.
5. Record one frame only after all participating solves succeed. A failed tick stops and invalidates its solver state; it does not publish a partial new time. Earlier complete frames remain available. Subsequent Simulate starts a fresh run.

This is first-order partitioned coupling with lagged velocity/field feedback, not a nonlinear converged monolithic solve. Refine the common timestep when interpreting results. Conservative explicit limits check `Δt v_A/h ≤ 0.5` and `Δt σ B²/ρ ≤ 0.5` using maximum total B and the selected liquid properties, in addition to the fluid viscous/velocity limits. The Alfvén estimate uses μ₀. The optional moving interface transports conductivity and heat with the material. Scalar bulk material coefficients remain constant; phase controls fluid mobility.

Low-level stationary EM and frozen-force fluid APIs/CLI checks remain useful isolated numerical diagnostics. The workbench's normal Simulate action always uses the common run; it does not require selecting an EM snapshot for fluid forcing.

See [energy accounting and analytic coupling benchmarks](ENERGY.md) for the measured work/heat/loss terms and their limitations.

## Headless verification

```sh
cargo test
./run.sh --run-check assets/hotend-cad.mhd.json 10
```

`--run-check` uses the same `RunState` as the workbench, prints the enabled processes and synchronized diagnostics, and writes `artifacts/run-diagnostics.csv`. It never opens a window.

Tests cover moving-conductor feedback, zero-velocity equivalence with stationary EM, relative discrete current divergence, common-time frames, continuation/scrubbing, source-free fluid runs, failed coupled ticks, timestep refinement, clipped/cavity-aware picking, project persistence and fully offscreen Setup/Simulation/file-picker rendering. No desktop window is opened or focused.
