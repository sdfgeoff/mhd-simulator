# Weak-field EM residual scaling

A switched-off coil exposed a single-precision CG failure after its field decayed below 10⁻⁹ T. The failure also occurs without fluid motion. A smaller GPU regression reproduces it on the first step simply by multiplying the applied drive by 10⁻⁸: the former implementation reaches 4,000 iterations with relative residual 1.11×10⁻³ against the requested 10⁻⁴.

The amplitude regression identifies a loss of arithmetic homogeneity in the per-edge dot products. Even when the global norm and requested relative residual are representable, individual residual/search products can enter the subnormal range and flush to zero before reduction. WGSL explicitly permits flushing subnormal arithmetic inputs/outputs to zero; see [floating-point evaluation](https://www.w3.org/TR/2026/CRD-WGSL-20260825/#floating-point-evaluation). Iteration and true-residual restart then stop behaving homogeneously under a change of source amplitude.

## Method

The physical reaction increment and its accepted history stay in SI units. Only residual and search vectors use an internal common scale `s`:

```
r_hat = r / s
p_hat = p / s
x_physical += s * alpha * p_hat
||r_hat||² <= tolerance² * ||b_hat||² + (1e-19 / s)²
```

This is the original acceptance inequality `||r||² <= tolerance² ||b||² + 1e-38`, expressed in normalized units. The true residual is still independently evaluated from the physical equation before normalization. No weak field is explicitly discarded, and no physical coefficient, conductivity mask or requested tolerance changes.

The first preparation also reduces `sum(abs(b))`, which remains representable when individual squares underflow. For an L1 norm below 10⁻⁸ or above 10⁸ in internal equation units, the GPU chooses an exact power of two from its exponent, bounded to exponents −100…100. The host repeats preparation once using that scale. Those bounds protect arithmetic range; they are not physical field cutoffs. Ordinary solves retain `s=1` and the existing dispatch sequence. A zero RHS also retains unity.

The existing 128-byte scalar buffer has a free vector for the scale. No field allocation or CPU reference solver is added. Prediction rejection, true-residual restarts and rollback retain the same physical state/history conventions.

## Checks

The amplitude regression checks linear scaling of B/J/applied B and quadratic scaling of force/Joule heating. With normalization, relative L2 differences after removing the amplitude factor are 3.18×10⁻⁷ (B), 1.35×10⁻⁶ (J), 1.27×10⁻⁶ (force), 1.09×10⁻⁶ (heating) and 2.16×10⁻⁷ (applied B). The test independently converts the accepted norms back to physical equation units and checks the original combined relative-plus-numeric-floor criterion. At extremely weak amplitude the existing absolute numeric floor can dominate the nominal relative term. This tests arithmetic homogeneity, not a second numerical implementation.

All **161 regular headless tests pass**, with strict Clippy clean. The native coil/nozzle application completes **400 coupled ticks to 200 µs**; the matching static-conductor case also completes 400 ticks. Both previously stopped near 80 µs. The native tiny-current case now completes its four-tick check, which previously failed at the first tick. Maximum sampled liquid-volume drift in the coupled run is 6.52×10⁻⁸.

The matched ten-tick V6 comparison retains the original 1,000-iteration cap and 10⁻⁴ tolerance. B/J/force/heating relative L2 differences against the preceding production implementation are 8.08×10⁻⁷, 9.83×10⁻⁷, 8.51×10⁻⁷ and 9.10×10⁻⁷ respectively. Material volume and working-buffer bytes are identical at every compared tick; fraction-field relative L1 difference is 4.44×10⁻¹¹. Iteration counts differ slightly because floating-point CG paths change.

Scaling cannot remove the precision limits of the physical f32 fields themselves, such as cancellation against a much larger background.
