# Stationary screen completion

1. `hardware_probe_decay` is an opt-in native entry point for a single
   table-driven coil with an explicitly permanent zero-current tail. Fluid,
   thermal and moving-interface modes must be disabled. The source must start
   at zero; uniform and permanent-magnet sources are excluded. Existing queued
   binaries and fixed-window experiments retain their original settings.
2. Every accepted PDE step is computed. After the declared tail transition,
   eight successive accepted states must have reaction-field energy/generated
   heat ≤10⁻⁸ and metal-work balance defect ≤10⁻³. These are the existing
   stationary analysis gates. The tool then exports the endpoint and records
   `completion_criterion: stationary_decay`, accepted steps and the requested
   maximum. Exhausting that maximum without meeting the criterion is a failure.
3. This ends the stationary experiment; it does not clamp fields or shorten
   a moving-fluid simulation. All accepted-step profiling is drained and
   retained. `scripts/verify_stationary_decay.py` independently checks the CSV,
   permanent-zero source, full step/profile coverage and final field export.
4. A batch job must explicitly declare `completion: stationary_decay` and use
   `--probe-binary target/debug/examples/hardware_probe_decay`. The runner
   freezes the verifier and requires its checks before reporting completion.
   Existing fixed-window analysis continues to require its declared full window.
5. [Retrospective checks](stationary-endpoint-screening.json) on completed
   heater-off/on datasets would avoid 84% and 71% of their steps. Peak force
   ranges are unchanged, recorded heat changes by less than7×10⁻⁹ relatively,
   and omitted axial impulse is negligible in those datasets. Step reduction
   is not measured wall-time speedup or a universal error bound.
6. Three CPU gate tests pass and the new executable builds. Native adaptive
   completion has not yet been exercised; no current or queued physical case
   has been switched to it. Validate its first use before relying on throughput
   estimates. The active fixed-window analyzer remains frozen for the running
   heater-slot experiment.
