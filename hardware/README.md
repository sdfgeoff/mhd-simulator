# Solder jetter hardware study

Research branch: `research/hardware-design-20261118`. Frozen application baseline:
`simulator-baseline-2026-11-18`. Latest assessment: 20 November 2026 (NZDT). Older sections retain their dated comparison context.
**No design has demonstrated sustained 200 Hz jetting or been qualified for
manufacture.** The target remains 0.4–0.6 mm solder drops, 24/48 V supply and
actuator currents below a few hundred amps, using accessible parts. Following
   the latest preference, roughly 30 A RMS is the working thermal target; pulse
   peaks are separate and no physical head operating envelope is qualified.

The current mechanical candidate is a **short M6 nozzle with a thicker,
radiused outlet boss and heated contact fingers**. It trades a small current
increase for a 0.2 mm nominal wall and an explicit R0.1 tool radius. The earlier
near-flush V6 and thin 0.7 mm boss packages remain comparison cases. See the
[current CAD fixture](designs/rooted08-hot-contact-m6) and
[prototype measurement plan](bench-test-plan.md).

**Motion qualification:** the 25 µm gravity control reduces the earlier coarse
pinning but still has about 14% speed error over 3 ms. Slow capillary motion,
breakup and repetition remain unqualified. Completed controls are recorded in
[drop-flight-controls.json](drop-flight-controls.json); no control is still
running merely because an older paragraph below describes its initial prefix.

The circuit-design handoff is [pulse-generator-handoff.md](pulse-generator-handoff.md):
a proposed ±300 A peak / 100 A RMS bipolar current driver on 24/48 V.

## Current evidence

| Candidate | Latest accepted evidence | Decision still open |
| --- | --- | --- |
| One-piece rooted stainless M6 tip, external contacts and permanent magnets | 110 A pulse: 50 µm / 1.5 µs run completes 3.6 ms with 0.596 and 0.417 mm detached regions. Fixed-grid 100 µm timestep halving changes the leading diameter only 0.12%, but both runs retain a second region and its slow motion changes. | No clean, converged single-drop setting. The 55 A counterpulse completes but retains a dense trailing region; the completed 110 A counterpulse gives a 0.480 mm leading region and an upward-moving 0.418 mm secondary region. [110 A result](rooted-retract100-screening.json). [Reverse-current assessment](rooted-retract-screening.json). [Timestep assessment](rooted-timestep-screening.json), [field comparison](rooted-timestep-field-comparison.json), [fine result](m6-rooted-flow-refinement-screening.json). |
| Earlier ten-turn single coil | Six stationary nonnegative pulse cases complete with energy/decay checks. The 250 A, 10 µs rise / 100 µs fall cases give about 29 V interval terminal estimates. Both lower-fill0/250A flow cases complete2.1ms without a resolved detached drop. [Matched flow result](coil-low-fill-flow-comparison.json). | No single-pulse coil jetting result. The winding clashes with the nominal standard heater; coarse 200 µm flow cannot settle breakup. [Stationary comparison](unipolar-coil-fast10-v2-comparison.json), [flow plan](coil-low-fill-flow-plan.json). |
| Stainless tube and split copper slit plates | Unpowered conditioned control completes 10.002 ms. Driven 40 A case fails at 9.999 ms; its last saved 9.519 ms export has one 0.408 mm region moving down at 0.90 m/s, plus substantial lateral spreading. | Failed coarse result is provisional. End confinement, timestep, gap resolution and hot sealing remain open. [Accepted-prefix evidence](split-slit-conditioned-prefix-screening.json), [unrun end-stop CAD](designs/split-slit-endstop-review/README.md). |
| Compact twelve-turn coil below a drawing-based heater | CPU-validated CAD; nominal return projection approximately 2 mm. Unloaded 250 A / 15 µs rise estimate approximately 36 V. | No native shielding, loaded voltage, flow or temperature result. Heater inclusion tests are queued after a native decay-step reference validation. [Package](lowered-coil-package.json), [guarded test plan](lowered-coil-cold-plan.json). |

