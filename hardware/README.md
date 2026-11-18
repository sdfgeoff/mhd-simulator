# Solder jetter hardware study

Research branch: `research/hardware-design-20261118`. The frozen application
baseline is `simulator-baseline-2026-11-18`. This study has **not demonstrated a
manufacturable solder jetter or qualified repetitive droplet production**.

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
Headless checks use the same `RunState` as the GUI; there is no separate fluid
or electromagnetic solver for the study.

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
the central x plane. A moving-interface trial is running. The entire pin bodies
are ideal terminals: their full bulk voltage drop, external mount, thermal
expansion and practical feedstock routing require separate design. Tungsten
properties are explicit screening assumptions. This variant uses an 8.5 mm pole
gap and magnet centres at z=-7 mm; the conductive heater block is still omitted.

For the 10-turn, 50 kHz single-coil carrier, moving the winding centre from
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
adiabatic steel rise falls from roughly 45 K to 8 K. Its moving-fluid trial is
running; these improvements do not establish a droplet result.

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
150 A / 800 µs fluid trial is queued instead of assuming the earlier domain
validation carries over. Pin clamps, feedstock routing, real electrode contact
and bulk rod losses remain separate engineering work.
