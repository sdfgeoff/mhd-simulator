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
3. Read the resulting `Jⁿ⁺¹ × Bⁿ⁺¹` directly from the shared EM output buffer, refresh its force/CFL bound, then advance velocity and pressure.
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

## Memory availability

Fixed 2/3 GiB caps have been removed. On Linux/Vulkan, allocation preflights query the selected device's current **VK_EXT_memory_budget** and usage, comparing **additional** allocations with currently available device-local memory. Existing geometry, solver state and resident display/transfer allocations already count in the driver's usage. The checks run before a domain build, solver allocation and the selected-frame GPU cache. This follows the driver's live [memory-budget estimate](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceMemoryBudgetPropertiesEXT.html), not the GPU model's advertised VRAM alone. Budgets can change as other applications allocate memory.

Hardware buffer/binding/texture limits remain mandatory. Backends without a live budget query enforce those device limits without substituting an arbitrary global cap. The current live query is implemented for Linux/Vulkan, with hardware and software Vulkan both supported. The CLI reports the adapter name so a software fallback is visible.

Working solver memory and recorded output are separate. The workbench keeps full-precision immutable frames in **RAM**, with one selected frame uploaded for GPU rendering. EM, fluid and thermal outputs each use 64 bytes per cell, so a complete three-physics frame is **192 bytes per cell**. The default ten-frame window limits RAM history by frame count. Scrubbing replaces the GPU cache; probes can read RAM without a GPU transfer. Repeated frames share immutable RAM bytes between the history and its selected view.

Downloads copy directly from live solver buffers into reusable mapped staging buffers, without an intermediate GPU snapshot. Up to two captures are in flight, followed by a bounded two-frame delivery channel; an active sender can hold one additional completed frame. Transfer buffers are retained only for the worker's run, then released. Their memory location depends on the backend; they are counted separately from solver buffers and history. Earlier accepted captures are delivered even if a later tick fails. Display-cache allocation failure leaves RAM history intact and reports a view error.

**Record every N steps** changes output frequency without changing the physical timestep. Step zero, interval ticks and the final accepted tick are retained (subject to the frame window). Stop also records the final accepted tick. Continuation advances the latest solver state, independently of the selected history frame.

Coupled fluids bind the live EM packet directly; the isolated frozen-force API still owns its snapshot. Interface enthalpy transport writes into the thermal solver's spare ping-pong buffer and switches the current index after successful interface transport. These remove two 64-byte-per-cell working allocations without changing the numerical kernels or rollback checkpoints.

## Profiling

Expand **Performance and memory** in Simulation for logical working allocations by physics, history RAM, selected-frame GPU storage, delivery-queue RAM, reusable transfer buffers and in-flight downloads. These are application buffer sizes, not a substitute for the driver's total memory usage; render resources, allocator overhead and deferred frees can add to it. Readback completion latency includes queued GPU work and polling delay; the separate RAM-copy time measures copying the mapped result.

**Profile solver stages** records exclusive CPU wall times and optional compute-pass GPU timestamps. CPU wall time includes submission and waiting. GPU times sum compute passes only, excluding transfers and idle gaps. GPU results arrive asynchronously and identify their step separately from the latest CPU step. Devices without timestamp support still show CPU measurements. Up to 2,048 passes per tick are timestamped; the displayed timed/total count makes partial coverage visible. Profiling can be disabled without changing the solver.

`--run-check` writes `artifacts/run-performance.csv` alongside `artifacts/run-diagnostics.csv`. Rows include step/time, accepted status, stage CPU/GPU milliseconds and timestamp coverage. On a failed tick, it still writes timings and earlier accepted diagnostic rows before returning an error. The CLI waits for outstanding timestamp readback only when finishing the check.

### Measured V6 comparison

A headless check of the local V6 hotend at 0.2 mm spacing (172×112×124 cells), Δt=2.5 µs and two accepted ticks reduced logical working storage from **2,007.0 to 1,715.4 MiB**, saving **291.6 MiB** (14.5%). A complete frame is 437.4 MiB: ten frames now require about 4.27 GiB of RAM plus a 437.4 MiB selected-frame GPU cache, with transfer buffers accounted separately. The original CAD/project files were unchanged.

The complete diagnostic CSV was identical before and after this change, including iteration counts, fields' extrema and energy/volume balances. Combined CPU step time was 12.334 s before and 12.347 s after; combined GPU compute time was 11.325 s before and 11.333 s after. This single short comparison shows no meaningful solve-time improvement. EM and pressure iterations dominate; caching source fields and improving preconditioners remain separate future work. The demonstrated benefit here is reduced working memory and GPU history growth.
