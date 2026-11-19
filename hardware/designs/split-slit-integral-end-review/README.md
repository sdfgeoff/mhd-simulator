# Slit with integral stainless end walls

1. This unrun concept makes the two end walls part of the stainless chamber.
   It removes the two separate mica end shims. The other nine body geometries,
   including copper plates, liquid and magnets, remain byte-identical.
2. Nominal machining is a 1 mm through-bore, a 2 mm OD upper tube and a
   3 mm basal flange. Two opposing shallow cuts leave 0.2 mm-wide end walls
   at the bore ends. The outlet is formed with the existing copper plates.
   These thin features still need machining, burr and clamping assessment.
3. Stainless end walls connect the electrodes electrically. The queued
   stationary comparison will measure that bypass against ideal mica end
   confinement before choosing a flow test. It uses 30 A, fixed properties
   and equipotential electrode bodies; extended-lead losses are excluded.
4. Open `flow-h010.mhd.json` for CAD inspection. It is a research-client
   input, not a GUI electrode-drive demonstration. Actual project loading
   passes. No field, droplet, hot seal or thermal result exists yet.
5. A single through-bore and joined analytic solids avoid sliver triangles
   at imported float seams. A 1 µm construction overlap joins the basal
   flange to the tube; initial liquid geometry is unchanged. Ideal contact,
   neutral wetting and the two-cell gap remain modeling assumptions.
6. Reproduce with `scripts/prepare_slit_integral_ends.py` in the cached
   manifold3d/matplotlib environment. `manifest.json` records hashes and limits.