1. Current coupled comparisons use the [Sn99.3Cu0.7 research properties](solder-properties.json) at 250 °C, with thermal evolution disabled. They do not qualify Sn60Pb40 or hot contact chemistry by transfer. Density, surface tension, viscosity and conductivity remain material/temperature assumptions with the recorded source limitations.
2. Coil flow uses the normal unified `--run-check` path. The electrode-driven cases use the research `conduction_flow` client and the same `RunState` fluid/interface engine, with conductivity updated from liquid fraction and motional EMF. Electrode setup is a research CLI feature, not a GUI electrode-control feature. Its imposed permanent field omits induction/self-field reaction; the field-export postprocessing is not an additional fluid solver.
3. The [explicit current criterion](conduction-dimensional-current-screening.json) keeps strict 10 ppm unit-voltage conservation and the local residual gate. Its optional motion-only 1 µA allowance passes a completed matched baseline comparison; it is an explicitly changed numerical criterion, not a new physical current source.
4. The [free-level control](coil-fill-control-v1.json) changes the upper free-interface area by 44.2 times as well as throughflow inertia and forcing. Its smaller inertia is not evidence of better jetting. The [plateau-pulse budget](single-pulse-headroom-screening.json) identifies a possible 250 A / 30 µs hold within approximately 28.9 A RMS at assumed 200 Hz; that changed waveform has no native field or flow result and is not queued.
5. The [unpowered coil startup](coil-zero-retention-screening.json) does not reach a stationary meniscus in2.1ms: its first-positive-cell-plane liquid proxy falls to13% of its initial value, and the lowest α≥0.5 occupied plane moves inward by one200µm cell. These coarse startup controls cannot settle experimental retention or disprove the coil mechanism. Grid, gas-domain and initial-state checks remain necessary.
6. [Saved runtime measurements](current-runtime-screening.json) put the existing fine direct-contact run at about two hours of solver-stage wall time, versus about fourteen minutes for the coarse 1.5 µs case. Pressure projection accounts for 81% of the unpowered AMD coil control’s profiled host wall time. The completed powered coil case spends69% on induction solves;55% of total wall time is induction after300µs, with stored field energy below5×10⁻²⁰J. These are workload observations, not controlled speedup claims or a validated cutoff.
7. All frequency/RMS figures are arithmetic operating targets. Stable repeated breakup, feed replenishment, thermal management, wetting/oxide control and real driver behavior remain unqualified. The frozen application baseline is unchanged.

## Earlier candidates and completed controls

Completed moving-fluid results use **zero gravity** unless explicitly marked
gravity-on. The matched gravity-on tip-contact control is now complete.

| Configuration | Accepted evidence | Main unresolved issue |
| --- | --- | --- |
| Slim square outlet boss, same 0.4 mm bore and short magnets | Cold 50 µm comparison: 0.7 mm square boss gives 50% more outlet-region force per amp for 23% more resistance. | No qualified moving-fluid outcome; contact heating, hot properties and thin-wall manufacture remain open. |
| Stainless V6 approximation, 0.4 mm bore, 0.8 × 0.4 mm external tip contacts, larger SmCo poles | Gravity-on full-height 100 µm case: one 0.59993 mm component moving down at 0.1795 m/s at 3.6 ms, with 200 A / 250 µs. | 50 µm result is oversized and materially faster; refill/repetition, electrical wetting and contacts remain open. |
| Machined tip with near-flush stacked SmCo bars | Gravity-on 100 µm case at 175 A / 250 µs: leading component 0.5949 mm / 0.3547 m/s downward at 3.6 ms. | Higher-threshold returning-tail fragments; fine-grid, isolated-flight and repetition checks open. Fixed-path 50 µm thermal replay peaks about 275 °C from 250 °C at this pulse. |
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

The matched [delayed-retraction control](retract-delay-screening.json) completes
with a 500 µs zero-current gap instead of 25 µs. Its separated component moves
upward at 3.6 ms, so the delay does not establish an improved departing jet.

The [matched-waveform V6/custom cold carrier pair](v6-carrier-comparison.json)
has completed 120 µs. Over 80–120 µs at 250 A peak/50 kHz, mean downward liquid
force is 4.22 mN for V6-like versus 28.26 mN for the custom chamber. Liquid takes
7.65% versus 35.38% of calculated metal Joule power. The custom case also contains
3.1 times as much liquid in the observation region; this is not wall-only
shielding isolation. Both 120 µs resonant fits need around 20 V drive with roughly
75 V internal AC peaks, before real switching/AC losses or full-burst recovery.
Actual exported coil/material/solver/grid settings match exactly. The 0.5 mm
V6-like model is not a claim of a catalogued stainless SKU; the standard heater is
omitted because it clashes with the coil. No new ejection claim.

