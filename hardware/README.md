# Solder jetter hardware study

Research branch: `research/hardware-design-20261118`. Frozen application baseline:
`simulator-baseline-2026-11-18`. Current evidence below is from 19 November 2026.
**No design has demonstrated sustained 200 Hz jetting or been qualified for
manufacture.** The target remains 0.4–0.6 mm solder drops, 24/48 V supply and
actuator currents below a few hundred amps, using accessible parts.

The circuit-design handoff is [pulse-generator-handoff.md](pulse-generator-handoff.md):
a proposed ±300 A peak / 100 A RMS bipolar current driver on 24/48 V.

## Leading candidates and completed controls

Completed moving-fluid results use **zero gravity** unless explicitly marked
gravity-on. The matched gravity-on tip-contact control is now complete.

| Configuration | Accepted evidence | Main unresolved issue |
| --- | --- | --- |
| Stainless V6 approximation, 0.4 mm bore, 0.8 × 0.4 mm external tip contacts, larger SmCo poles | Gravity-on full-height 100 µm case: one 0.59993 mm component moving down at 0.1795 m/s at 3.6 ms, with 200 A / 250 µs. | 50 µm result is oversized and materially faster; refill/repetition, electrical wetting and contacts remain open. |
| Same tip contacts, one 10 × 5 mm SmCo disc per pole | 100 µm case: one 0.5962 mm component moving down at 0.212 m/s at 3.6 ms, with 250 A / 250 µs. | Higher peak current, local heating and omitted self-field; no fine-grid or gravity/repetition confirmation. |
| V6 with two milled collar flats and immersed 1 mm tungsten pins | Fixed 1.5 µs rerun completes 4.2 ms without a detached component. The earlier 3 µs run hit its CFL guard at 2.154 ms. | Gas is drawn between the electrodes; a large cold force did not establish jetting. |
| Custom stainless chamber, one ten-turn coil | Normal unified solver completes 3.597 ms at 250 A / 50 kHz without resolved ejection on the 200 µm grid. Matched zero-current control completes too. | Coarse interface; geometry, frequency and loaded driver remain open. |
| Same custom chamber, two phased six-turn coils | Cold 200 A-per-coil phase reversal changes mean axial force from −0.059 to +0.096 N over 80–100 µs. Positive-phase timestep halving changes force 0.93%. | Unified 200 µm flow completes 3.597 ms without resolved ejection; coil placement is unoptimized and loaded two-port circuit unresolved. |

The single-coil chamber has **6 mm ID, 0.5 mm radial stainless wall, 0.5 mm
aperture and 0.15 mm exit land**. The thin exit land must not be confused with
radial wall thickness. The V6 cases use a **3.2 mm inlet**, corresponding to the
2.85 mm-filament variant, rather than the 1.75 mm-filament nozzle.

The original **smaller-contact** V6 reference completes 3.6 ms at 50 µm with a
0.618 mm component moving down at 0.504 m/s. Its corresponding coarse result is
0.591 mm / 0.289 m/s: velocity and breakup timing are **not mesh-converged**.
This is a different contact geometry from the wider-contact candidate above.
The wider-contact 50 µm run has now completed 3.6 ms with one separated component:
0.6253 mm / 0.3781 m/s downward at alpha 0.01, or 0.6128 mm / 0.3833 m/s at alpha
0.5. Its coarse counterpart is 0.5947 mm / 0.1608 m/s. This combined grid/timestep
refinement changes diameter by 5.2% and speed by 135%; the result is not converged.
No saved fine-grid liquid reaches a boundary; relative volume drift stays below
4×10⁻⁸. [Completed record](tip-wide-fine-completed.json).

Equivalent diameters in the table use six-neighbour connectivity at alpha 0.01,
excluding the main pool and components smaller than one alpha-weighted cell.
Other thresholds are retained in the records. A component is not automatically
a departing, repeatable jet.

