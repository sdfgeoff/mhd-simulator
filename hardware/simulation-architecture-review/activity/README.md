# Detailed timestep activity diagrams

Source-traced activity diagrams for the stopped Concept1 V5 direct-contact client, prepared for board topic25 reply1056. No solver changes, simulations or profiling were run to produce these figures.

1. [Full timestep](01-timestep-activity.png) ([scalable SVG](01-timestep-activity.svg)): CPU pulse evaluation; two electrical bases; force refresh; advection, viscosity and forcing; pressure; interface/momentum transport; acceptance and output.
2. [Electrical basis solve](02-electrical-basis-loop.png) ([SVG](02-electrical-basis-loop.svg)): predictor, conductivity update, PCG kernels, residual/current gates, basis storage and final field combination.
3. [Pressure projection](03-pressure-projection-loop.png) ([SVG](03-pressure-projection-loop.svg)): coefficients, warm start, PCG, optional multigrid V-cycle, physical-equation checks and velocity correction. Used at both projection points.
4. [Interface, copies and export](04-interface-copies-and-export.png) ([SVG](04-interface-copies-and-export.svg)): rollback buffers, three VOF/momentum sweeps, conditional topology-mask roundtrip, second projection and chunked disk output.

## Reading the sheets

1. Read numbered rows downward. Dashed return arrows and row references show loops. Conditional branches and early exits skip subsequent rows as labelled; these are activity maps rather than a complete exception/control-flow graph.
2. Blue arrows are host-to-device uploads; orange arrows are device-to-host readbacks. Bidirectional arrows label mixed transfers. Green links indicate resident GPU reads/writes, not copies to a separate device. Explicit GPU-to-GPU copies are marked **D2D**. Purple export arrows originate at the CPU: there is no GPU-direct filesystem path.
3. **N** is the number of dense cell slots; **F = (nx+1)(ny+1)(nz+1)** is the allocated face-grid slot count. `64N`, `16F`, etc. are bytes. Masked slots still occupy storage. **R/W** means read/write, **PCG** preconditioned conjugate gradient, **MG** multigrid, **VOF** volume of fluid, and **MAC** staggered face velocity.
4. Bulk GPU numerical data is **f32**; masks/IDs are **u32**. Electrical potential uses compensated high/low f32 words. CPU clocks and energy ledgers use **f64**. The diagrams do not imply bulk GPU f64 or f16 storage.
5. Transfer labels describe payload sizes or allocation extents, **not measured memory-bus traffic or time**. GPU kernels can reread neighbouring values many times. Conversely, a fence waits for work without downloading a field. CPU readbacks use staging copies and map/wait; the iterative checks download only small scalar packets.
6. Dense physical state remains on the GPU between steps. Buffers shared by handle are aliases: notably the final DC result is the fluid force input. A checkpoint is GPU rollback storage, not a disk checkpoint. See the [allocation audit](../README.md) for distinct buffer accounting.

## Path-specific details

1. The two electrical basis solves use the same accepted interface and old fluid velocity. Each currently refreshes conductivity and reconstructs a full DC packet. Final voltage-controlled reconstruction overwrites those packets; it is **not a third PCG solve**.
2. Force refresh writes the full fluid packet before the actual fluid step. Viscosity may perform additional full face-buffer copies; EM/gravity/capillary forcing is added after stress diffusion. Fluid projection completion also reconstructs fields/diagnostics.
3. Momentum transport follows the same directional mass fluxes as interface advection. Updating the interface may require a second pressure projection. The conditional CPU connected-components path occurs only if active topology changes; air/liquid exchange alone normally leaves that mask unchanged here.
4. Stopped V5's large-grid watchdog workaround fences electrical chunks of at most five commands and individual fluid/MG stages. Standard smaller-grid execution batches more work. These waits are distinct from scalar readbacks; reducing their count is not automatically safe without checking the platform constraints.
5. Magnet B₀ is computed once during startup. This impressed-force client has no per-step induction/self-field or thermal evolution. Output uses one reusable staging buffer of at most32 MiB, writing mapped host bytes before publishing metadata. CSV snapshots are also flushed at saved frames.

## Source map and regeneration

| Sheet | Main source |
|---|---|
|1|`examples/conduction_flow.rs`; `src/run.rs` (`RunState::step`); `src/fluid/mod.rs`|
|2|`examples/conduction/gpu.rs`; `examples/conduction/solver.wgsl`|
|3|`src/fluid/mod.rs`; `src/fluid/solver.wgsl`; `src/fluid/multigrid.rs`; `src/fluid/multigrid.wgsl`|
|4|`src/surface.rs`; `src/surface.wgsl`; `src/surface_momentum.wgsl`; `src/fluid/phase.wgsl`; `src/run/export.rs`|

Editable definitions are in `render_activity.py`; `activity-data.json` is its exported row/loop content. Regenerate from the repository root with:

```sh
uv run --offline --with matplotlib python hardware/simulation-architecture-review/activity/render_activity.py
```

All four PNG layouts were visually reviewed. SVGs retain vector text for zooming. Source tracing establishes operation order and residency, not hardware-counter evidence of which kernel is bandwidth-bound.
