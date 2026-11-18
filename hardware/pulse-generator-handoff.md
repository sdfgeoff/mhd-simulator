# Metal-jetter pulse-generator design brief

Revision 1 — 19 November 2026. Design a circuit from this brief; identify trade-offs
before producing a PCB. These are proposed engineering requirements, not a tested
circuit or demonstrated droplet repetition rate.

## 1. Purpose and architecture

Drive current through two external contacts on a hot stainless solder nozzle,
inside a steady transverse permanent-magnet field. Reversing current reverses the
Lorentz force. The intended actuator makes 0.4–0.6 mm solder droplets.

Use one programmable, closed-loop, bipolar current channel. A synchronous MOSFET
full bridge with a deliberately chosen series inductor is the starting topology.
It must apply either voltage polarity with either current polarity, including
active current decay and energy recovery. A unipolar capacitor-discharge circuit
cannot reproduce the required independent push/retract waveforms.

The head is electrically floating between bridge outputs. It must not be bonded
to supply negative through its heater, thermocouple, fixture, feedstock or USB.
Document grounding and isolate control/trigger interfaces where needed. A grounded
nozzle would require an explicitly different power architecture. Keep the power
board away from the 250–300 °C head; use short, closely paired output conductors.

## 2. Electrical requirements

| Parameter | Requirement |
| --- | --- |
| Input | 24 V or 48 V nominal DC; qualify operation over 20–55 V |
| Output | One bidirectional current channel, command range −300 to +300 A |
| Normal experiments | 150–250 A positive pulse; independently set reverse pulse, initially 0–100 A |
| Repetitive thermal rating | 100 A RMS through the complete output path at 40 °C ambient, with documented forced-air/heatsink conditions |
| Repetition | Single shot and 1–500 pulse pairs/s; 1 kHz desirable only inside the same RMS/thermal envelope |
| Pulse segment duration | 20–2,000 µs; short segments may be triangular rather than reaching commanded current |
| Timing | 1 µs segment resolution and trigger-to-waveform jitter ≤1 µs |
| Rise/fall | Nominal 0↔250 A in 20 µs at 24 V with total L ≈1 µH, total hot R ≤10 mΩ; 0↔300 A in 20 µs at 48 V with the same load |
| Plateau regulation | Mean current error ≤max(2 A, 3% of command) after settling, over ±20–300 A; characterize smaller commands |
| Ripple | Target ≤max(5 A, 10% of command magnitude) peak-to-peak on settled plateaux; report bandwidth and any necessary operating derating |
| Overshoot | ≤10% on normal qualified pulse transitions; protection must independently bound fault current |
| Idle | Return to zero current after the sequence; no unintended DC through the nozzle |

Peak current, width and frequency are NOT independent simultaneous maxima.
Enforce RMS current, per-pulse I²dt and temperature limits in firmware, backed by
hardware protection. For example, two 300 A, 2 ms plateaux at 500 Hz are invalid.
100 A RMS is a circuit rating, not permission to put that heat into the nozzle.
Initially lock experimental waveforms to conservative head-specific limits.

## 3. Load and voltage compliance

1. Initial nozzle resistance is approximately 1 mΩ in the tip-contact cold model;
   design for head resistance 0.1–3 mΩ, with joints characterized separately.
   Total hot series resistance, including head, leads, switches, shunt, inductor
   and joints, should preferably stay ≤10 mΩ. Analyze 20 mΩ degraded-contact
   cases and open/short loads as well.
2. Design around total differential inductance 1 µH, and analyze 0.5–3 µH,
   including the chosen series component and actual cable loop. Show inductance
   at peak current and temperature, not merely zero-bias catalogue inductance.
   A nearly resistive head is not itself a useful current-limiting inductor.
3. Use V = IR + L dI/dt to publish an operating envelope. At 300 A, 1 µH,
   10 mΩ and a 20 µs ramp, required voltage is about 18 V. At 3 µH it is
   about 48 V, leaving insufficient margin at a nominal 48 V bus. Slower edges
   or lower current must be accepted automatically when compliance is lost.
