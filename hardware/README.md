# Solder jetter hardware study

Research branch: `research/hardware-design-20261118`. The frozen application
baseline is `simulator-baseline-2026-11-18`. This study has **not demonstrated a
manufacturable solder jetter or qualified repetitive droplet production**.

**Gravity scope correction (19 November):** all earlier generated pulse and
carrier flow screens used `acceleration_m_s2: [0, 0, 0]`. Their trajectories
isolate EM and capillary dynamics; they are not gravity-complete flight or
repetition predictions. This includes the fine-grid reference, alloy controls,
compact poles and the two-pulse screen. A matched full-height, nonperiodic pair
with zero acceleration and −9.81 m/s² is running separately. Do not combine a
new gravity result with a different cropped zero-gravity domain as a matched
comparison. Initial free surfaces also start at rest, rather than from a
settled hydrostatic/wetting equilibrium.

## Current design selection

| Candidate | Accepted evidence | Unresolved before a hardware recommendation |
| --- | --- | --- |
| Stock stainless V6, opposing tip contacts, permanent magnets | Wider 0.8 × 0.4 mm contacts: one approximately 0.595 mm component at 3.6 ms on the 100 µm grid, 200 A / 250 µs. Lumped drive requirement about 11 V. | Wider-contact grid refinement and full-domain gravity/repetition controls; the completed cropped zero-G two-pulse screen does not qualify repetition. Contact fabrication, wetting and cooling remain open. |
| V6 with two milled collar flats and immersed tungsten pins | Cold 200 A model puts approximately 171 A through liquid, with 0.46 T mean liquid field. | Half-timestep full-domain trial completes4.2ms with no saved detached component; mounts, rod heat, feedstock clearance and compatibility remain unqualified. |
| Same V6, smaller single-disc poles | 250 A / 250 µs coarse trial: one approximately 0.596 mm component, downward 0.212 m/s. Nominal magnet span 14.2 mm. | Mesh/repetition, stronger omitted local self-field, attachment and wetting remain unresolved. |
| Custom chamber with one resonant coil | Native 250 A / 50 kHz EM/fluid trial completes 3.597 ms without resolved ejection on the 200 µm grid. | Matched zero-current control confirms small interface response at this coarse resolution; geometry/source refinement and actual driver remain open. Approximately 62 W electrical heat at 200 Hz in the scaled cold estimate. |

These are research candidates. The contact-driven trials use the normal fluid
and interface engine with a research GPU charge solver; they are not a new GUI
electrode feature. The single-coil trial uses the normal combined `RunState`.
Recorded free surfaces use the existing mirror/neutral wall treatment, without
measured solder/stainless advancing/receding angles or contact-line hysteresis.
Oxide skins are absent. Numerical breakup alone does not qualify a nozzle finish.

A proposed measurement sequence is in [bench-test-plan.md](bench-test-plan.md).
It distinguishes numerical evidence from the wetting, electrical and repetition
measurements still needed for a prototype decision.

## Requirements and evidence

1. [requirements.json](requirements.json) records the requested materials,
   droplet/bore sizes, fabrication preferences, drive limits and stop condition.
2. [sources.json](sources.json) records primary references, numerical values,
   and the limitations on transferring those values to this apparatus.
3. [screening-stage1.json](screening-stage1.json) records cold EM screening,
   including the refined post-pulse convergence failure. A radial force moment
   is a diagnostic; it is neither a pressure solution nor a jetting criterion.

## Reproduce the calculations

Use the study worktree. All runtime output stays under ignored `artifacts/`.
Fluid/interface and induction checks use the same engines as the GUI. The
direct-current research clients add the GPU charge solver described below;
they are not a GUI electrode feature.

1. Generate a loadable CAD/project pair, for example:

   ```sh
   python scripts/make_hardware.py --name foil-open-z6-freewheel36-h015 \
     --family foil --chamber 6 --orifice .6 --land .15 \
     --coil-length 6 --layers 4 --coil-z 6 --cell .15 \
     --current 200 --voltage 48 --drive-voltage 36 --fluid --freewheel
   ```

   `--drive-voltage 36 --voltage 48` describes an ideal 36 V pulse waveform
   supplied by a 48 V system. It needs a regulated pulse bus or a separately
   checked PWM implementation. It is not a validated driver schematic.
   `--freewheel` models resistive decay through a shorted bridge, excluding
   switch/diode dynamics. The generated assumptions file contains the estimates.

2. Run the combined solver and export accepted frames:

   ```sh
   target/debug/mhd-workbench --run-check PROJECT.mhd.json 400 \
     --output artifacts/hardware-research/NEW_RUN --field-every 80
   python scripts/hardware_flow_report.py artifacts/hardware-research/NEW_RUN --plot
   ```

   The report measures all liquid fraction below the nominal exit at z=0.
   Detached components use alpha >= 0.01, 0.25, 0.5 and 0.75 with six-neighbour connectivity; small
   components and apparent breakup require grid/time refinement. It never
   substitutes a synthetic droplet for the combined run.

   Reports also record the maximum liquid fraction on each of the six domain
   faces, any boundary contact at alpha 0.01, and relative total liquid-volume
   change. Check these for every cropped/periodic flight-domain trial; a
   detached component's own boundary flag does not cover the main pool.

3. For EM-only screening, generate without `--fluid`, then:

   ```sh
   target/debug/examples/hardware_probe PROJECT.mhd.json 80 \
     artifacts/hardware-research/NEW_EM_RUN 3 8
   ```

   The final arguments select the liquid diagnostic interval in millimetres.
   Coil reaction linkage is obtained from GPU metal currents by reciprocity.
   Its energy-balance test and observer-invariance test are in
   `src/em/coil_response_tests.rs`. The observer supports one coil with unit
   permeability and no additional field sources.

4. Independently estimate the winding's low-frequency air inductance:

   ```sh
   OPENBLAS_NUM_THREADS=1 python scripts/winding_inductance.py PROJECT.mhd.json \
     --output artifacts/hardware-research/inductance.json
   ```

   This is a lumped thin-wire energy integral, not a second field solver. The
   generated helix includes its return lead. Its regularized round-wire model
   and circular-loop asymptote check are explicit in the output. Skin,
   proximity, finite cross-section and physical lead routing remain uncertain.