The next bounded waveform control is [delayed retraction](retract-delay-screening.json):
the completed flat-tip 150 A case repeats with a 500 µs zero-current gap instead
of 25 µs. Geometry, pulse amplitudes/durations and gravity remain fixed. It is
queued behind the first near-flush trial; no result is assumed.

## Physical scope and limits

1. **Gravity and domain boundaries.** Earlier generated flow cases use
   `acceleration_m_s2: [0, 0, 0]`. Their velocities isolate electromagnetic and
   capillary motion. The full-height zero-gravity control preserves the wider
   contact drop size within 0.03% but changes speed by 8.1% relative to the
   cropped case. It changes domain extent and periodic air return together.
   The matched gravity-on case gives 0.59993 mm / 0.1795 m/s instead of
   0.59484 mm / 0.1477 m/s without gravity. Exported settings match except uniform
   acceleration; initial states start at rest, not from settled hydrostatic/
   wetting equilibrium.

2. **Repetition has not passed.** The cropped zero-gravity two-pulse case
   completes 9.999 ms without a second resolved detached component in saved
   frames. Its first component later turns upward. Liquid reaches a side
   boundary by the saved 7.2 ms frame. This does not demonstrate 200 Hz jetting.

3. **Wetting and electrical contact.** The current free-surface wall treatment
   is mirror/neutral, without measured advancing/receding angles, hysteresis or
   oxide skins. A steel/solder interface resistivity of 10⁻⁹ Ωm² reduces the
   wider-contact cold force by 43%. This is not copper lead/nozzle joint
   resistance. Internal electrical wetting and external drop adhesion need
   separate measurements; coating either surface may change both behavior and
   current paths. Published angles are not substituted for nozzle measurements.

4. **Thermal estimates are not solved temperatures.** Refining the wider-contact
   cold field from 100 to 50 µm increases its initial-field adiabatic steel peak
   rise from 20.1 to 34.7 K for 200 A / 250 µs, despite only a few percent change
   in total energy and force. Scaling the same stationary current path to 250 A
   gives 54.2 K. Diffusion, actual contact-edge radii, temperature-dependent
   conductivity and joint losses remain absent. Local heating is not converged.

5. **Direct-current magnetic approximation.** The research charge solver uses
   `J = sigma(-grad phi + u cross B0)` with static permanent-magnet field B0.
   Accepted liquid fraction updates conductivity; forces drive the normal
   fluid/interface engine. Applied-current self-field and magnetic reaction are
   omitted. Postprocessed local force corrections are roughly 10–15% for the
   leading tip cases, and are not fed back. External return wiring is absent. The assumed 0.95 T hot magnet remanence
   does not qualify a catalogue grade or its hot load line. Specify nozzle alloy
   and condition: room-temperature stainless datasheets do not establish the
   hot conductivity or finished-part near-unit permeability assumed here.

6. **Alloy transfer is incomplete.** Sn60Pb40 separates with the smaller
   contacts in a coarse trial at about 0.600 mm / 0.14 m/s, while wider-contact
   variants are weak or do not detach under the tested pulses. One waveform
   does not serve both alloys automatically. Material values and their source
   limitations are retained with each study.

7. **Single-coil negative is specific.** Compared at identical exported settings
   except current, the driven liquid has final cell-centred RMS speed 0.0131 m/s
   versus 0.00180 m/s without current. Final alpha L1 difference is 14.3 nL in a
   168,352 nL pool; that is an interface-difference diagnostic, not ejected mass.
   No resolved liquid is below the exit in either case. This does not reject
   every induction geometry or establish fine-grid convergence.

## Packaging, electrical compliance and feed

These are **generated CAD envelopes**, before heaters, mounts, insulation,
external lead routing or tolerances. Coil bounds include the stored return lead.

| Packaged case | Nominal envelope, x × y × z |
| --- | --- |
| Wider-contact V6, larger poles | 20 × 24.2 × 30.7 mm |
| Compact-pole V6 | 10 × 14.2 × 20.7 mm |
| Single-coil custom chamber | 17.3 × 13.7 × 11 mm |