The independent [droplet-flight heat balance](droplet-flight-screening.json)
quantifies travel after pinch-off. A 0.5 mm SnCu sphere starting at 250 °C and
0.15 m/s in still 25 °C air loses about 13 K over 10 mm (32 ms), versus about
1 K over 0.5 mm (3 ms). This is a lumped convection/radiation sensitivity,
not native MHD thermal integration or oxide kinetics. Centre travel after release
is not the mechanical nozzle gap: breakup position and drop radius also matter.

A [30 A maintained-current startup](continuous-current-screening.json) now tests
whether the near-flush head sustains outward flow. It covers only 3.6 ms with
current still on at the end, and is not a steady-state or thermal qualification.
The [thermal qualification record](nozzle-thermal-qualification.json) separates
electronics capability from unqualified head limits. Revision 2 of the driver
handoff starts the real-head mode disarmed and reserves full-current validation
waveforms for dummy loads.

The [isolated-drop controls](drop-flight-controls.json) expose velocity-dependent
capillary errors: the 25 µm slow-translation case loses about 5.2% speed over
0.6 ms, and its gravity control remains about 14% low in speed over 3 ms.
[Read-only curvature probes](drop-curvature-diagnosis.json) show that complete
height-function coverage improves strongly with refinement. No numerical force
correction was applied to the hardware results.

## Physical scope and limits

1. **Gravity and domain boundaries.** Earlier generated flow cases use
   `acceleration_m_s2: [0, 0, 0]`. Their velocities isolate electromagnetic and
   capillary motion. The full-height zero-gravity control preserves the wider
   contact drop size within 0.03% but changes speed by 8.1% relative to the
   cropped case. It changes domain extent and periodic air return together.
   The matched gravity-on case gives 0.59993 mm / 0.1795 m/s instead of
   0.59484 mm / 0.1477 m/s without gravity. Exported settings match except uniform
   acceleration; initial states start at rest, not from settled hydrostatic/
   wetting equilibrium. The older unmodified-hex domain clips its outer
   shoulder corners. A [cold lateral-domain control](tip-lateral-domain-screening.json)
   clears those corners with identical common-cell material IDs and changes liquid
   force by 0.0018%, voltage by less than 0.0001%. This bounds that electrical
   truncation; it does not establish moving-fluid lateral-domain convergence.
   The machined near-flush collar has no nonterminal conducting boundary cells.

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

4. **Thermal replay is separate from the moving-fluid trials.** The native GPU
   heat solver now replays accepted cold Joule fields with diffusion for a single
   300 A pulse. Starting at 250 °C, the machined tip peaks around 300/322 °C on
   100/50 µm grids; the wider-contact reference peaks around 286/296 °C. The
   current path, properties and ideal external contacts stay fixed. These peaks
   are not spatially converged and do not qualify melting margin or repetition.
   [Checks, sources and results](nozzle-thermal-qualification.json).
   The earlier adiabatic estimates remain useful comparisons. Refining the wider-contact
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

## Current qualification work

1. Isolated-drop controls reveal severe coarse-grid capillary translation error.
   At 100 µm, a uniformly translating 0.6 mm drop loses 8.8% of liquid speed
   over 0.6 ms and nearly stops by 6 ms. At 50 µm, the 0.6 ms speed loss falls
   to 0.16% (0.27% at 25 µm). Centre-position error improves through all three
   grids, but the 50 µm gravity control falls only 5 µm instead of 44 µm over
   3 ms. A fourfold coarse timestep reduction does not fix translation error.
   See [drop-flight-controls.json](drop-flight-controls.json). Hardware departure
   speeds and long-time repetition trajectories remain provisional.
2. The wider-contact 50 µm zero-gravity trial completes 3.6 ms with a separated
   component, but speed differs substantially from its coarse counterpart.
3. The near-flush 175 A case gives a leading 0.595 mm component moving down
   0.355 m/s at 3.6 ms. Tail classification, fluid refinement, real contact
   heating and repetition remain open. The 150 A and delayed-retract cases
   separate components moving upward.
4. Maintained 30 A produces continued connected outflow through 3.6 ms in the
   coarse near-flush model. This is startup only; no feeding, steady flow,
   thermal evolution or qualified speed. See
   [maintained-current-startup.json](maintained-current-startup.json).
5. The gravity-on two-pulse test with retraction completes without a second
   separated component in saved frames and reaches a side boundary after
   8.1 ms. Its no-retraction counterpart is still running. The coarse two-phase
   induction trial completes without resolved ejection.