4. The controller must distinguish failure to reach current from a successful
   pulse. Do not silently report the command as measured output.
5. Prefer a 100 V-class bridge device starting point for 48 V operation; justify
   actual ratings from worst-case supply, regeneration and measured ringing.
   The earlier 60 V MOSFET example is not a qualified choice for this bus.

## 4. Waveform control and measurement

1. Store a timed piecewise-linear current table: at least 32 segments, signed
   current commands, independent ramp/hold/zero/delay/retract settings. Permit
   multiple pulses and either polarity first. Upload tables between runs;
   generate timing locally, without USB/host timing dependence.
2. Independently vary the gap between positive and negative pulses from 0 to
   at least 2 ms, subject to sequence length. Do not hard-code the simulation's
   current 25 µs gap or one-third reverse duration into hardware.
3. Provide hardware trigger input, enable/interlock, busy, latched fault and a
   synchronized output for camera/strobe timing. Use a documented 3.3 V logic
   interface; isolate it from the switching power domain as appropriate.
4. Measure actual bipolar load current during driven, recirculating and decay
   states. A DC input-current sensor alone cannot do this. Provide a calibrated
   scope current-monitor output and voltage probe points; target ≥1 MHz analog
   measurement bandwidth and ≥2 MS/s synchronized waveform capture. Validate
   PWM transient recovery and usable measurement windows.
5. Record measured current, differential output voltage, DC bus voltage,
   temperatures, trigger time and fault/compliance status per shot. Kelvin
   voltage-sense connections at the head should allow later hot resistance and
   contact diagnostics. Include a command-versus-measurement trace export.
6. Select PWM frequency and control method from ripple, settling and loss
   calculations; do not choose an arbitrary motor-driver PWM frequency. At low
   inductance, ripple can dominate. Consider unipolar modulation, hysteretic
   control or another justified method. Check low-duty-cycle behavior and
   minimum on-time on both 24 V and 48 V.

## 5. Energy, protection and construction

1. Local low-inductance film/ceramic decoupling plus bulk capacitance are required.
   Size bulk capacitance for pulse droop, ESR, RMS ripple and regenerated energy;
   do not assume the upstream PSU can absorb reverse current. At 300 A and 3 µH,
   stored magnetic energy is 0.135 J. Provide a defined clamp/brake or storage
   path, including repeated aborts and loss of input power. A single nominal
   capacitor value or TVS label is not an energy-handling calculation.
2. Hardware overcurrent protection must work without firmware. Start with a
   nominal trip near 330 A; derive allowable threshold tolerance, detection and
   turn-off delay from maximum bus/minimum differential L and switch SOA.
   Target an absolute fault peak ≤400 A within the declared qualified load
   envelope, or explicitly revise the operating rating. At 55 V/0.5 µH,
   current can change 110 A/µs: a slow ADC loop is insufficient. A bridge
   shoot-through fault bypasses the output inductor and needs separate treatment.
3. Include gate undervoltage, enforced dead time/interlock, bus overvoltage,
   overtemperature, open-load/compliance detection, watchdog, maximum-on-time
   limit and latched safe disable. Define a controlled path for existing
   inductive current on every fault, emergency stop and unplug event.
4. Provide input fuse/disconnect, inrush/precharge and capacitor discharge with
   bus-voltage indication. Keep high pulse current out of control/USB grounds.
5. A single controller/power PCB is preferred. Bolt-on copper/busbars, an external
   inductor, capacitors, heatsink and fan are acceptable. Do not force 300 A
   through an ordinary connector or narrow PCB trace to satisfy 'single PCB'.
   Demonstrate parallel-device static and switching current sharing, Kelvin
   source routing, gate damping, copper/via temperatures and connector losses.
6. Aim to keep hot board-only resistive loss equivalent ≤2 mΩ, excluding head
   and output cable; this is a design target to verify, not an assumed result.
   Provide a complete loss budget including switching, gate drive, inductor
   copper/core, sensing and cooling. Specify required supply power from that
   budget. A 24/48 V, roughly 150 W bench source is an initial planning allowance,
   not a substitute for proving maximum continuous input requirements.

