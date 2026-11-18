# Held applied-source fields

Prescribed wire paths, magnet geometry and uniform-field directions are fixed during an EM state. Re-evaluating their complete applied potential is redundant when all GPU source amplitudes are unchanged. The solver now retains the existing applied-field allocation and advances its history with a single edge pass (`source.y = source.x`). This makes ΔAs zero for that step while preserving conductivity, gauge flags and motional EMF in the same packet.

Amplitude comparisons use the exact f32 bit patterns consumed by the GPU. Changing any wire/uniform amplitude triggers the original direct source summation. Initialization always computes the field. Fallible waveform evaluation and both physical rollback paths invalidate the cache, so a failed changing-current step cannot leave metadata describing the rejected field. Material/interface changes still rebuild their coefficients; the externally prescribed applied potential is independent of those coefficients.

No additional grid-sized allocation, interpolation or field approximation is introduced. This is reuse of a static source result, not reuse of an old material reaction or Lorentz force.

## Verification and timing

1. A paired GPU regression compares cached fields with full recomputation across two independently driven CAD loops, a uniform background, constant intervals, amplitude changes and a forced failed solve/retry. The complete source/history/coefficient/motion packets, final fields and energy integrals are **byte-identical** at each accepted tick.
2. **162 regular headless tests pass**, with strict Clippy clean.
3. Three alternating warmed pairs of the actual native combined application use the coil/nozzle project with all five currents held at 100 A for 20 ticks. Applied-source GPU time falls from **168.518 to 3.036 ms (98.2% lower)**. All compute passes are timestamped, all diagnostic CSVs are identical, and input hashes are unchanged. No other agent GPU job runs during these timing pairs.
4. Median process time is **8.933 → 8.879 s (0.6% shorter)**. This is within run-to-run variation, so this case demonstrates **no clear whole-application speedup**. Induction dominates this small example. Warmups are excluded; first-launch shader compilation is not represented by the warmed medians. Benefit depends on source complexity and how often amplitudes remain constant; changing-current steps still use direct summation.
