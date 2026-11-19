# Radial heater-slot sensitivity

1. Open `heater-slotted.mhd.json` for CAD inspection. This prepared stationary
   case has no numerical result yet. The application project loader passes.
2. A 0.8 mm cut connects the smooth M6 bore to the +x outer face through the
   full heater height. The heater remains one connected part. Every nozzle,
   liquid, feedstock and coil vertex byte is unchanged from the completed
   unslotted comparison; settings differ only in geometry and triangle count.
3. The cut tests whether interrupting circumferential heater current paths
   helps. It is a proposed custom operation, not the stock clamp slot.
   [Published MRI experiments](https://doi.org/10.1002/mrm.25424) motivate testing
   current-path patterns; they do not establish an effect size for this heater,
   and simple slot arrangements can leave substantial eddy-current effects.
4. The pressure-containing nozzle is untouched. Threads, tightening strength,
   thermal conduction, insert retention and conductive bridges through the
   assembled heatbreak/fasteners need separate evaluation. This is not a
   qualified machining drawing or recommended working head.
5. Reproduction: `scripts/prepare_slotted_heater.py` in the cached
   manifold3d/matplotlib environment. Native inputs use the same 250 A
   15/100 µs single pulse and 200 µm grid as the completed heater controls.
   The cut spans four cells; spatial convergence remains unproven.