## Interpretation limits

1. Cold EM runs freeze the liquid geometry. Coupled runs enable its interface,
   velocity, pressure, surface tension and EM feedback. One-pulse experiments
   currently hold thermal evolution and gravity off, at prescribed initial
   temperatures. They do not model continuous wire replenishment or oxidation.
2. The feed-neck fixture has a stationary 0.8 mm wire and a 1.2 mm bore above
   the chamber. It is a separate hypothesis from the broad, open pool. Its cold
   wire does not melt or advance in these thermal-disabled runs.
3. A 0.15 or 0.20 mm cell spacing gives only a few cells across the aperture.
   These runs rank hypotheses. Ejection, droplet size and pinch-off require
   refinement; foil thickness and annular feed clearance need particular care.
4. The V6 heater is a simplified, solid aluminium block. The Volcano screening
   instead exposes the long nozzle to the coil and assumes a different heater
   arrangement. It does not represent an unmodified Volcano assembly.
5. Conductivity, viscosity, wetting and contact-angle uncertainty remain
   material. Copper heat estimates exclude AC effects and cooling qualification.
   A successful isolated pulse would not establish 200–500 Hz operation.

## Experimental current-driven layouts

The research examples also test direct current through the metal/liquid
assembly in a transverse permanent field. These are **headless research clients**,
not a GUI electrode feature. They reuse the normal `RunState` fluid/interface
engine through `new_with_impressed_force`. Loading their CAD/material project
in the GUI does not configure their current pulse.

1. `scripts/make_conduction_hardware.py` generates a 2 mm ID, 2.5 mm OD,
   6 mm long stainless capsule with a 0.5 mm downward side orifice, a 1.2 mm
   feed neck, copper end contacts and two 10 mm magnet cubes. The default wire
   is withdrawn above the free surface. The initial immersed-wire experiment
   used `--wire-tip 1.65` and deliberately froze that wire; do not treat it as
   a replenishment/melting calculation.
2. `scripts/make_v6_conduction.py` instead uses external copper jaws on the
   V6 hex flats. It omits the normal conductive heater block. Heater and mount
   design must avoid additional current shunts.
3. `examples/conduction/solver.wgsl` solves finite-volume current continuity,
   `div J = 0`, with `J = sigma (-grad phi + u cross B0)`, harmonic face
   conductivity, current-controlled terminals and optional specific contact
   resistance in ohm m². It recomputes conductivity from accepted liquid
   fractions and includes motional voltage. The static magnet field comes
   from the existing applied-field evaluator and actual magnet CAD.
4. This is an imposed-field, inductionless approximation. Applied-current
   self-field, magnetic reaction/diffusion and magnet heating are omitted.
   Thermal evolution, oxidation, contact chemistry and wire replenishment
   remain outside these pulse trials. The research report keeps the lagged
   electrical work and actual fluid mechanical work separate.

For a cold current-sharing calculation:

```sh
cargo run --offline -j2 --example conduction_probe -- PROJECT.mhd.json 100 \
  artifacts/hardware-research/NEW_COLD_RUN 1e-9
```

For a moving-interface trial:

```sh
cargo run --offline -j2 --example conduction_flow -- PROJECT.mhd.json \
  130 350 900 artifacts/hardware-research/NEW_FLOW_RUN 0 .25 3 0 50
```

The final arguments are contact resistivity, reverse-current fraction,
timestep in microseconds, repetition frequency in hertz (0 = one pulse), and
field-export interval. The positive trapezoid includes 20 µs rise/fall ramps;
the reverse trapezoid starts 25 µs later and lasts one third of the positive
duration. A zero peak current runs the passive control. Supply switching,
lead/inductor impedance and current-controller implementation are separate
from the computed head-terminal voltage.

The four numerical GPU checks in `examples/conduction/tests.rs` cover
uniform/parallel conduction, contact resistance and motional-voltage power
balance, and accepted-potential reuse under changing phase/velocity. There is
no duplicate CPU field solver. The potential predictors must pass the same
true residual criterion as a fresh solve. A disconnected, rigidly translating
conductor also checks that a cached bridge potential does not spuriously drive
a detached droplet: the fixture measured 2.2e-9 N net force after separation.

Pressure acceleration now also attempts the existing symmetric multigrid
preconditioner after a preceding difficult solve, rather than relying only on
cell count. The fine pressure equation and physical acceptance criterion are
unchanged. A manufactured small-domain check reduced 3,456 iterations to 320
with normalized velocity difference 1.6e-6. Runtime profiling is retained in
the trial directories; concurrent desktop/GPU use limits timing comparisons.

## Source-traceable material variations and diagrams

1. `hardware/solder-properties.json` records the measured Sn60Pb40 liquid
   resistivity fit and both target alloys' Arrhenius viscosity fits, including
   units and PDF locations. `scripts/apply_solder_properties.py` creates a
   distinct trial project and a property provenance record. SnCu surface
   tension/density remain bracketed near-tin estimates.
2. `scripts/draw_hardware_layouts.py` renders sections and cutaways from the
   actual generated glTF meshes. It needs optional offline CAD/plotting tools
   (`manifold3d`, `matplotlib`); these are not runtime dependencies of the Rust
   simulator. PNGs are published as board attachments.
3. Component reports inspect multiple liquid-fraction connectivity thresholds.
   A coarse alpha-half detachment alone does not establish a converged droplet
   diameter or repeatable jetting. Flight clearance, nozzle voxel area, timestep
   and grid changes must be checked before selecting a design.

## Stage 2 checkpoint

[screening-stage2.json](screening-stage2.json) retains completed and failed trials,
local evidence paths and the limitations on coarse detachment. At this checkpoint,
the 130 A coarse cross-channel run detached a roughly 0.63 mm equivalent-volume
component, but finer runs have not confirmed that result. The 110 A, 300 µs run
at 75 µm remained attached at 3 ms. Neither is a qualified droplet design.

The V6 initializer initially rejected a small numerical excess at the common
steel/liquid conical surface. Matching the shared triangles through profile
traversal fixed initialization without changing dimensions or tolerances.
Fraction errors now report maximum total-fraction and liquid/open-ratio excess
to distinguish small raster discrepancies from substantial CAD intersections.

