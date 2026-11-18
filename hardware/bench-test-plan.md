# Measurements that would turn the screens into a prototype decision

This is a proposed handoff sequence, not completed experimental evidence.
The leading geometry is a 3.2 mm inlet stainless V6 approximation with a
0.4 mm outlet and two external 0.8 × 0.4 mm tip contacts. The compact pole
variant uses 10 × 5 mm SmCo discs. Contact mounting and the driver are not
manufacturing-ready designs.

1. **Measure the actual nozzle and hot electrical path.** Record alloy and condition; do not infer near-unit magnetic permeability
   from the word stainless. Check outlet land,
   inlet and external contact positions against the generated CAD. Record
   four-wire resistance of the empty nozzle at operating temperature before
   fitting wall conductivity. Repeat after filling with solder. The difference
   constrains the liquid path, but does not uniquely identify steel/solder
   interface resistivity: the steel remains a parallel current path. Include
   the heater, frame and feedstock in the electrical diagram: an unintended
   external ground return can bypass the modeled two-terminal current path.

2. **Establish the stationary meniscus first.** Record fill height, alloy,
   temperature, surface preparation and atmosphere. Observe the unpowered
   nozzle for dripping, recession and wetting changes. The simulations start
   at rest with a neutral wall treatment; they do not reproduce a measured
   settled contact line or oxide skin. Observe the internal conducting path and
   external exit separately: a coating can improve electrical contact while
   changing external spreading and drop adhesion. Coupon wetting measurements can bound
   the missing behavior, but should not be treated as exact nozzle conditions.

3. **Measure the assembled magnetic field.** Compare accessible points with
   the actual source geometry, respecting probe temperature limits. Record
   magnet temperature and grade rather than assuming that the 0.95 T hot
   remanence used in the screen describes an arbitrary catalogue disc.

4. **Verify the driver on an electrical load.** Measure the push/retract
   waveform, rise time, ripple, terminal voltage and returned bus energy.
   Replace assumed series resistance and inductance with assembled values.
   The lumped voltage screen excludes PWM dynamics and switching losses.
   Current rating printed on a MOSFET is not a PCB thermal result.

5. **Compare single pulses with a zero-current control.** Use calibrated
   backlit imaging to measure drop count, equivalent size, direction, velocity,
   satellites and post-pulse meniscus recovery. Record the actual waveform
   rather than fitting only peak current. The research targets are 200 A for
   the larger poles and 250 A for the compact poles, with 250 µs positive pulses
   and a smaller retract pulse; their numerical trajectories remain provisional.

6. **Test recovery before increasing frequency.** A detached component is
   insufficient if the bore stays empty afterward. The completed cropped
   zero-gravity two-pulse screen does not demonstrate refilling or repetition.
   Compare retract strength and interval while keeping temperature and fill
   height recorded. Include feedstock only after the single-pulse response is
   understood; immersion changes both electrical and fluid geometry.

7. **Measure sustained mass and heat balance.** At 200 Hz, 0.5 mm SnCu drops
   imply approximately 25 mm/s of 0.8 mm solid feedstock and 10 W feed enthalpy;
   0.6 mm drops imply approximately 43 mm/s and 17 W under the recorded constant
   property assumptions. Weigh many drops as well as imaging individual ones.
   Track missed pulses, satellites and drift over a run before claiming the
   requested rate. Heater losses and retained pulse heat are separate terms.

The corresponding inputs and limits are in [designs](designs/README.md),
[driver-hot-allowance.json](driver-hot-allowance.json),
[feedstock-screening.json](feedstock-screening.json) and [README.md](README.md).