The direct-current lumped screen, using 5.5 mΩ non-head resistance and 1.025 µH
series inductance, needs about **11.56 V at 200 A** or **14.45 V at 250 A**.
Head-plus-series heat is about **11.9 or 18.6 W at an electrically assumed
200 Hz**. This preserves 24 V voltage headroom but excludes PWM/gate losses,
control dynamics, actual contact resistance and thermal qualification.

A stock 0.68 µH Vishay IHXL1500VZEBR68M51 is an alternative to the illustrative
four-inductor bank. Its roughly 38 × 38 × 22 mm body belongs on a remote driver
PCB. Hot differential inductance and ripple/core losses remain unresolved.
The recorded MOSFET example is a 60 V part on a 24 V bus, not a completed bridge.

The scaled single-coil resonant estimate at 250 A implies roughly 62 W
electrical heat at 200 Hz before AC winding and switching losses, with internal
reactive voltages above the bus.
The two-coil air matrix has about 0.388 µH self inductance, 0.0974 µH mutual
inductance and coupling coefficient 0.251. Prescribed quadrature currents imply
about 612 W of opposite-signed **air mutual power exchange** during the steady
envelope. This is internal exchange, not loaded consumption. Independent
uncoupled resonance estimates are insufficient for its driver.

At 200 Hz, 0.5 mm SnCu drops require approximately **25 mm/s of 0.8 mm solid
wire** and **10 W feed enthalpy** under the recorded constant-property budget.
At 0.6 mm these rise to roughly 43 mm/s and 17 W. Heater losses and retained
pulse heat are separate; no replenishing wire is simulated in the flow trials.

## Inspect and reproduce

1. [designs/README.md](designs/README.md) contains tracked CAD/project bundles,
   manifests and headless reproduction commands. Payload hashes and project
   equivalence were checked after packaging. These are research inputs, not
   manufacturing drawings.

2. Open a bundled project in the main GUI to inspect CAD/materials. The
   **contact-current cases require the research conduction client**; their
   terminal-current experiment is not a GUI electrode feature. The induction
   cases use the normal unified `RunState` engine.

3. Measure accepted exports with `scripts/hardware_flow_report.py`. It records
   component thresholds, below-tip volume, all six boundary maxima and volume
   drift. `scripts/compare_native_flows.py` compares completed matched clocks
   and native cells; explicit `--overlap` permits different domains only when
   their cell centres align, without interpolation.

4. `scripts/compare_phase_screens.py` integrates accepted cold diagnostics over
   a common cycle. `scripts/winding_matrix.py` estimates the air winding matrix
   with lumped Neumann integrals and reciprocity/distant-loop checks. Neither
   substitutes a CPU volumetric physics solver.

5. [bench-test-plan.md](bench-test-plan.md) proposes measurements of the actual
   nozzle, hot resistance, wetting, field, pulse waveform, recovery and sustained
   mass/heat balance. They have not been performed.

## Evidence records

| Subject | Record |
| --- | --- |
| Requirements and source provenance | [requirements.json](requirements.json), [sources.json](sources.json) |
| Original fine-grid reference | [tip-grid-screening.json](tip-grid-screening.json) |
| Compact poles and pulse variants | [compact-tip-pole-screening.json](compact-tip-pole-screening.json), [tip-pulse-screening.json](tip-pulse-screening.json) |
| Domain, gravity and queued pulse controls | [gravity-controls.json](gravity-controls.json) |
| Completed two-pulse limitation | [tip-repetition-screening.json](tip-repetition-screening.json) |
| Hotspot refinement | [tip-heat-grid-screening.json](tip-heat-grid-screening.json) |
| Alloy and cross-channel controls | [snpb-tip-screening.json](snpb-tip-screening.json), [cross-fine-screening.json](cross-fine-screening.json) |
| Immersed electrodes | [immersed-pin-flow-screening.json](immersed-pin-flow-screening.json), [milled-pin-flow-screening.json](milled-pin-flow-screening.json) |
| Native single- and two-coil trials | [single-coil-flow-screening.json](single-coil-flow-screening.json), [two-phase-coil-screening.json](two-phase-coil-screening.json) |
| Electrical and feed budgets | [driver-hot-allowance.json](driver-hot-allowance.json), [driver-comparison-screening.json](driver-comparison-screening.json), [feedstock-screening.json](feedstock-screening.json) |
| Self-field and analytical cylinder limits | [conduction-self-field-screening.json](conduction-self-field-screening.json), [cylinder-induction-screening.json](cylinder-induction-screening.json) |
| Surface evidence and geometry bounds | [wetting-evidence.json](wetting-evidence.json), [candidate-envelopes.json](candidate-envelopes.json) |