Initial exit-plane liquid area explains part of the grid sensitivity: the
0.5 mm cross-channel CAD aperture has area 0.19635 mm²; its initialized voxel
proxy is 0.160, 0.180 and 0.200 mm² at 100, 75 and 50 µm spacing. Thus the
coarse and finer cases do not yet present the same effective opening.

The V6 generator accepts `--magnet disc20-stack10`: two abutting 20 × 5 mm
axial SmCo discs per pole, represented as a homogeneous 10 mm stack. Catalogue
sources are in sources.json; Br=0.95 T remains an assumed hot value. A cold
180 A screen gave 0.363 T mean liquid field, 0.1006 N downward force and
9.93 W total head Joule heat. The original 10 mm cubes at 250 A gave
0.233 T, 0.0981 N and 19.16 W. This comparison is electrical only. At the
stack midpoint, the GPU value 0.3776 T differs by 0.3% from the ideal
finite-cylinder axial value 0.3787 T (near-axis cell sample).

[The V6 contact-resistance screen](v6-dc-screening.json) is a major caveat:
at 180 A, specific steel/liquid contact resistivities 1e-9, 1e-8 and 1e-7 Ωm²
reduce downward liquid force from the ideal 0.101 N to 0.077, 0.025 and
0.00325 N. Bare oxidized steel cannot be assumed electrically wetted.
Indium recommends appropriate stainless flux or nickel plating for soldering
stainless; any residue and hot contact resistance need physical qualification.

The direct-current clients release the applied-field EmState after retaining
its static field buffer. This removes unused source/reaction/checkpoint
workspaces before fluid allocation; the report records logical bytes freed.
Conduction profiling is separate from RunState profiling: cold probes write
`conduction-performance.csv`, and moving trials write `iteration-costs.csv`
with charge and fluid wall times. A tested diagonal cache was rejected: the
first layout slowed three alternating cold solves, and the packed version
showed no reliable gain above concurrent-GPU timing variation.

[The lumped V6 driver budget](v6-driver-budget.json) checks a 180 A,
800 µs pulse with assumed 1.025 µH total series inductance and 0.75 mΩ
parasitics. Required voltage is about ±9.4 V, within 24 V compliance. At
200 Hz, head/parasitic heat is 1.57/3.83 W and current RMS is 71.5 A.
The 500 Hz electrical-only case is 3.92/9.58 W and 113 A RMS. Four
parallel 3.3 µH AGP4233-332ME parts are one catalogue bank option, assuming
equal sharing and room-temperature maximum DCR. This is neither a PCB
design nor a demonstrated fluid repetition rate. PWM, protection, hot
component limits and measured assembled impedance remain unresolved.

[Contact-position screens](v6-contact-position-screening.json) compare the
hex-flat contacts with concave upper-barrel patches (`--contact-band upper`)
and small tip contacts (`--contact-band tip`). Upper contacts give 0.115 N
liquid force at 180 A with 5.95 W head heat, but remain sensitive to oxide
contact resistance. A 30 A tip-contact screen gives 16.4 A estimated liquid
current at the centre plane and 1.47 W head heat. This smaller total force is
localized near the orifice; comparing total force alone is misleading. Its
moving-interface trial is running, not an established jetting result.

For the tip variant, the same 20 mm magnet stacks move below the shoulder
(centre z=-8.2 mm) and closer together (4.2 mm pole gap). The upper pool
sees return field, so its nearly zero mean field is not an outlet-field
metric. Heater-block current shunts, microcontact fabrication and the wetting
of both internal steel and external tip surfaces still need qualification.

The original 250 A, 800 µs V6/cube-magnet case completed 4.2 ms without
a detached component at any reported alpha threshold. It spread liquid
across the tip instead. The tip-contact pulse trials use stronger pullback
variations as well as the first 30 A hypothesis.

Generator options `--fill-height`, `--domain-top-mm` and
`--air-return-periodic-z` support a partially filled, cropped-domain research
case. Periodic z reconnects the air headspace to the surrounding air; it
is not a physical repeating nozzle. Validate against a full-height case
and stop before liquid touches either periodic boundary. Cropping steel
changes current sharing, so electrical force and fluid comparisons are
required. Every new V6 project has an adjacent assumptions JSON.

A low-current Kelvin resistance comparison of an empty versus filled hot
nozzle would be useful before jetting tests. The ideal tip-contact model
predicts 1.84 versus 1.64 mΩ. Oxide contact resistance makes the filled
value approach the empty value. These are model values, not specifications;
external contact resistance, actual steel grade and temperature must be
separated from the effect of filling.

The cropped low-fill V6 check through 0.6 ms changed liquid-weighted velocity
L2 by 0.25% and alpha L2 by 0.033% relative to the full-height domain, with
matched 0.1 mm spacing and pulse. This supports provisional pulse screening
in the smaller domain; it does not certify later breakup or periodic flight.
The colder electrical crop comparison changes total liquid force by 1.69%.


### Carrier winding observations

`make_carrier_trial.py` generates a separate prescribed-current sinusoidal-burst
project from an existing winding CAD project. `fit_carrier_circuit.py` combines
its accepted GPU reaction linkage with independently supplied air inductance and
series resistance/inductance. The fitted series capacitor and drive voltage are
circuit screening estimates, not a switching implementation or droplet result.
Use explicit `--through-us` only when intentionally evaluating a partial record;
the output reports both the input duration and evaluated duration.

The optional winding observer now captures unit-current vector potential during
the first observed nonzero interval. This preserves reaction linkage through
successive zero-current tail intervals, with four bytes per edge rounded to a
64-byte block. Normal runs do not allocate this buffer. Calling the observer
only after a pulse cannot recover an uncaptured winding basis. Its GPU check
compares subsequent accepted fields byte for byte against an unobserved run,
and checks reciprocal work against Joule heat and reaction energy.


### Preliminary tip-contact and immersed-pin trials