[research-notes.md](research-notes.md) preserves the earlier detailed notes and
commands as history. It contains interim statements; this README and the
completed run-status/field records take precedence for current status.

## Fixed-path thermal replay

This reuses `ThermalState`, with exact time-integrated squared-current scaling of
an accepted cold Joule field. It does not introduce a second thermal PDE solver.
For example, create a new cold output using the packaged near-flush case, then:

```sh
cargo build --offline -j2 --example conduction_probe --example thermal_replay
target/debug/examples/conduction_probe \
  hardware/designs/v6-flat-shortbar/case.mhd.json 30 COLD_OUTPUT 0
target/debug/examples/thermal_replay \
  hardware/designs/v6-flat-shortbar/case.mhd.json COLD_OUTPUT \
  300 250 180 2 THERMAL_OUTPUT false false
```

The final two optional booleans select a near-zero-diffusion verification control
and a small tip thermal window, respectively. The window maps native cells from
the full-head electrical solution and checks material IDs; it does not solve a
truncated electrical problem. Its 100 µm peak matches the full thermal-domain
result, but the insulated cuts are not validated for long-time heating. Nonzero
magnet thermal coupling is excluded: the existing magnet entries contain borrowed
thermal properties, which must be replaced before any magnet thermal analysis.
All outputs above must be new directories. Current is in amperes, lobe duration
and timestep in microseconds. The default reverse lobe is 25% for one third of the
positive duration, following a 25 µs gap. These are numerical experiments, not
qualified commands for a physical nozzle.

## Unresolved electrical contact heating

[contact-flash-screening.json](contact-flash-screening.json) separately evaluates
an ideal two-half-space analytical joint model. For a rectangular 300 A / 250 µs
pulse and 100 µΩ per joint, the added local interface rise is 11 K if power is
spread over the nominal 0.32 mm² pad, or 112 K at 10% active area. At 175 A the
corresponding rises are 3.8 K and 38 K. Resistance and active area are unmeasured
sensitivity inputs. These figures are not added to a different voxel hotspot,
not the native bipolar waveform, and not a safe operating envelope. Finite tip
geometry, thermal contact resistance, films and constriction need resolution or
measurement. This mechanism is absent from the ideal-contact native replay.

## Slimmer all-metal outlet bosses

The same near-flush head has a matched cold screen with extra outside flats on
its lower 0.6 mm. The 0.4 mm bore, pool and magnets remain unchanged. At 50 µm,
the square 0.8 / 0.7 / 0.6 mm bosses have nominal minimum walls of 0.2 / 0.15 /
0.1 mm and outlet-region force gains of 25% / 50% / 82% per amp. Their head
resistances are 1.10 / 1.12 / 1.14 mΩ versus 0.916 mΩ for the two-flat control.
These are electrical screens, not matched droplet outcomes. See
[refined comparison](tip-boss-cold-refinement-verified.json),
[intermediate boss](tip-boss07-cold-screening.json) and
[source quadrature control](tip-boss-quadrature-control.json).

Native liquid masks and applied magnetic fields match bit for bit within the
refined family. Force-per-square-root-power gains agree closely with the coarse
screen. The finer run uses source subdivision 3 to stay within the native work
budget; subdivision 3 vs 4 changes coarse outlet force by 0.4%, with identical
current/heating fields. Sharp-contact hotspot and real-contact qualification
remain open. The modified mesh simplification tolerance is 10 nm, with closed
CAD and native overlap checks; unmodified liquid/magnet triangles are preserved.

Reproduce a fresh family with e.g.:

```sh
TMPDIR="$PWD/artifacts/build-temp" uv run --offline --with manifold3d --with matplotlib \
  python scripts/make_tip_boss_cases.py NEW_CASE_DIRECTORY \
  --cell-mm .05 --magnet-subdivisions 3 \
  --only control square08 rect08x06 square06 square07
```

The output directory must be new. Run `conduction_probe` at 30 A on each emitted
project before calling the read-only comparison script; its help describes output
prefix conventions. No frozen GUI electrode feature is implied.

For separate one-shot thermal replays, selected lower amplitudes give:

| Square boss | Nominal wall | Peak current | RMS at assumed 200 Hz | Cold-resistance average head power | Model peak from 250 °C |
| --- | --- | --- | --- | --- | --- |
| 0.8 mm | 0.2 mm | 140 A | 29.8 A | 0.98 W | 263.1 °C |
| 0.7 mm | 0.15 mm | 125 A | 26.6 A | 0.80 W | 262.7 °C |
| 0.6 mm | 0.1 mm | 100 A | 21.3 A | 0.52 W | 260.2 °C |