## Machinable tip-contact variant

The user has authorized custom V6-like machining. Two opposed external flats,
0.8 mm apart over the lower 0.6 mm, retain the 0.4 mm bore with a nominal 0.2 mm
minimum steel wall. In matched 100 µm cold screens, this increases outlet-land
force by 73% while decreasing total liquid force by 26%. With the compact
magnets at 150 A, its cold outlet-land force is about 8% above the unmodified
large-pole case at 200 A. The first gravity-on 150 A trial has now completed without detachment by
3.6 ms. The cold metric did not establish equivalent jetting. Clamping, cutter radii and tolerances remain open.
[custom-tip-flat-screening.json](custom-tip-flat-screening.json) retains geometry,
fields, validation and electrical budgets.

[Bias-field alternatives](bias-field-options.json) records the SmCo lifetime
assumptions and an illustrative DC electromagnet estimate.

## Magnet projection and a shorter flight path

Global CAD checks and native GPU applied-field observations give the following
mean useful field in the 0–0.6 mm outlet land. All share assumed hot remanence
0.95 T and unit permeability; these are not flow-performance comparisons.

| Layout | Magnet projection below exit | Mean transverse field |
| --- | --- | --- |
| Current 10 × 5 mm discs, 4.2 mm pole gap | 8.2 mm | 0.312 T |
| Raised discs, 8.4 mm gap clearing the unmodified shoulder | 5.2 mm | 0.222 T |
| Raised discs, 4.2 mm gap and 4 mm shoulder clearance flats | 5.2 mm | 0.407 T |
| Short 10 × 5 mm pole face, 5 mm magnetic depth | 0.2 mm | 0.257 T |
| Short 25 × 5 mm pole face, 10 mm stacked magnetic depth | 0.2 mm | 0.318 T |

The short stacked option uses two catalogue 25 × 5 × 5 mm bars per pole, with
magnetization aligned through their 5 mm thicknesses. It is wider: the magnet
assembly spans roughly 25 × 24.2 mm horizontally. Clamps add clearance beyond
the nominal 0.2 mm projection. Shoulder clearance flats are required; the
reference heater remains clear. Lifting stock discs much farther instead
clashes with that heater in this layout.

Source quadrature, full collision results and sourcing scope:
[pole-clearance-screening.json](pole-clearance-screening.json). The corrected
Boolean shoulder construction does not silently update the old completed
immersed-pin inputs. Field matching still requires cold-current and fluid tests
before claiming comparable jetting.

## Work still in progress

1. Wider-contact 50 µm, fixed-step flow refinement.
2. Gravity-on full-height two-pulse tests with and without the retract pulse;
   the matched 200 A / 250 µs single-pulse gravity pair is complete.
3. The shorter gravity-on 250 A / 150 µs control has completed without a
   detached component by 3.6 ms; further waveform changes are unproven.
4. The first flat-contact/compact-disc 150 A trial completes 3.6 ms without
   detachment. A near-flush short-pole trial now tests the same 150 A waveform.
5. The exploratory two-phase unified fluid trial at 200 A per coil, explicitly
   zero gravity and 200 µm resolution.

[research-notes.md](research-notes.md) preserves the earlier detailed notes and
commands as history. It contains interim statements; this README and the
completed run-status/field records take precedence for current status.
