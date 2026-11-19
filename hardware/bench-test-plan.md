# Prototype qualification measurements

This is a proposed experiment sequence, not completed bench evidence. The current
mechanical candidate is the short M6 nozzle with a 0.8 mm square outlet boss,
0.2 mm nominal wall, R0.1 root and heated external copper fingers. The thinner
0.7 mm boss remains an electrical-performance reference. The models do not
establish physical operating limits or sustained jetting.

1. **Check the machined assembly.** Measure the 0.4 mm bore, 0.6 mm land, root
   radius, wall concentricity and contact position. Preserve the top nozzle-to-
   heatbreak seal and specify the thread/root and assembly torque from the real
   materials and heater fit. The nominal contact top is 0.05 mm below the fillet
   start; that clearance needs a positioning tolerance. The model has no clamp
   preload or stress calculation. Check actual stainless permeability; an alloy
   label alone does not establish the unit-permeability assumption.

2. **Qualify the unpowered thermal assembly.** Fit the real copper leads,
   electrical insulation, clamps, magnets and feedstock arrangement. Measure
   heater, outlet/finger and magnet temperatures through warm-up and a steady
   soak. Characterize sensor loading and optical emissivity where applicable.
   The stationary screen gives about 239 °C at the outlet with a 250 °C ideal
   heater and 25 °C far lead ends under its poorer-interface assumption. It
   omits wire melting, exterior losses and pulse heat. Establish measured margin
   above the alloy liquidus, including measurement uncertainty; an initial
   engineering target of at least 10 K is a test target, not demonstrated margin.
   Cold nearby copper can freeze the outlet despite the raised pool. Record
   heater power and actual clamp/insulator heat transfer.

3. **Qualify the magnet grade and assembled field.** Obtain the exact grade and
   its hot demagnetization/recoil curves. A generic maximum-temperature label
   does not certify this geometry. Map accessible gap-field points and record
   both magnet temperatures. Repeat the field measurement after cooling from
   the soak to distinguish reversible temperature change from irreversible loss.
   The near-surface internal mesh-source samples are inaccurate and are not
   accepted evidence of demagnetization margin. The separate analytical bar
   bound is conditional on fixed, aligned magnetization and omits current fields.

4. **Measure the electrical paths hot.** Record four-wire resistance before and
   after filling, and partition nozzle, copper and joint voltage drops. Steel is
   a parallel path; empty/filled differences do not uniquely identify liquid/
   wall contact resistivity. Record heater, carrier and feedstock grounding:
   an unintended third current path changes the modeled two-terminal problem.
   The current coarse 7.5 mΩ fixture result includes two 50 mm copper extensions; count
   those only once in a driver budget. Inspect the real contact area and its
   stability: low total resistance does not establish absence of a microscopic
   pulse hotspot. A bulk thermocouple cannot resolve a 250 µs contact transient.

5. **Verify the driver on an electrical load.** Use the separate pulse-generator
   handoff for waveform, ripple, current regulation and active recovery tests.
   Replace assumed R/L with measured assembled values. The electronics capacity
   is not a nozzle current limit. For orientation only, the radiused 0.8 mm
   screen uses a 110 A / 250 µs positive comparison pulse, 20 µs ramps, a 25 µs
   gap and a quarter-amplitude reverse lobe for 83.3 µs. Its modeled dose is
   2.745 A²s, or 23.4 A RMS at an assumed 200 Hz. No real-head waveform is
   qualified by those numbers.

6. **Establish the stationary meniscus, then compare single pulses.** Record
   alloy, temperature, fill height, surface preparation and atmosphere. Observe
   unpowered dripping, recession and wetting first. Record solid versus flux-
   cored feedstock; the current model contains no flux or oxide-skin dynamics.
   Use calibrated backlit imaging for drop count, size, velocity, direction,
   satellites and recovery, alongside the measured electrical waveform. Compare
   a zero-current control. The existing coarse trajectories are provisional;
   the current geometry has no qualified drop-size or velocity prediction.

7. **Measure recovery before rate.** Verify that the bore refills and the
   meniscus recovers without gas entering the conducting path. The earlier
   two-pulse model did not demonstrate repeatable jetting. Observe feed dipping
   separately because it changes electrical and fluid geometry. A detached
   component alone does not establish one drop per pulse.

8. **Measure sustained mass and heat balance.** At 200 Hz, 0.5 mm SnCu drops
   imply about 13.1 mm³/s: roughly 25 mm/s of 0.8 mm solid wire or 5.4 mm/s of
   1.75 mm wire. The constant-property feed enthalpy estimate is about 10 W.
   Weigh many drops as well as imaging individual events. Track missed pulses,
   satellites, wetting drift, joint resistance and temperatures during a run.
   Feed enthalpy, exterior losses and retained electrical heat are separate
   terms; the ideal fixed-temperature heater model has no power limit.

See the [current study](README.md), [CAD inputs](designs/README.md),
[heated-contact thermal controls](m6-hot-contact-thermal-screening.json) and
[pulse-generator handoff](pulse-generator-handoff.md). These measurements are
what would turn numerical candidates into a hardware decision.
