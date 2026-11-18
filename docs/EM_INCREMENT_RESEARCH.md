# EM increment prediction research

This branch tests an initial-guess improvement. Prediction is enabled only in
test builds; it has not been integrated into the main application. No speedup
claim is based on the overlapping validation runtimes.

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

The candidate needs 858/554/384/381/**368** iterations for ticks one through five.
At tick six the drive stops rising: the guessed residual is 3.415 times the
zero-guess residual, so the safeguard rejects it. Later predictions are useful
again. Working-buffer bytes remain unchanged. Full-field comparison against a
zero-guess, larger-work diagnostic is pending; timings must be measured with the
other GPU jobs idle before considering adoption.
