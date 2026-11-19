# Full-length replaceable stainless strip for review

1. The strip is 16 mm long, 4 mm wide at its tabs and 0.5 mm thick. Its central
   waist is 0.8 mm wide and 0.8 mm long, with a 0.4 mm hole. Two M2×3 screw/nut
   envelopes clamp it to copper contact pads. This adds usable contact length
   beyond the earlier short central-material comparison. All nominal bodies are
   closed, connected and checked for unintended intersections.
2. The upper stainless body has a flat direct-metal seat at z=0.5 mm. The modeled
   contact screws clamp strip to copper; they **do not supply the nozzle-seat
   preload**. Heater-anchor clamps, seating force, finish, leakage and assembly
   tolerances remain unresolved. This is research CAD, not a manufacturing release.
3. Pole gap grows to 4.6 mm for 4 mm wide contacts and nuts. Nominal copper-to-pole
   clearance is 0.3 mm. The old carrier assembly is not included: it needs adapting
   to these pole positions. Tool access is not qualified; contacts would need
   assembly before fitting the poles. Screw sockets and real threads are absent.
4. Native 50/100 µm comparisons give about 136/145 A to match the earlier integral
   tip's cold outlet-force metric, or 29/31 A RMS at the **assumed** 200 Hz pulse
   shape. Force changes 6.7% between these grids. Fine-grid current imbalance is
   0.030%; a tighter long-waist solve stalls in f32. These exploratory comparisons
   use an explicit 0.1% conservation criterion, rather than the earlier 0.01%
   criterion. They do not qualify drop size, pulse shape or repetition rate.
5. Shortening the waist from 2 mm to 0.8 mm reduces the cropped cold resistance
   by about 25%, with about a 2% force reduction. The shortened strip's cropped
   resistance is 6.5–6.7 mΩ. It excludes outer copper anchors/leads, real joints,
   hot conductivity and the driver; do not use it as total circuit resistance.
6. A separate stationary native thermal screen gives about 242 °C minimum liquid
   temperature for a 250 °C fixed heater, 25 °C far copper ends and effective mica
   conductivity 0.1 W/mK. It includes the strip/contact fastener heat paths but
   omits carrier contact, feeding, Joule heat, exterior losses and temperature
   feedback. It is not a repetitive operating temperature prediction.
7. A conditional simply supported strip calculation gives about 48 MPa maximum
   bending stress per newton of seat preload, and about 2 µm centre deflection/N
   using illustrative room-temperature stiffness. Thus arbitrary tightening is
   inappropriate: 10 N would give about 480 MPa in that model. Actual support,
   hot strength, contact and leakage tests are required; no torque is specified.
8. `assembly.mhd.json` is for inspection. `charge.mhd.json` and `observer.mhd.json`
   are separate research-client inputs; main GUI Simulate does not configure
   electrode drive. Native charge/field composition and the thermal analogy reuse
   the existing GPU operators. No CPU field solver is introduced.

Reproduce with `scripts/make_dogbone_strip_case.py NEW_DIRECTORY
--waist-half-length-mm .4`; run the native clients as recorded in the screening
JSON and logs, then `scripts/analyze_dogbone_strip.py` with explicit case paths and
`scripts/assess_dogbone_strip.py`. See `hardware/m6-dogbone-strip-screening.json`.

Nominal fastener dimensions: [Bossard BN 610](https://www.annapol.eu/documents/6ee345428db47b168b9ffe8bc9f44571/BN610-DE.pdf)
and [Accu M2 DIN 934](https://www.accu.co.uk/hexagon-nuts/7884-HPN-M2-A2).
These provide envelope dimensions, not a hot fastener/preload specification.