## 6. Initial commissioning waveforms

Times below are TOTAL lobe durations, including both ramps. Each lobe starts and
ends at zero. Each ramp is 20 µs; remaining lobe time is the plateau. Positive
lobe → 25 µs at zero → negative lobe. Reverse polarity is a configurable choice.

| Case | Positive lobe | Negative lobe | Repetition | Calculated RMS |
| --- | --- | --- | --- | ---: |
| A | +150 A for 250 µs | −37.5 A for 83.333 µs | 200 Hz | 31.95 A |
| B | +200 A for 250 µs | −50 A for 83.333 µs | 200 Hz | 42.60 A |
| C | +250 A for 250 µs | −62.5 A for 83.333 µs | 500 Hz | 84.20 A |

For B/C, an illustrative total R of 6.565 mΩ and L of 1.025 µH requires
11.56/14.45 V peak respectively. Estimated total resistive dissipation is
11.9 W for B at 200 Hz and 46.5 W for C at 500 Hz, excluding switching/core losses.
These are prescribed-waveform electrical calculations. C is circuit headroom;
500 Hz jetting has not been established. Start with a dummy load and reduced
current. Nozzle heating, wetting, self-field and refill can limit usable pulses
well before the circuit's maximum rating.

## 7. Acceptance evidence requested from the circuit designer

1. Schematic, BOM with current primary datasheets, PCB/source files and firmware
   interface; explicitly list any requirement not met.
2. Switch-level simulations at both input voltages over R/L bounds, including
   current ripple, dead time, gate parasitics, supply regeneration and faults.
   Provide hot/tolerance-aware current sharing, SOA, inductor saturation and
   thermal calculations. Simulation alone does not qualify the manufactured PCB.
3. Dummy-load measured waveforms for the commissioning cases, ±300 A pulses
   within RMS limits, polarity reversal and 100 A RMS thermal soak. Demonstrate
   accurate sensing during all bridge states and stable regulation over load
   bounds. Derate edge times where voltage compliance demands it.
4. Low-energy fault tests first, followed by controlled worst-case validation:
   open load, short load, missing trigger/control, PSU removal, overtemperature,
   regeneration and bounded abort current. Do not start at maximum stored energy.
5. State allowable cable geometry/length, cooling, ambient, pulse envelope and
   grounding. Include scope setup/probe bandwidth so ripple and overshoot claims
   can be independently checked.

## 8. Separate induction-coil option

Do not claim this brief qualifies an induction jetter. Existing exploratory
coils use approximately 200–250 A peak, 50 kHz carrier and 100–300 µs bursts;
windings are approximately 0.4–0.8 µH before leads, conductive loading and mutual
coupling. A 0.8 µH coil at 250 A/50 kHz already needs about 63 V reactive peak
before losses. A 48 V direct bridge cannot assume it can impose that sinusoid.
Resonant voltage gain is an option, with separate capacitor, current, insulation
and frequency-control requirements. A two-phase version needs two synchronized
channels with adjustable phase and a coupled-load analysis. Reserve a trigger/
clock expansion interface if convenient; design that power stage separately.

## 9. Basis and references

1. Research calculations: [hot driver allowance](driver-hot-allowance.json),
   [current-drive calculator](../scripts/current_drive_budget.py),
   [current physical limitations](README.md). These are model-based estimates.
2. [TI: Current Sensing in an H-Bridge](https://www.ti.com/document-viewer/lit/html/sboa174)
   supports bidirectional load-current sensing and PWM-state/transient analysis.
   It is architectural guidance, not a 300 A reference design or part selection.
3. [Nexperia: Paralleling MOSFETs](https://www.nexperia.com/applications/interactive-app-notes/IAN50005_paralleling_MOSFETs_in_high_power_applications)
   explains static/dynamic current-sharing sensitivity and gate/layout effects.
   Verify individual selected devices against their current datasheets.
