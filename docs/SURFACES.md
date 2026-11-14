# Moving material, liquid/air motion and capillarity

## Use it

1. Assign the solder material and temperature in Setup. Imported liquid geometry supplies the initial material distribution; other included CAD solids remain fixed walls.
2. In Simulation, enable **Fluid motion → Moving liquid / air interface**. Enable **Thermal / phase evolution** to transport heat and let melting/freezing control mobility. Without thermal evolution the selected material is treated as liquid.
3. Set **Surface tension vs ambient** on the liquid material in Setup; set ambient density on **Air / void** in the same material library. The solder materials’ initial **0.5 N/m is illustrative**, not a calibrated value for either solder or its oxide-covered surface. Choose a small common timestep; capillary, viscosity and conservative advection limits are checked explicitly.
4. Simulate. **Liquid occupancy** displays material volume fraction α, including any frozen portion; **Liquid fraction** in the thermal views displays melt fraction f within that material. Actual molten volume is the integral of αf. Velocity and pressure also include the surrounding air. Both viewports sample the retained fields at the same time. The 3D view reconstructs the current α=0.5 surface; CAD hatching/outlines on its cut match the native 2D section.
5. Try `assets/hotend-free-surface.mhd.json`. It combines initially 250 °C solder, the CAD coil/magnets, thermal transport and a moving interface. Its default timestep is **2.5 µs**, with a larger EM iteration cap. The fixed-region and heating examples remain available separately. Repeating Simulate continues the run.

The pool can change shape and level as material moves into the surrounding air. Domain outer faces are impermeable: include sufficient air padding and keep interfaces away from them. This is an incompressible liquid/air model in a finite box, not a vented-gas or compressible-jet model. Periodic moving-interface runs are not implemented.

## Scheme

The GPU stores material occupancy α separately from melt fraction f. A conservative first-order donor-cell finite-volume update uses the projected MAC velocity on all nonwall cells. The total outgoing velocity CFL is bounded by `3 max|u_face| Δt/h ≤ 0.45`. Occupancies below 10⁻¹² are flushed before denormal arithmetic corrupts specific enthalpy; their volume/energy effects join the independently reported transport balances. There is no hidden mass renormalization. The method is diffusive: it is a starting interface scheme, not geometric PLIC reconstruction.

Pressure is solved through both liquid and low-density air. Cell density is `α ρ_liquid + (1−α) ρ_air`, with arithmetic face density. The pressure gradient, pressure operator and surface force use the same face mobility `ρ_liquid/ρ_face`, multiplied by the phase/mushy mobility. Fixed CAD walls and fully frozen occupied cells close faces. The liquid's kinematic viscosity is weighted by face occupancy; air is inviscid. This simple viscous operator is not a full variable-viscosity stress discretization.

