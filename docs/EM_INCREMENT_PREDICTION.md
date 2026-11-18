# EM increment prediction

The normal application predicts the next reaction-potential increment from accepted history, without allocating another field buffer. This changes the initial guess, not the physical operator or acceptance tolerance.

## Method

The EM solver already solves a reaction-potential increment and retains the last
two accepted absolute reaction potentials in `work.x` and `previous`. Predict the
next increment by their difference, scaled by the ratio of consecutive timesteps.
This reuses existing buffers. The first dynamic step and applied-field solves
keep a zero guess; constrained boundary increments remain zero.

`prepare` now distinguishes the original right-hand side `b` from `r=b-A*x_guess`.
The stopping threshold remains `tol²*||b||² + 1e-38`, and the original physical
true-residual check still decides acceptance. An improved guess therefore cannot
silently redefine the requested relative tolerance.

If the guessed residual is nonfinite or larger than the zero guess's residual,
reset the increment to zero and rebuild the same solve. The timestep history is
updated only on acceptance. A failed solve can discard prediction history while
retaining a valid physical state for retry.

## Initial checks

1. Eleven existing regular EM GPU tests pass, including analytic fields,
   conductivity/permeability response, motional torque and energy accounting.
2. Three new controls pass: variable timesteps; failed solve/restoration/retry;
   and deliberately corrupted history, including overflow. Rejecting corrupted
   history reproduces the zero-guess final fields **byte-for-byte**.
3. Variable-step B, J, force and heat comparisons against zero-guess GPU solves
   stay within the original 2e-4 test bound (observed differences below 1e-6).
   There is no duplicate CPU field solver. Strict Clippy passes.

## Coupled V6 probe

At the unchanged 10 µs timestep, EM tolerance 1e-4 and original cap of 1,000
iterations, the prediction candidate completes ten coupled V6 ticks. Zero-guess
EM fails at tick five under that cap; a separate diagnostic needs 1,057 iterations
for that tick at the same tolerance.

The candidate needs 858/554/384/381/**369** iterations for ticks one through five.
At tick six the drive stops rising: the guessed residual is 3.415 times the
zero-guess residual, so the safeguard rejects it. Later predictions are useful
again. Working-buffer bytes remain unchanged. A full-field comparison against the zero-guess, larger-work diagnostic gives
relative L2 differences 1.24e-5 for B, 9.84e-5 for J, 5.57e-5 for force and
6.74e-5 for heating. All predicted solves satisfy the original 1e-4 operator
tolerance, with maximum reported residual 7.29e-5. Liquid volume is identical at every compared tick; the fraction-field relative L1 difference is 1.26e-9. These checks use the production connected-pressure solver. The reference alone raises the EM work cap to 4,000 to obtain a converged comparison; the candidate retains 1,000. Geometry, material properties and tolerances are unchanged.

## Application timing and acceptance

Three alternating warmed pairs of the actual headless application (`--run-check`, three coupled V6 ticks) give median process time **24.2494 → 18.3170 s (24.5% shorter)** and EM GPU time **20,292.66 → 14,173.15 ms (30.2% lower)**. Pressure time is essentially unchanged. All GPU passes are recorded. Warmups are excluded; the cold candidate took 55.23 s. No other numerical GPU job ran during this final timing comparison. These figures describe this project/hardware, not a universal speedup.

The final application also completes ten ticks with the original 1,000-iteration cap. The full headless suite passes 157 tests, with strict Clippy clean. Zero-RHS steps disable prediction until useful history exists; failed solves also clear its validity. This avoids extrapolating stale or overwritten history on a later retry.
