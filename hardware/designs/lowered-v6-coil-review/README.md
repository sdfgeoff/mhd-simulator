# Lower winding with a V6-like nozzle and heater

1. Open `heater-on.mhd.json` for CAD/material inspection. These are stationary
   electromagnetic inputs, not a fluid demonstration or a qualified build.
   Both projects pass the application's actual CPU-only project loader.
2. The twelve-turn winding has four layers of three turns, using nominal
   1.2mm bare copper and 1.296mm pitch. Its winding envelope is18.87mm diameter.
   It clears the assumed heater underside by0.96mm. The block starts atz5.5mm,
   assuming0.5mm assembly gap above the source nozzle's hex top. This is our
   positioning assumption, not a manufacturer tolerance stack.
3. Minimum nominal radial clearance to the nozzle's circumscribed hex is
   only0.31mm. Enamel, bobbin, hot deformation, retention and insulation still
   need space. The return plus bare-wire radius extends2.04mm below the outlet;
   final support geometry must also count toward surface stand-off.
4. The source nozzle, liquid and feedstock vertex bytes are unchanged. The
   nozzle has a3.2mm inlet,0.5mm outlet and0.9mm land. It approximates released
   V6 dimensions; a matching catalogue stainless0.5mm SKU is not established.
5. The heater uses the E3D drawing's23×16×11.5mm envelope and offset bores.
   Heater/sensor inserts, clamp slot, fasteners, thread helix and heatbreak
   are omitted. Aluminium conductivity15MS/m is an explicit sensitivity,
   not a calibrated hot-material curve. `heater-off.mhd.json` differs only
   by excluding this heater body. The [completed stationary comparison](../../lowered-coil-cold-comparison.json)
   isolates its inclusion: peak inward force moment falls65.5% at250A and the
   axial-force history changes substantially. Both work/decay checks pass.
   This simplified result does not bound shielding in an assembled heater.
6. Checked thin-wire air inductance is about1.661µH. With explicit25MS/m
   winding copper and2mΩ/0.2µH series assumptions,250A with15µs rise/100µs
   fall needs about36.2V unloaded and gives21.9A RMS at an assumed200Hz.
   With the modeled heater included, native reaction gives−4.31 to32.03V
   interval voltage. Switching overshoot, cooling and repetition remain unqualified.
   See the [assessment and physical limits](../../lowered-coil-heater-assessment.json).
7. Generation: `scripts/prepare_lower_coil_package.py`. Loading validation:
   `examples/validate_project.rs`. Circuit parameter check:
   `scripts/winding_inductance.py`. Full assumptions, file hashes and limitations
   are in `hardware/lowered-coil-package.json`.

Geometry dimensions come from [E3D's released drawing, page7 of this mirrored
datasheet](https://cdn03.plentyone.com/ioseuwg7moqp/propertyItems/4488763/V6Datasheet_2019%5B1%5D.pdf).
The source drawing carries E3D's GPLv3-or-later notice; the heater is a simplified
dimension-based model with the omissions above.