Each uses 250 µs positive duration, 20 µs ramps and a quarter-amplitude reverse
lobe after 25 µs. These amplitudes roughly match the old two-flat 175 A cold
outlet-region force; **they are not matched droplet outcomes**. Replays use
50 µm native cells, ideal contacts, fixed properties and insulated short-time
thermal cuts. Halving the 0.7 mm replay timestep changes its maximum by less
than one exported temperature increment. None includes repeated heating or
establishes a safe physical-head pulse. See
[thermal record](tip-boss-thermal-screening.json).

A [fixed hot-conductivity sensitivity](tip-boss07-hot-conductivity-screening.json)
for the 0.7 mm boss changes steel conductivity from 1.3 to 1.1 MS/m, consistent
with an approximate reading of the manufacturer’s elevated-temperature curve.
Head resistance increases from 1.12 to 1.29 mΩ and its 125 A single-pulse replay
peak increases from 262.7 to 265.0 °C. The assumed 200 Hz average head loss becomes
0.91 W. This changes only fixed electrical conductivity; it is not a fitted
material law or temperature feedback, and other properties remain unchanged.

The [0.7 mm CAD/material bundle](designs/slim-square07) now includes the fixed
hot-conductivity sensitivity. It is a research input for inspection, with the
terminal-current client and scope explained in its README. A
[resolved-copper boundary control](tip-boss07-resolved-copper-control.json)
supplies current at the domain ends and solves the copper potential inside the
domain. The added copper resistance matches a straight-bar reference within
0.12%; total liquid force changes 0.5%. At the assumed 200 Hz / 125 A waveform,
nozzle plus modeled copper dissipates 1.60 W; estimating the remaining CAD lead
length gives 1.85 W. The earlier 0.91 W refers to the equipotential-contact
nozzle-path screen. These estimates must not double-count the lead allowance in
the earlier lumped circuit model. One-shot native thermal replay still peaks
near 265 °C; it is not a repetition or real-contact temperature limit.

[Read-only curvature probes](drop-curvature-diagnosis.json) reproduce exported
curvature exactly and preserve physical fields bit for bit. Complete
height-function coverage is 19% / 76% / 99% at 100 / 50 / 25 µm initially. The
fallback estimate warrants further work; simply trying other coordinate axes
recovers no extra cells in the 50 µm initial sphere. Tiny-domain 25 µm slow-motion
and gravity controls are progressing separately; no correction is applied.

## Relieved M6 lower bodies

The [new chamber/waist family](m6-waist-cold-screening.json) retains the M6 outer
connection and 0.4 mm outlet while changing the lower body. It uses a 45° internal
funnel and pole bottoms 0.1 mm above the exit. Chamber, waist and pole gap change
together; compare whole layouts rather than attributing all gains to one change.

| Chamber diameter | Waist wall | Pole gap | Outlet-region force at 30 A | Mean outlet By |
| --- | --- | --- | --- | --- |
| 3.2 mm | 0.4 mm | 4.2 mm | 1.50 mN | −0.284 T |
| 2.0 mm | 0.4 mm | 3.0 mm | 1.70 mN | −0.323 T |
| 1.2 mm | 0.4 mm | 2.2 mm | 1.86 mN | −0.355 T |
| 4.0 mm | 0.4 mm | 5.0 mm | 1.38 mN | −0.261 T |
| 2.0 mm | 0.2 mm | 2.6 mm | 1.78 mN | −0.339 T |
| 1.2 mm | 0.2 mm | 1.8 mm | 1.95 mN | −0.373 T |

All six electrical screens use 50 µm cells, fixed material properties and resolved
copper with outer-end current supply. Resistance remains near 2.26 mΩ inside the
modeled copper extent. The small-chamber/close-pole layout improves force per amp
31% over the new family control; its pool is smaller too. These are not pressure
or droplet predictions. The previous slim-tip layout has a different funnel and
magnet height, so it is a separate reference.

A [separate 110 A thermal screen](m6-waist-selected-thermal.json) of the 1.2 mm /
0.2 mm-wall variant gives 23.4 A RMS at an assumed 200 Hz and about 1.43 W including
the same outer-lead estimate. Its fixed-path one-shot peak is 261.5 °C from 250 °C.
No repeat heating, real joints or moving-fluid outcome is established. Wrench
features, thread details, tool radii, clamping and magnet mounts remain to design.