[Stage-three records](stage3-preliminary-screening.json) retain coarse tip-contact
detachment, the driver assumptions and initial-field heat extrapolation. At
100 µm spacing, 200 A for 250 µs with a −50 A pullback separates a leading
component of approximately 0.59 mm equivalent diameter at the 0.01 liquid-fraction
connectivity threshold by 3.6 ms. A second returning component remains. The
150 A / 300 µs trial with −75 A pullback separates but has little departing
speed. These are preliminary numerical observations, pending finer-grid results;
they do not establish repeatable jetting or a measured material contact angle.

`cold_pulse_heat_budget.py` uses the accepted cold Joule field and the driver's
exact squared-current integral. The 200 A pulse gives approximately 45 K maximum
initial-field adiabatic steel rise. Heat diffusion, conductivity changes and
external contact losses are absent. Average head power alone therefore does not
qualify the small tip contacts. The liquid heat-capacity and conductivity values
are inherited screening properties, not new sourced temperature fits.

`make_v6_conduction.py --contact-band immersed` instead places two 0.5 mm
tungsten pins at x=±0.5 mm, extending from z=2 to 14 mm. With a 3.5 mm free
surface they dip 1.5 mm into the pool. The liquid CAD subtracts their volumes.
The cold ideal-contact calculation at 50 A puts about 41 A through liquid at
the central x plane. Its completed 150 A / 600 µs moving-interface trial
retracts by 4.2 ms without a detached drop. The entire pin bodies
are ideal terminals: their full bulk voltage drop, external mount, thermal
expansion and practical feedstock routing require separate design. Tungsten
properties are explicit screening assumptions. This variant uses an 8.5 mm pole
gap and magnet centres at z=-7 mm; the conductive heater block is still omitted.

In the earlier 0.5 µs cold screen of the 10-turn, 50 kHz single-coil carrier,
moving the winding centre from
z=4 to 6 mm changes the cold time-mean axial liquid force from slightly upward
to about 0.028 N downward over 80–240 µs at 200 A peak. This remains a cold
screen. The prefix circuit fit gives approximately 10.8 µF series capacitance,
16 V drive and 60 V internal AC under the stated low-frequency inductance,
resistance and lead assumptions. AC wire losses, source-core sensitivity,
complete-tail voltage and fluid response remain open checks.


### Practical electrodes, heater and pole refinement

The [dry-rod budget](electrode-rod-screening.json) separates electrode losses
from the ideal-terminal field calculation. At the trial 150 A / 600 µs pulse
pair repeated at 200 Hz, two 10.5 mm dry lengths of 0.5 mm tungsten dissipate
about 35 W under the stated resistivity assumption. Increasing diameter to
1 mm reduces this to about 8.8 W. The constant-property, equal-fixed-temperature
end model estimates approximately 49 K maximum rise for 1 mm rods; actual clamp
and liquid cooling must maintain those boundary temperatures. The very large
0.5 mm steady estimate invalidates its constant-property approximation and
makes that geometry unattractive for continuous operation.

