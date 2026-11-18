# Slim square outlet: 0.7 mm prototype candidate

1. This is a research CAD/material input for inspection. The main GUI does not
   implement the terminal-current research client. Loading this project does not
   reproduce the direct-current pulse experiments by pressing Simulate.
2. Stainless outlet boss: 0.7 × 0.7 mm over the lower 0.6 mm, 0.4 mm bore and
   nominal 0.15 mm minimum wall. Two external copper contacts are 0.7 mm wide,
   0.4 mm high, at z=0.1–0.5 mm. The existing 3.2 mm inlet and shoulder flats
   remain. Threads, tool radii, tolerances, clamps and mounting are not detailed.
3. The short magnets represent two abutting 25 × 5 × 5 mm SmCo bars per pole:
   four bars total. Pole gap 4.2 mm, nominal projection below the exit 0.2 mm.
   The assumed hot remanence is 0.95 T with unit permeability. Grade, seams,
   mounts, hot load line and temperatures remain unqualified.
4. The material setup uses fixed stainless conductivity 1.1 MS/m as a rounded
   hot-property sensitivity, not a temperature law. Other properties retain
   their documented assumptions. No real Cu/steel electrical contact resistance
   is included. The source data and all model limits remain in the JSON/records.
5. At 50 µm, the static field screen is promising. A 125 A / 250 µs positive
   lobe, 20 µs ramps, 25 µs gap and −31.25 A reverse lobe for 83.3 µs is a
   **simulation candidate**, not an authorized physical-head operating point.
   It is 26.6 A RMS at an assumed 200 Hz; this is not demonstrated 200 Hz jetting.
6. With current supplied at the modeled copper ends, nozzle-plus-copper
   resistance is 2.26 mΩ and calculated average loss is 1.60 W for that waveform.
   Adding a simple series estimate for the remaining 1.2 mm per copper lead
   gives about 1.85 W. Do not add those lead losses again in a lumped driver model.
   The separate ideal-contact, fixed-path, one-shot thermal replay peaks near
   265 °C from 250 °C. Repeated heating, real joints and safe limits are unresolved.
7. Slow capillary motion in the fluid solver is under investigation. No final
   drop diameter, speed, repetition rate or continuous-flow performance is
   qualified for this geometry. This is not a manufacturing drawing.

To reproduce the cold electrical screen from the research branch, choose a new
output directory:

```sh
TMPDIR="$PWD/artifacts/build-temp" cargo build --offline -j2 --example conduction_probe
target/debug/examples/conduction_probe \
  hardware/designs/slim-square07/case.mhd.json 30 NEW_OUTPUT 0 x-ends
```

The final `x-ends` option resolves copper inside the electrical domain. Omitting it
uses equipotential contact bodies for comparison with earlier research records.
The electrical domain ends at x=±3.8 mm; the copper CAD extends to ±5 mm.

See [resolved copper](../../tip-boss07-resolved-copper-control.json),
[hot conductivity](../../tip-boss07-hot-conductivity-screening.json) and
[cold comparison](../../tip-boss07-cold-screening.json). The last record uses the
older 1.3 MS/m steel value and equipotential copper bodies; its source is explicit.