A [full-domain conductive-heater control](m6-heater-electrical-control.json) finds
no resolved electrical shunting effect above 0.001% in the paired static screen.
The 100 µm / source-subdivision-2 pair needs a tighter charge tolerance of 1e-6:
both close power to a few parts per million. The loose 1e-5 pair missed 0.5% power
balance and must not be interpreted. This is electrical contact screening, not a
heater thermal model. The research probe now accepts an optional charge tolerance
and locates terminal planes at the actual outer copper ends, including when the
pads are fully inside a larger domain.

The [125 A omitted-self-field diagnostic](tip-boss07-self-field-screening.json)
for the earlier 0.7 mm head gives a local self-force norm around 12% of the applied
force in the outlet region. Its external return is missing and it is not coupled
back into the flow; small net-force cancellation does not qualify breakup.

Finally, the completed 25 µm gravity control falls 39 µm instead of the dragless
44 µm over 3 ms and remains downward, with about 14% speed error. This is a marked
improvement over coarser pinning, but still not a converged jetting prediction.

## Short heated-contact concept

The [radiused 0.8 mm tip](designs/rooted08-hot-contact-m6) is the current
mechanical candidate. The following controls introduced its heated-finger layout
using the earlier 0.7 mm tip. The updated record is
[m6-rooted08-screening.json](m6-rooted08-screening.json).

1. The shorter M6 body moves the pool into the heated barrel and avoids the long,
   thin relieved section. The first pool-isothermal heat-path result does **not**
   prove the outlet remains molten: a later native stationary check finds cold
   nearby copper ends can pull the outlet below the SnCu melting point. See
   `m6-stub-lead-cooling-screening.json`.
2. `designs/hot-contact-m6` adds folded copper fingers and electrically insulated
   hot anchor plates. The 100 µm fixture gives 237 °C minimum liquid with a 250 °C
   heater and 25 °C far wire ends using a deliberately reduced effective sheet/
   interface conductivity. This requires a real clamp/contact heat-transfer check.
   Native matched controls are in `m6-hot-contact-thermal-screening.json`; they
   omit feeding, exterior losses, Joule heat and magnet temperatures.
3. The 3.4 mm pole gap and 1 mm heater gap improve nominal assembly clearance,
   at a small static-force cost. Nominal magnet projection becomes 2 mm. The
   added hot copper plates create another magnet heat path, so the earlier bare
   gap estimate is not a thermal qualification of the complete assembly.
4. The static terminal-current fixture includes both 50 mm, 1 mm² copper supply
   extensions: approximately 7.1 mΩ total at fixed hot copper conductivity. Its
   added copper changes matched coarse outlet force by only 0.1%; see
   `m6-hot-contact-current-screening.json`. No final drop or rate is qualified.

Run the research GPU analytic checks serially on this machine:

```sh
TMPDIR="$PWD/artifacts/build-temp" cargo test --offline -j2 --example conduction_probe -- --test-threads=1
```

Parallel example tests stalled locally with waiting workers and no GPU work;
the serial run now passes five checks, including the charge-conservation bottleneck control. The underlying cause was not established. The native thermal rod checks
are `target/debug/examples/thermal_conductance --check`.

## Magnet-carrier sizing and field limits

1. Native midplane Maxwell stress gives roughly 9.4 N attraction per pole group.
   Expanding the integration plane changes force by 0.6%. A chosen 20 N per-pole
   carrier load is an engineering allowance, not a certified bound. A simple
   two-end cantilever example suggests a metal carrier can keep closure small;
   actual joints, lips, thermal distortion and stand-off still need CAD/detailing.
   See `m6-magnet-carrier-load-screening.json`.
2. Source quadrature inside the magnets near their surfaces disagrees strongly
   with analytical cuboid checks, including a wrong sign at one sampled point.
   Those samples are rejected as evidence of demagnetization margin. The separate
   conditional component bound and grade-curve links are recorded in
   `m6-magnet-internal-field-screening.json`; no hot lifetime is qualified.

## Slit and single-pulse coil comparisons

1. The [slit electrical matrix](split-slit-electrical-screening.json) contains
   all 13 completed stationary comparisons after the conservation repair.
   At 30 A, the stainless chamber has about 20% less outlet-region downward
   force than alumina. This force is neither nozzle pressure nor a drop prediction.
