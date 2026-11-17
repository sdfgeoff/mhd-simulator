# Interface momentum transport

Geometric VOF conserves material volume, but that alone does not conserve momentum. The current fluid solver advects face velocity with a semi-Lagrangian RK2 backtrace; the interface separately transports density with geometric liquid fluxes. Their different transport operators can change the integrated momentum of a moving high-density drop, even without external forces.

## Reproducible baseline

```sh
cargo test --offline nonuniform_periodic_metal_drop -- --ignored --nocapture
```

This diagnostic writes `artifacts/periodic-momentum.csv`. It initializes a spherical drop on a 24³ periodic unit box, density ratio 7000:1, zero surface tension/gravity and negligible liquid viscosity. Velocity combines uniform translation with a smooth, discretely divergence-free staggered vortex. It advances 64 steps of 0.01 s. Momentum uses each unique periodic MAC face once, with the same arithmetic face density used by the pressure operator. This avoids changing the diagnostic by interpolating velocity back to cells.

At commit 38c5d71, relative vector momentum error after 0.64 s is **2.391%**, while volume drift is zero to the reduced diagnostic precision. Kinetic energy falls by **4.293%**. The test is deliberately an ignored measurement, not a momentum-correctness assertion: it currently asserts only finite results. The uniform-velocity periodic regression cannot detect this error.

## Next discretization

A consistent update must carry momentum with the mass flux implied by VOF, then recover velocity using the updated density. On this solver's MAC grid, the dual volume of a face is formed from half of each neighboring primal cell. Its density is their arithmetic mean; averaging the neighboring primal mass fluxes gives a dual mass balance consistent with that density. Momentum fluxes must be shared between neighboring dual volumes, and the split-compression contributions must cancel over a divergence-free full step. Pressure then projects velocity using the updated density.

This is a discretization change requiring conservation, uniform-flow, capillary-dynamics and rollback validation. It must also replace the existing velocity advection for moving-interface runs to avoid advecting twice. The first implementation can use upwind transported velocity; its dissipation must be measured before adding higher-order reconstruction.

[Arrufat et al., *A mass-momentum consistent, Volume-of-Fluid method for incompressible flow on staggered grids*](https://arxiv.org/abs/1811.12327) describes the physical consistency requirement and staggered-grid complications. The planned arithmetic dual-volume construction follows this solver's existing face-density definition; it is not a claim to reproduce that paper's auxiliary geometric reconstruction.
