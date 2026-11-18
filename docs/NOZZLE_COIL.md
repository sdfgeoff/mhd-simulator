# Coupled coil/nozzle example

`assets/nozzle-coil.mhd.json` runs the nozzle through the normal combined EM/fluid/interface engine. There is **no prescribed fluid force**. Currents belong to five closed centreline curves imported from `assets/nozzle-coil.gltf`; each can be selected and edited independently in Setup. Lorentz force drives the fluid, fluid velocity supplies motional EMF, and evolving liquid occupancy updates conductivity on the common clock.

## Try it

1. Open `./run.sh assets/nozzle-coil.mhd.json` or use **Load file**.
2. Inspect the two solids and five current loops in Setup. The wall uses Alumina; the pool references **Benchmark conducting liquid (uncalibrated)**.
3. Switch to Simulation. The saved timestep is **0.5 µs**. Set **150 steps** to reach **75 µs**, covering the current pulse. Repeat Simulate to continue the coupled evolution after the current reaches zero.
4. Use Section view to choose B, induced J, Lorentz force, Joule heating, liquid occupancy, velocity or pressure. These use the same timeline frame in both viewports. Trails can show field direction or fluid motion. Increase the retained frame window and recording interval to suit the trajectory you want to inspect.

Regenerate both examples without a CAD kernel:

```sh
cargo run --offline --example make_nozzle          # prescribed pulse regression
cargo run --offline --example make_nozzle -- --coil # computed EM forcing
cargo run --offline -- --run-check assets/nozzle-coil.mhd.json 150
```

## Inputs and interpretation

The wall and pool match the [nozzle regression geometry](JET_EXPERIMENT.md). Five ideal circular current paths have radius **0.95 mm**, at z=−0.9, −0.7, −0.5, −0.3 and −0.1 mm. Each curve has 64 straight segments and explicitly coincident endpoints. The source regularization radius is **0.05 mm**. Every loop rises linearly from 0 to **100 A over 25 µs**, holds for **25 µs**, and falls to zero over **25 µs**. These independent ideal loops approximate a winding field; they do not include a circuit, wire resistance, coil heating or a physical helical return lead.

Liquid density is **7000 kg/m³**, dynamic viscosity **0.002 Pa·s**, surface tension **0.5 N/m**, electrical conductivity **2×10⁶ S/m**, and relative permeability 1. These are explicit constant benchmark assumptions, not calibrated molten-solder data. Thermal evolution is off: Joule heating is reported, but does not change temperature or viscosity. The rear liquid surface is open to air, without replenishment; static CAD walls use the existing neutral contact model.

The initial domain is **3.6×3.6×4 mm**, with **50 µm** cells (72×72×80). The aperture has only six cells across. The reaction magnetic field uses the existing finite-box boundary condition; the prescribed source field itself is evaluated from the curves. Domain size, mesh spacing, source segmentation, timestep and solver tolerance require refinement checks before interpreting jetting thresholds or droplet size. A 75 µs field/flow check establishes that the application runs the coupled case; it is too short to assess late breakup. The 100 A setting is a starting experiment, not an asserted jetting current.

## Initial numerical controls

The 0.5 µs pulse check and a matched no-current control both complete 150 ticks. The former reaches a whole-domain peak B of **0.3965 T** (including points near the ideal source paths); the latter has zero B and zero Joule heating. Final kinetic energy is **1.494×10⁻¹³ J** with the coil and **1.187×10⁻¹⁴ J** without it. Initial meniscus relaxation remains present in the no-current case. Neither value establishes a droplet-release threshold.

1. Enlarging the air domain to **4.8×4.8×5 mm** changes accumulated Joule heat by **−0.0060%**, final kinetic energy by **−0.0013%**, and final maximum speed by **0.0078%**. The weak residual B at 75 µs differs by **0.60%**, or **10.5 µT**; differences normalized by the peak drive field are much smaller. This is one domain check, not a convergence proof.
2. Halving Δt to **0.25 µs**, with all material/source data unchanged, changes Joule heat by **0.039%**, final kinetic energy by **2.4%**, and final maximum speed by **3.0%**. The largest transient speed difference is **11.2% of the original peak**, so the fluid motion is not yet time-converged. Volume drift remains below **7×10⁻⁸** in these controls.
3. These controls use EM relative tolerance **10⁻⁴**, pressure relative tolerance **10⁻⁵**, and absolute RMS divergence tolerance **10⁻⁵ s⁻¹**. The original solver stopped near 80 µs after switch-off. With [weak-field residual normalization](EM_RESIDUAL_SCALING.md), both the static-conductor case and the coupled moving-interface application complete **400 ticks to 200 µs**. The coupled run retains volume within **6.52×10⁻⁸**. Its accepted 75 µs prefix closely matches the original solver (peak-normalized speed difference below **7×10⁻⁷**, Joule-energy difference below **5×10⁻⁹**). This is a continuation check, not a jetting demonstration.