2. The [unpowered slit diagnostics](split-slit-zero-drive-diagnostics.json)
   show sideways spreading beneath the chamber followed by meniscus retraction.
   No detached component appeared through 5.001 ms. Motion persists, and two
   cells across the gap do not establish retention, equilibrium or calibrated
   wetting. `scripts/analyze_slit_retention.py` reproduces the spatial analysis.
3. `scripts/run_slit_conditioned_pair.py` runs matched 0/40 A, 10.002 ms cases,
   with a common 5.001 ms conditioning interval and finite copper electrodes.
   These runs are queued; preparation is not an ejection result.
4. The earlier [single-coil trial](single-coil-flow-screening.json) used an AC
   burst. It does not test a single unipolar coil-current pulse. The new
   `prepare_unipolar_coil_cases.py` prepares 250 A triangular pulses with
   100/100 µs and 50/250 µs rise/fall times in the custom and V6-like chambers.
   Corrected version-2 stationary comparisons completed on an isolated AMD
   adapter; the [full four-case result](unipolar-coil-v2-comparison.json) remains
   a stationary screen with no new coil jetting result. Version-1 preparation accidentally
   retained surface tracking in stationary mode and was rejected before any
   timesteps. Its inputs and failed setup are retained separately.
5. `analyze_unipolar_coil.py` combines native reaction linkage with checked
   air inductance and explicit winding/lead assumptions. It does not infer
   voltage compliance from air inductance alone. Thermal and circuit estimates
   remain separate from moving-fluid and repetition qualification.
6. Earlier `fluid_mechanical_work_J` values included gravity. See the
   [work-ledger correction](flow-work-ledger-correction.json) before interpreting
   old comparisons with lagged EM work. New runs record those components separately.
7. The [analytical pulse screen](cylinder-pulse-screening.json) Fourier-synthesizes
   the existing closed-form conducting-cylinder solution. It isolates push/pull
   timing and wall shielding at the same prescribed external field. Instantaneous
   force identities, periodic Poynting/Joule balance and time/window controls pass.
   Its radial pressure difference is an infinite-cylinder diagnostic; it supplies
   no finite-nozzle pressure, current requirement or droplet result. Reproduce with:

   ```sh
   TMPDIR="$PWD/artifacts/build-temp" uv run --offline --with numpy --with scipy --with matplotlib \
     python scripts/cylinder_pulse_screen.py --check-refinement \
     --output hardware/cylinder-pulse-screening.json \
     --plot-path artifacts/hardware-research/diagrams/cylinder-pulse-screen-v2.png
   ```
8. [Recovery-resolution screening](recovery-resolution-screening.json) separates
   drop-scale resolution from viscous damping resolution. For the recorded SnCu
   properties and exploratory oscillation frequencies, the wall layer scale is
   roughly 7–23 µm. A 50 µm grid can therefore show a detached drop while leaving
   its recovery dynamics unresolved. Conditioning time is not equilibrium, and
   single-pulse outcomes do not establish the target repetition rate. The cited
   nozzle study assumes pinned contact lines; its fitted rates are not applied
   to these free-level and slit geometries.
9. [Independent-adapter checks](dual-adapter-screening.json) record the privately
   extracted RADV driver, verified AMD selection, three native induction tests
   and a completed 20-step case. `amd_hardware_probe` delegates to the same native
   probe body and checks its actual adapter before dispatch. One solver per
   physical device permits AMD stationary screens alongside NVIDIA flow. The
   full batch uses `run_unipolar_coil_screen.py --adapter amd` and version-2 inputs;
   `analyze_unipolar_coil.py` reads the same plan. Net throughput remains to be
   measured. The driver is process-local; system packages are unchanged.
10. The [first completed unipolar case](unipolar-coil-v2-first-result.json)
    records only one of four stationary cases: custom chamber,250A peak,
    100µs rise/100µs fall. Estimated terminal compliance is−2.6..+6.3V under
    its explicit winding/lead assumptions. This is neither switching-node
    qualification nor a moving-fluid result. Use `--available` for an explicitly
    partial analysis while the batch runs; `plot_unipolar_native.py` plots it.
