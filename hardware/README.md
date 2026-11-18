# Solder jetter hardware study

Research branch: `research/hardware-design-20261118`. Frozen application baseline:
`simulator-baseline-2026-11-18`. Current evidence below is from 19 November 2026.
**No design has demonstrated sustained 200 Hz jetting or been qualified for
manufacture.** The target remains 0.4–0.6 mm solder drops, 24/48 V supply and
actuator currents below a few hundred amps, using accessible parts. Following
   the latest preference, roughly 30 A RMS is the working thermal target; pulse
   peaks are separate and no physical head operating envelope is qualified.

The [near-flush CAD/project bundle](designs/v6-flat-shortbar) is available for
inspection and research reproduction. Its 175 A run is the most promising
packaging case, but remains a numerical candidate.

**Motion qualification:** isolated-drop controls expose severe capillary motion
error at 100 µm spacing. Every coarse velocity/trajectory below is provisional;
a separated component is insufficient evidence of usable jetting. The 50/25 µm
600 µs translation controls improve strongly, but the 50 µm gravity case still
shows severe numerical pinning over 3 ms. Longer translation checks are running.
See [drop-flight-controls.json](drop-flight-controls.json).

The circuit-design handoff is [pulse-generator-handoff.md](pulse-generator-handoff.md):
a proposed ±300 A peak / 100 A RMS bipolar current driver on 24/48 V.

## Leading candidates and completed controls

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

The [isolated-drop flight controls](drop-flight-controls.json) are now a priority:
an initial 0.6 ms periodic-translation prefix already loses about 9% of its liquid
speed. Hardware-run departure velocities remain provisional until that numerical
behavior is understood. The zero-gravity at-rest control remains centred in its
initial prefix; gravity and translation controls continue.

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