Pin controls `--pin-diameter-mm`, `--pin-spacing-mm`, `--pin-tip-mm` reject
intersecting pins and pin tips that touch the conical nozzle wall. A 1 mm
variant uses 1.3 mm centre spacing and tips at z=2.4 mm. These dimensions fit
the **3.2 mm inlet / 2.85 mm filament V6 geometry**, not the 2 mm inlet version.
[E3D lists the separate inlet variants](https://e3d-online.com/pages/e3d-classics-support-v6-nozzle).
Actual bore dimensions and electrode clearance must be measured.

`--heater` adds the simplified conductive bulk aluminium block used in earlier
coil screening. `--heater-sigma` is an explicit hot conductivity assumption
(default 13 MS/m). Its bores, sensor, clamps and thread contact resistance are
absent. Compare electrical shunting before assuming a normal block works.
Clipping a heater across the whole fluid domain can block the external air
return: such a domain cannot be reused for fluid validation without checking
its air connectivity/boundaries.

[Tip-field records](tip-magnet-field-screening.json) sample the accepted GPU
magnetic field in a small window, with the complete magnet CAD as its source.
Their **cropped current/force outputs are not hardware predictions**. Two
10 × 5 mm discs per pole give nearly the original tip field with much smaller
pole diameter. Longer stacks increase field but source-quadrature convergence
must be checked. `--magnet disc`, `--magnet-disc-diameter-mm`,
`--magnet-stack-mm` and `--magnet-subdivisions` expose these controls. Stack
thickness must correspond to available abutting discs. The default tip pole
top remains z=1.8 mm to clear the nozzle shoulder; mounting tolerances remain
separate from nominal CAD clearance.

Carrier postprocessing now also reports the discrete reciprocal-source energy
balance and reaction backward-Euler numerical loss. In the z=6 mm, 0.5 µs cold
run the work defect is below 1e-6, but numerical loss is 7.1% of metal heat.
The first complete 80–100 µs cycle at 0.25 µs changes mean axial force by about
29% relative to 0.5 µs. Smaller timesteps are required before accepting its
force estimate. The circuit postprocessor also passes an independent analytic
unloaded R/L/C fit check; that does not validate the loaded field discretization.


### Common timestep changes and screening limits

`RunState::set_timestep(volume, dt)` changes the accepted run's common clock,
EM configuration and fluid GPU parameters together. It retains fields and warm
starts, rejects invalid changes, and preserves each subsystem's ordinary advance
stability checks. Headless checks cover analytic uniform acceleration, invalid
change rollback, coupled EM/fluid clocks and the existing EM timestep predictor.

The research `conduction_flow` client accepts an optional final `TAIL_DT_US`
after `FIELD_EVERY`. It applies only after both parts of a single electrical
pulse have ended. Repeated pulses currently require the original fixed timestep.
CSV rows record their actual timestep and run metadata records the transition.

A 3 → 6 µs tail reduces the coarse 3.6 ms V6 trial from 1200 to 660 accepted
steps. Its leading drop diameter and mean speed change by less than 1%, and
leading-region velocity L2 by 2.7%, but the returning tail differs substantially:
whole-liquid velocity L2 differs by 34.5% and alpha L2 by 1.4%. This supports
only provisional leading-drop screening, not equivalent resolved dynamics or
satellite prediction. Fixed-step runs remain the numerical reference. No default
simulation timestep or ongoing run was changed.

The 0.25 µs carrier trial completes its drive and reaches 367.75 µs, then fails
the relative-only field check: 1.040e-4 versus requested 1e-4 after 4000 iterations.
Accepted reaction energy has already fallen below 9e-22 J. Its fitted voltages
cover the accepted prefix, not the requested 500 µs. The energy defect is below
1e-7 and backward-Euler numerical loss falls from 7.1% to 3.6% of metal heat.
That failure remains recorded; no convergence tolerance or field cutoff was
introduced to label the whole trial successful.

### Larger tip contacts and current-lead heat leakage

[Larger contact records](larger-tip-contact-screening.json) use 0.8 × 0.8 mm
copper pad boxes centred at z=0.5 mm. At 30 A the cold model gives about
0.70 mΩ head resistance and 3.11 mN downward liquid force, versus 1.64 mΩ
and 2.65 mN for the smaller pads. The 200 A pulse's maximum initial-field
adiabatic steel rise falls from roughly 45 K to 8 K. Its completed 3.6 ms moving-fluid trial does not detach a drop; these cold
improvements do not establish useful outlet forcing.

[Current-lead thermal records](current-lead-thermal-screening.json) illustrate
why short thick copper arms must not be connected straight to a cold driver
without considering heater load. Under explicit constant-property assumptions,
two 4 mm lengths of 0.64 mm² copper connecting 250 to 25 °C conduct about
28 W away from the head after distributed Joule heating is included. Extending
the path to 20 mm reduces head heat loss to about 3.4 W, while increasing pair
electrical loss to about 4.5 W for the trial waveform at 200 Hz. This is a
steady 1D scale estimate, not a mount or cooling qualification. The final driver
budget must use the longer assembled leads' resistance and inductance.


The normal headless application also accepts an explicit common-timestep schedule:
`--run-check PROJECT STEPS --output NEW_DIR --field-every N
--tail-after-us TIME --tail-dt-us DT`. Both scheduling flags are required together.
It advances every enabled subsystem through the same `RunState`, writes native
fields and a durable `run-status.json` including the requested schedule and actual
accepted time, and retains ordinary solver failures. The schedule is a numerical
choice that must be checked; it does not freeze an EM frame. A short headless
moving-interface smoke run verifies scheduled time and native output. No GUI
setting or default project timestep changed.


### Separating contact width and height

[Matched-domain records](contact-width-height-screening.json) compare the original
0.4 × 0.4 mm contact boxes, 0.8 × 0.4 mm wider contacts, and 0.8 × 0.8 mm
wider/taller contacts. All cold controls now use the same low-fill compact domain.
The taller contacts reduce heating but move more of the forcing upstream. Their
200 A / 250 µs fluid trial retracts across the tip without detachment at 3.6 ms.
Widening alone retains approximately 95% of the original outlet-land axial
force/area diagnostic and reduces maximum initial-field steel heating to about
20 K. Its coarse fluid trial separates one roughly 0.595 mm component at alpha
0.01, travelling downward about 0.16 m/s at 3.6 ms. This still needs refinement.
`plot_tip_contact_study.py` renders the accepted fields and heat extrapolation;
it does not infer solved outlet pressure from total liquid force.

### Milled-collar electrode candidate

[Milled-collar records](milled-collar-screening.json) retain a 3.2 mm bore and
M6 barrel while milling two Y flats 4 mm apart on the z=2..5 mm outer collar.
Ten-millimetre pole blocks can then approach to a 4.4 mm gap, ending at z=4.8 mm
below the thread shoulder. The ideal cold calculation at 200 A puts about
171 A through liquid at the central X plane, with approximately 0.46 T mean
liquid field and 0.051 N downward liquid force. This is not a jetting result.

The nozzle/poles/pins/pool were checked as disjoint, closed positive-volume CAD.
Nominal pole/flat clearance is 0.2 mm; actual fabrication and mounting tolerance
remain unqualified. The small-domain force differs by 7.5%, so a full-domain
150 A / 800 µs fluid trial is running instead of assuming the earlier domain
validation carries over. Pin clamps, feedstock routing, real electrode contact
and bulk rod losses remain separate engineering work.


### Driver and bus-energy comparison

[Driver comparison records](driver-comparison-screening.json) include explicit
longer copper lead and tungsten rod assumptions. The wider-contact 200 A pulse
has an approximately 11.3 V peak lumped requirement from a 24 V bus, with
about 1.9 W in the head and 7.7 W in the specified leads/series path at 200 Hz.
The milled-electrode 150 A / 800 µs waveform gives approximately 8.3–8.8 V,
with about 15–26 W total electrical dissipation depending on assumed dry rod
length. These are constant-property impedance budgets, not a PCB or cooling
qualification; the modeled ideal-terminal pin length is not a completed mount.

The 250 A resonant-coil estimate scales an accepted fixed-geometry 200 A
record. It gives approximately 0.32 J peak cumulative interval drive work and
62 W mean electrical heat if repeated at 200 Hz. Without supply assistance,
a 1 mF/24 V bulk capacitor lacks enough stored energy; 4.7 mF/24 V falls to
about 21.0 V in the ideal budget, and 1 mF/48 V to about 40.8 V. ESR, ripple,
switching/control, AC winding loss and temperature-dependent load are absent.
The fit covers accepted data through 367.75 µs, not a successful 500 µs trial.
Electrical pulse budgets are not jetting efficiencies or repetition proofs.

On the same 80–100 µs carrier cycle, reducing timestep 0.25 → 0.125 µs changes
mean axial liquid force by approximately 4.7% and the radial diagnostic by
1.0%. Reaction numerical loss relative to heat is 7.1%, 3.5%, 1.8% for
0.5, 0.25, 0.125 µs respectively. Spatial/source-core refinement remains separate.

The fixed-step contact check at 100 µm, 3 → 1.5 µs changes the leading alpha
0.01 component diameter from 0.59147 to 0.59130 mm and its speed by less than
1%. Whole-liquid velocity L2 still differs by 15%, mainly around the returning
tail; alpha L2 differs by 0.77%. The 50 µm reference has detached a leading
approximately 0.618 mm alpha 0.01 component at 2.4 ms, moving about 0.51 m/s.
This is slightly above the target size and not a completed grid-convergence
study. Shorter-pulse tuning and the wider-contact refinement are queued.

[The completed 50 µm reference](tip-grid-screening.json) reaches 3.6 ms with
one detached component: 123.727 nL, approximately 0.61824 mm at alpha 0.01,
moving downward 0.50386 m/s. Alpha 0.5 gives 0.60613 mm. No liquid reaches
a domain boundary; maximum relative total-volume change over saved frames is
below 4e-8. Refinement increases leading diameter by 4.5% and departing speed
by 74% relative to the coarse values. Speed and pinch-off timing are not
mesh-converged. Initial aperture proxies are 0.120 versus 0.130 mm², around the
0.12566 mm² CAD circle; this is one geometry contribution to the grid effect,
not a complete explanation. The wider-contact 50 µm run has now started.

### Omitted applied-current self-field

[Self-field diagnostics](conduction-self-field-screening.json) postprocess the
accepted cold GPU current density using the free-space Biot–Savart integral.
`scripts/conduction_self_field.py` uses zero-padded linear FFT convolution and
uniform source-cube subquadrature within four cells. A finite straight-wire
check agrees within 6e-6 relative error, and a direct discrete sum checks FFT
indexing. Four-to-eight subcell refinement barely changes the wider-contact
result; that is quadrature agreement, not a mesh or external-circuit check.
The continuous-current integral is given in
[Fitzpatrick's electromagnetism notes](https://farside.ph.utexas.edu/teaching/em/lectures/node39.html).

For wider tip contacts at 200 A, the outlet-land self-field RMS is approximately
0.036 T versus 0.374 T applied. The self-force L2 is about 10% of the applied
force L2 there, with a net upward correction about 9% of the applied downward
land force. These are potentially relevant corrections near breakup, not proof
that the imposed-field droplet result survives unchanged. For the milled-pin
candidate at 150 A the same land diagnostic is approximately 0.4%, and the
whole-liquid force L2 ratio approximately 1.1%. External return leads are absent
from both estimates; their routing needs checking. Neither correction has been
fed back into the recorded fluid trajectories.

### Sn60Pb40 material control

[The Sn60Pb40 control](snpb-tip-screening.json) uses its separate sourced liquid
property fit in the original 0.4 × 0.4 mm tip-contact geometry. At 200 A / 250 µs
with a 25% reverse pulse, the 100 µm trial completes 3.6 ms with one detached
component: approximately 0.600 mm at alpha 0.01 or 0.549 mm at alpha 0.5, moving
downward about 0.14 m/s. It has no domain-boundary contact. This is a coarse
single-pulse control, not confirmation that the wider-contact geometry or
repeated operation works with both alloys.

The matching wider-contact Sn60Pb40 control also completes 3.6 ms, but its
approximately 0.618 mm alpha 0.01 component moves downward only about
0.012 m/s. Separation with almost no departing speed is not a robust jetting
result. Two material-specific coarse checks, 200 A / 230 µs and 240 A /
200 µs with 15% reverse current, now both complete 3.6 ms without detachment
at the reported thresholds. These endpoints do not establish later-time
behaviour. The smaller-contact Sn60Pb40 control currently departs more clearly.

[Both unmodified immersed-pin controls](immersed-pin-flow-screening.json),
using 0.5 and 1 mm electrodes with the wider pole spacing, complete 4.2 ms
and retract without detached components. This does not establish the outcome
of the closer-pole, milled-collar variant.

[The custom cross-channel 50 µm control](cross-fine-screening.json), at
110 A / 300 µs, completes 3.5 ms without detachment. Its roughly 175 nL below
the tip is still connected. The earlier coarse detached result uses 130 A /
350 µs, so this is not a matched-waveform grid refutation or a general
rejection of that design family.

### Contact and heater controls

[The wider-contact interface screen](tip-contact-resistance-screening.json)
retains the same CAD and grid. Specific steel/liquid resistivities 1e-10,
1e-9 and 1e-8 Ωm² reduce cold downward liquid force by approximately 8%,
43% and 86%. These are sensitivity inputs, not measured resistivities.
This parameter acts on the liquid/solid interface; copper lead/nozzle joint
resistance is separate and still absent. A dry clamp, braze or weld must be
qualified electrically and thermally rather than assumed equivalent to the
ideal terminal CAD.

[Matched full-head heater controls](tip-heater-screening.json), at 200 µm
spacing, change downward liquid force by only about 0.03% when the simplified
13 MS/m aluminium block is included. That supports retaining a conventional
heater as a candidate for localized tip contacts, subject to finer geometry
and actual thread contact. It does not qualify heater shunting for the other
contact positions.

[An exploratory wall-conductivity screen](tip-wall-conductivity-screening.json)
at 0.8, 1.3 and 1.8 MS/m changes wider-contact downward force by about +12%
and −9% around the nominal value. Those are sensitivity inputs, not a sourced
alloy range. [E3D's support page](https://e3d-online.com/pages/e3d-classics-support-v6-nozzle)
does not identify a supplier nozzle as the exact 316 material assumed here.
Actual grade, hot resistance and magnetic behaviour need checking; a hardened
ferromagnetic nozzle is not represented by this unit-permeability model.

The full-domain milled-pin trial at 3 µs stops at 2.154 ms on the conservative
interface CFL guard. Its saved 2.1 ms frame is still attached; gas is drawn
down between the electrodes while liquid rises outside them. The maximum
saved speed is approximately 4.4 m/s in a nearly gas-only cell, versus roughly
1 m/s in liquid. A fixed 1.5 µs rerun is in progress. This is an incomplete
trajectory, not a completed failed-jet result. `hardware_flow_report.py --plot
--pool-view` includes the free pool and native impermeable solid mask, rather
than hiding this circulation in a tip-only crop.

### Smaller stock-size magnet package

[The compact-pole screen](compact-tip-pole-screening.json) uses one
[10 × 5 mm SmCo disc](https://aussiemagnets.com.au/products/10mm-x-5mm-disc-samarium-cobalt)
per pole with the same 4.2 mm gap and wider contacts. The nominal magnet
assembly shrinks from 24.2 to 14.2 mm across the poles; heater and mounting
dimensions are separate. At equal current, full-domain cold liquid force is
about 80% of the larger stacks' result. A local 50 µm sampling window gives
approximately 0.312 T mean By magnitude, changing about 0.75% with source
subdivision 2 → 4. The full-domain subdivision-4 evaluation exceeded the direct
source work budget. The window's cropped electrical outputs are not hardware
predictions or whole-volume force convergence.

A 250 A / 250 µs coarse fluid trial completes 3.6 ms with one detached
approximately 0.596 mm alpha 0.01 component moving downward 0.212 m/s.
It has no boundary contact; this is not a mesh or repeated-pulse qualification.
A shorter 230 µs comparison also completes, but its component is almost stalled
at 3.6 ms. Under the same driver assumptions,
the lumped peak requirement is approximately 14.1 V from 24 V, with about
15 W head/series electrical heat at 200 Hz. Initial-field tip heating scales
up by 56%. Actual hot magnet grade, mounting temperature, wetting and contacts
remain unqualified; smaller packaging trades against current and heating.
Its cold self-field diagnostic gives an outlet-land force L2 correction about
15%, larger than the larger-pole 200 A case. This remains absent from the
recorded trajectory and excludes external return leads.

### Feed and melting demand

[Feedstock budgets](feedstock-screening.json) conserve mass between spherical
liquid drops and solid 0.8 mm wire, using separate liquid/solid densities.
At 200 Hz, 0.5 mm drops require approximately 25 mm/s wire feed, 5.5 g/min
and 10 W feed enthalpy from 25 to 250 °C. At 0.6 mm these become 43 mm/s,
9.5 g/min and 17 W; 400 Hz doubles all three rates. The nominal solid density
and melting point follow [Feinhütte's alloy listing](https://www.feinhuette.de/en/metals/sn993cu07).
Constant cp=214 J/kg/K and fusion heat=59 kJ/kg remain screening inputs,
without a measured alloy enthalpy curve. Actual heater demand also includes
losses and any retained electrical dissipation. These figures qualify neither
a heater nor a wire-feed/replenishment mechanism; ordinary flux-cored wire is
not the solid-wire assumption used here.

### Analytical limit for induction near a small bore

[The cylinder screen](cylinder-induction-screening.json) is a closed-form
magnetic-diffusion limit, not another implementation of the 3D solver. It
matches B and tangential E across a conducting liquid core and cylindrical
wall, using modified Bessel functions. The underlying axial diffusion problem
is described in [MIT's field diffusion chapter](https://web.mit.edu/6.013_book/www/chapter10/10.3.html).
Homogeneous-layer and low-frequency Faraday checks pass; integrated Joule
heat agrees with inward Poynting flux within 2e-14 relative error.

At an imposed 0.5 T peak field and a 0.4 mm liquid diameter, all tested walls
give only about 1.9 Pa cycle-mean radial pressure scale at 50 kHz. A 0.5 mm
stainless radial wall peaks around 0.63 kPa over the sampled 1 kHz–10 MHz range;
an ideal 0.05 mm wall reaches about 11 kPa at 5 MHz. The larger values at high
frequency do not include a feasible coil/driver or finite nozzle geometry.
Most local Joule heat in the thick-wall cases goes into the wall. These are
axis-to-wall radial pressure diagnostics, not solved outlet pressures or jetting
thresholds. They neither reject every stock-nozzle coil nor validate a MHz
microcoil; they explain why liquid radius, wall shielding and frequency need
consideration together. Radial wall thickness here is not foil exit-land length.

### Two-coil phase-direction hypothesis

[Two phased sources](two-phase-coil-screening.json) are queued for cold screens
at ±90° relative phase, 200 A per phase and 50 kHz. Both six-turn coils are
stored as centreline curves in the main glTF; the chamber bodies and material
properties are unchanged. This uses the existing 3D EM engine. A headless smoke
run passes after disabling an inherited free-surface flag for the EM-only case.
Original body vertex payloads and the generator's default single-coil winding
coordinates are checked unchanged.

The experiment is motivated by
[two-phase liquid-metal induction studies](https://arxiv.org/abs/1212.2105), not
a claim that their pumping performance transfers here. The first coil centres
are z=2.5 and 8 mm. The main 6 mm chamber ends at z=6 mm, then narrows to a
1.2 mm feed neck around 0.8 mm wire; this upper-coil placement is not optimized.
Cold force direction and heat come first. Loaded per-coil voltage, mutual
circuit, driver, cooling and actual droplet production remain open. The
single-coil reaction observer does not provide per-coil voltages for this case.

`animate_hardware_flow.py` can render accepted tip slices as a GIF. Each image
is a saved native frame, with its actual simulation timestamp; no synthetic
intermediate droplet motion is added. The original 50 µm reference gives 25
frames from 0 to 3.6 ms, with a visible playback-time disclaimer.

### Completed native single-coil trial and pulse endpoints

[The native single-coil trial](single-coil-flow-screening.json) completes 3500
accepted steps, reaching 3.597125 ms. EM and moving fluid/interface remain
coupled through the common clock; thermal evolution is disabled. No resolved
liquid crosses below the exit in saved frames. Signed alpha-volume roundoff
below 1e-7 nL is not a negative physical volume. The 200 µm grid gives only
2.5 cells across the 0.5 mm hole, so this is not a converged rejection of
single-coil jetting. A matched zero-current control is running.

Its actual custom chamber has 6 mm ID, 7 mm OD, a 0.15 mm exit land, and a
large reservoir ending at z=6 mm. A 1.2 mm feed neck above contains 0.8 mm wire,
with the free level at z=9 mm. The 0.15 mm exit land is not its radial wall
thickness. This distinction matters when comparing to the cylinder limit.

[Pulse endpoint comparisons](tip-pulse-screening.json) show why simply
shortening the push is insufficient: with larger poles at 200 A / 230 µs,
the roughly 0.605 mm component moves upward 0.0715 m/s at 3.6 ms. With compact
poles at 250 A / 230 µs, the roughly 0.599 mm component moves downward only
0.0076 m/s. These observations are not claims of inevitable recontact or
failed free flight; they are weaker departing trajectories than the 250 µs
controls and need repetition checks.


## Two-phase induction direction control

The first two-coil geometry retains the single-coil chamber, with six-turn
windings centred at z=2.5 and 8 mm. The upper winding is mostly over the narrow
feed neck; this is an unoptimized placement. Both carry prescribed 200 A peak,
50 kHz currents with a 90° phase difference, a 300 µs burst and 40 µs ramps.

Both cold phase directions complete 100 µs at dt=0.125 µs. Trapezoidal averages
of the same 80–100 µs carrier cycle give axial liquid forces of −0.058974 N
and +0.096376 N. Their phase-odd half-difference is −0.077675 N, with a
+0.018701 N phase-even bias. Total liquid-plus-solid Joule power is about
485.52 W in both directions during that cycle. This is an early-cycle cold
comparison, without moving liquid, per-coil loaded voltage or jetting proof.
The positive-phase half-timestep check now completes: 0.125 → 0.0625 µs changes mean axial force by 0.93%, liquid heat by 1.17% and solid heat by 0.59% over that cycle. This is one temporal halving, not spatial convergence or a fully converged phase-odd difference.

```sh
python scripts/compare_phase_screens.py \
  artifacts/hardware-research/foil-two-phase-plus90-200A-cold100-h020 \
  artifacts/hardware-research/foil-two-phase-minus90-200A-cold100-h020 \
  --output artifacts/hardware-research/two-phase-cold-direction-comparison.json
```

Full comparison and source hashes: [two-phase-coil-screening.json](two-phase-coil-screening.json).
Gravity correction and matched cases: [gravity-controls.json](gravity-controls.json).


## Repetition and driver sensitivity update

The wider-contact two-pulse screen completes 9.999 ms at a prescribed 200 Hz,
with zero gravity. It does **not** establish 200 Hz jetting: no second resolved
detached component appears in saved frames, the first component later turns
upward, and side-boundary liquid reaches alpha 0.01 by the saved 7.2 ms frame.
[tip-repetition-screening.json](tip-repetition-screening.json) retains the full
trajectory, boundary diagnostics and completed status. Earlier single-pulse
frames have no boundary contact; extended domain controls are separate.

A larger hot-driver allowance increases non-head resistance from 4.25 to
5.5 mΩ, including 3.5 mΩ of narrow copper leads. At the same waveforms, required
peak voltage becomes about **11.56 V at 200 A** and **14.45 V at 250 A**. Head
plus series heat becomes **11.9 and 18.6 W**, respectively, at an electrically
assumed 200 Hz. Switching/gate losses, contact chemistry and actual repetition
remain excluded. This sensitivity preserves 24 V lumped voltage headroom.

A single stock 0.68 µH [Vishay IHXL1500VZEBR68M51](https://www.vishay.com/docs/34561/ihxl-1500vz-51.pdf)
is a more compact alternative to the earlier four-part bank. It is a roughly
38 × 38 × 22 mm PCB component, separate from the nozzle. Its differential
inductance, temperature and PWM losses still require checking. The switch
example uses [Infineon IPT007N06N](https://www.infineon.com/assets/row/public/documents/24/49/infineon-ipt007n06n-datasheet-en.pdf)
60 V MOSFETs on a 24 V bus; it is not a qualified bridge design. Assumptions,
component facts and calculation inputs: [driver-hot-allowance.json](driver-hot-allowance.json).


The actual two-coil air-core matrix gives about **0.388 µH self** and
**0.0974 µH mutual**, coupling coefficient **0.251**. Halving filament segment
length changes entries by less than 0.02%; reciprocity and a distant-loop dipole
limit pass. Cross-section, AC current distribution and induced-metal loading
remain outside this integral. At prescribed quadrature currents, the air mutual
term alone transfers about **612 W between phases during the steady envelope**,
with opposite signs. That is internal exchange, not total loaded consumption.
Any eventual driver must account for coupling and bidirectional phase power;
two independently tuned uncoupled-coil estimates are insufficient.

```sh
OPENBLAS_NUM_THREADS=1 python scripts/winding_matrix.py \
  artifacts/hardware-research/cases/foil-two-phase-plus90-200A-h020.mhd.json \
  --output artifacts/hardware-research/NEW_TWO_COIL_AIR_MATRIX.json
```


The milled-collar immersed-pin trial now completes **4.2 ms** at fixed **1.5 µs**,
beyond the earlier 3 µs trial's CFL stop at 2.154 ms. It has **no detached
component** at any reported threshold in saved frames. About 177 nL lies below
the tip at the final frame but remains connected; the exit-plane mean liquid
velocity points upward. No saved liquid reaches the domain boundary, and
relative volume drift stays below 2×10⁻⁷. This is a completed zero-gravity
negative for that waveform, not a rejection of all immersed-electrode designs.
See [milled-pin-flow-screening.json](milled-pin-flow-screening.json).


## Contact hotspot refinement

At the same 200 A / 250 µs waveform, refining the wider-contact cold field from
100 to 50 µm raises the initial-field adiabatic steel maximum from **20.1 to
34.7 K**, while steel pulse energy decreases 4.5%, terminal resistance decreases
4.4% and cold axial liquid force increases 1.1%. Local current crowding and voxel
contact edges are therefore much less resolved than the integrated quantities.
The same stationary contact-current scaling at 250 A gives **54.2 K** for the
compact-pole variant; that is an I² extrapolation, not a solved temperature.

These figures exclude thermal diffusion, temperature-dependent conductivity,
actual contact-edge radius and joint losses. The 20 K coarse figure must not
be treated as a converged hardware hotspot. Full inputs, hashes and cold-field
budgets: [tip-heat-grid-screening.json](tip-heat-grid-screening.json).


## Single-coil zero-current control

The zero-current case also completes the same 3500 accepted steps through
3.597125 ms. Exported geometry, materials, effective fluid/run settings and
static masks match the driven case; only winding current tables differ. Neither
has resolved liquid below the exit. The driven case has measurable internal
liquid motion: final liquid-weighted RMS speed is **0.0131 m/s**, versus
**0.00180 m/s** without current. Final alpha L1 phase-volume difference is
**14.3 nL**, against a 168,352 nL pool; this is an absolute interface-difference
diagnostic, not ejected volume or lost mass. The native 200 µm interface response
is small. It remains a coarse negative for this geometry/waveform, not a general
induction-jetting rejection. Comparison and field hashes are retained in
[single-coil-flow-screening.json](single-coil-flow-screening.json).