11. The [free-level control](coil-fill-control-v1.json) prepares matched high/low
    fills with identical solid and coil accessor bytes. Lowering the surface
    from9mm to5.6mm removes the liquid column in the narrow feed annulus.
    A geometric plug-flow estimate gives equivalent outlet slug lengths1.128mm
    and0.188mm. It does not establish a pressure or droplet benefit: conducting
    volume, free surface and force also change. The newer lower-fill flow control is underway; see the current-evidence table.
    `prepare_coil_fill_control.py` creates fresh inputs and verifies closed
    geometry, liquid volume and unchanged solid/coil payloads. It deliberately
    refuses to overwrite an existing control folder.
12. The [completed custom pulse pair](unipolar-coil-v2-custom-pair.json) retains
    both stationary100/100µs and50/250µs results. Their rise/fall force integrals
    largely cancel; this cannot determine the response of a moving interface.
    [Shorter future screens](unipolar-coil-tail-cost.json) avoid much of the
    negligible-field tail, with an explicit final field-energy/heat gate.
13. The [lower-fill flow pair](coil-low-fill-flow-plan.json) and
    [V6-like flow pair](coil-v6-flow-plan.json) use the normal unified CLI for
    matched unpowered/driven controls. The lower-fill unpowered case is complete;
    its driven case is now complete without a resolved detached drop. The V6 pair
    has started. These are mechanism tests,
    with no single-pulse jetting result yet. The200µm startup mesh does not resolve breakup. These encircling
    coils also clash with a conventional heater block; alternate heating or
    coil placement remains a packaging requirement.
14. The [winding cooling budget](coil-winding-thermal-budget.json) separates
    RMS current from thermal qualification. It reports required cooling
    conductance under explicit temperatures and head-coupling assumptions.
    Its20°C copper reference and linear temperature sensitivity do not replace
    a hot-property curve or predict actual winding temperature. In particular,
    the earlier fixed25MS/m assumption must not be called100°C copper.
15. The [completed direct-contact refinement](m6-rooted-flow-refinement-screening.json)
    reaches3.6ms on both100µm/3µs and50µm/1.5µs grids. The fine leading
    region is0.596mm at0.889m/s downward, with a second0.417mm region moving
    slowly downward. This is ejection in the model, not clean single-drop release.
    Leading diameter changes11% and volume36% with joint refinement. The
    [follow-up controls](rooted-temporal-retract-plan.json) isolate the coarse
    timestep, then increase reverse-current magnitude with geometry held fixed.
    The [fixed-grid timestep assessment](rooted-timestep-screening.json) is now
    complete. The [27.5/55 A reversal comparison](rooted-retract-screening.json)
    also completes, with threshold-sensitive trailing liquid. The [110 A counterpulse](rooted-retract100-screening.json)
    leaves two regions across all thresholds, with the secondary moving upward;
    this does not establish recovery or clean release.
16. The [rooted-tip self-field diagnostic](m6-rooted-self-field-screening.json)
    postprocesses the earlier native cold current at110A. The local omitted
    self-force is about6% of applied-force L2 in the liquid/outlet regions.
    External current outside that electrical crop is missing, so this is not
    a complete-circuit error bound or a correction to the moving-fluid results.
17. The [lower-coil review package](designs/lowered-v6-coil-review/README.md)
    addresses the heater clash with a shorter, wider twelve-turn winding below
    a drawing-based heater. Nominal round-wire clearances are checked and both
    projects pass the actual CPU-only project loader. Heater-included/excluded
    stationary EM comparison now [completes through5.001ms](lowered-coil-cold-comparison.json).
    Adding the simplified heater reduces peak inward force moment by65.5%
    at the same250A; its loaded interval voltage is−4.31 to32.03V.
    [Assessment](lowered-coil-heater-assessment.json) records the substantial
    axial-force change and unchanged energy/decay gates. This is not a moving-fluid
    or thermal result; the nominally non-overlapping package still needs work.
    Its19mm winding diameter and roughly2mm return projection are
    geometry estimates before insulation/support/tolerances. See the
    [full package assumptions](lowered-coil-package.json).
18. The [slit conditioned prefix](split-slit-conditioned-prefix-screening.json)
    contains a complete10.002ms unpowered control and a driven40A run that
    failed its CFL guard at9.999ms. Its last accepted export,9.519ms, shows
    one detached0.408mm component at0.90m/s downward; the unpowered control
    shows none. Conditioning fields through5.001ms are byte-identical.
    However, the drive also pushes about189nL into the slot outside the bore
    footprint, including93nL beyond the tube's outside radius. End confinement,
    timestep and gap resolution remain unresolved. `plot_slit_matched_views.py`
    shows both orthogonal sections and the failed-run status.