Surface force is `σ κ ∇α`. Curvature uses local 3D height functions where a complete single-crossing stencil exists, with a smoothed-normal divergence fallback. Face curvature is weighted toward mixed cells so arbitrary curvature in pure air/liquid cannot pollute the force. For constant κ, the force balances the same discrete pressure gradient exactly. The explicit capillary bound is `Δt ≤ sqrt[(ρ_liquid + ρ_air) h³/(2 π σ)]`. The primary [Basilisk surface-tension](https://basilisk.fr/src/tension.h) and [curvature](https://basilisk.fr/src/curvature.h) documentation describes the force formulation, capillary stability condition and height-function approach; this is an independent Rust/WGSL implementation, not Basilisk's geometric VOF solver.

Thermal transport carries mixture enthalpy `α ρ (cp ΔT + L f)` with the same donor flux as α, then recovers specific temperature/phase using the updated occupancy. Conduction uses harmonic occupancy-weighted conductivities. Specified exposed-surface heat/flux scales with occupancy, which is an approximate voxel treatment rather than reconstructed interface area. Joule heat is already a mixture-volume source. Heaters and thermostats must be attached to static solids; attachment to moving material is deliberately undefined and rejected.

Each run owns a private evolving material-ID buffer. EM uses α-weighted conductivity from this buffer, refreshing edge coefficients before each induction step. The first coupled moving-liquid model requires unit liquid permeability; magnetic solids and prescribed magnets remain fixed. The CAD masks and Setup ID buffers remain unchanged. Conductive occupancy is quantized to 23 fractional bits; material IDs must fit eight bits. A common tick advances EM, fluid, conservative material/enthalpy transport, then thermal/phase and fluid topology. Complete frames retain α through the fluid packet, along with all other fields. A failed tick cannot publish partial state.

## Validation

1. A sphere transported by a closed-box, analytically divergence-free circulation preserves total material volume to the reported f32 reduction precision. Co-transported enthalpy error is below 2×10⁻⁷ relative; the blob moves and the original CAD IDs remain identical.
2. For a sphere of radius 0.25 m, exact volume-fraction initialization gives curvature RMS error **2.81% → 1.01%** on 24³/48³ grids. The deliberately large dimensional sphere is a numerical benchmark, not the target apparatus size.
3. With σ=0.5 N/m, Laplace pressure is **2σ/R=4 Pa**. Reconstructed curvature gives pressure-jump errors about **0.560% → 0.294%**, with decreasing parasitic velocity. A separate constant-curvature check isolates force/pressure balance from reconstruction error. The interface is transported and reconstructed during the reconstructed-drop check. These are short pressure-balance checks, not long-duration drop-stability tests.
4. A common moving-material/thermal test preserves volume, keeps uniform temperature, conserves enthalpy and verifies synchronized clocks, immutable Setup geometry and movement into initially empty cells. Offscreen checks compare live hatched caps at 256/512 pixels and render the complete Setup/Simulation workflow without desktop windows.

```sh
cargo test surface::tests -- --nocapture
cargo test moving_liquid -- --nocapture
./run.sh --run-check assets/hotend-free-surface.mhd.json 24
```

## Interpretation and remaining work

1. This is the first moving-interface implementation. It has **not validated nozzle pinch-off, satellite sizes or jetting thresholds**. First-order transport and initial cell-centre CAD occupancy can smear/alias a small drop. Resolve the intended droplet with many cells and demonstrate spatial/time convergence before trusting breakup; six cells across 0.5 mm is inadequate for that claim.
2. Contact angle/wetting, oxide skins, evaporation, compressible air, variable material-property curves, resolved viscous stress across the interface and moving wire-feed mechanics remain absent. Subcell CAD wall treatment and sharper geometric transport are next numerical improvements. Surface behaviour against staircase walls should be treated cautiously.
3. The explicit partitioned method has temporal splitting and interface/density-advection errors. The energy panel reports capillary work and estimated surface energy `σ ∫|∇α| dV`, plus their mismatch. This area estimate includes voxel/contact artifacts and is not an exact geometric-area or total-energy-conservation claim. Fluid loss includes unresolved numerical/interface work and is not deposited as heat.
4. Diffuse conductive tails can make the single-precision EM system poorly conditioned. The example passes its 60 µs check at 2.5 µs steps; a 10 µs run failed the strict EM residual check at a later tick. Use timestep refinement and the reported residuals. Failed solves stop explicitly rather than retaining an inaccurate result. Better EM preconditioning remains useful.

## Material references and older projects

Surface tension is an interface property. This first model stores each liquid material's scalar against the ambient material, currently ID 0 (**Air / void**); a general material-pair interface table is not implemented. Simulation settings only enable or disable the moving interface. Editing the material restarts dependent results, with no copy/fill workflow. Ambient density and EM relative permeability are read from Air / void; vacuum μ₀ remains a physical constant. Permittivity ε is not used by the magnetoquasistatic solver.

Project version 3 saves these material references and values, with no global air-density or surface-tension duplicates. Older global surface tension is moved to the selected liquid material when its new field was absent. Older air density is moved to Air / void when that material still held its default density; an explicitly edited ambient density takes precedence. Explicit unknown material values in version 3 remain unknown. Loading does not overwrite the project file.
