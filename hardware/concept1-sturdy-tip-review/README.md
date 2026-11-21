# Sturdy Concept1 tip candidates

1. User911 prioritizes geometric improvements over extended verification. The flightspace comparison was stopped at its last logged530steps1.59ms, preserving partial outputs; no further verification cases are queued.
2. Both candidates widen the squaretip from0.8to1.2mm and move the copper/mica inneredges0.2mm outward. Straightland stays0.6mm. The0.4mmexit leaves0.4mmwall;0.5mmexit leaves0.35mmwall. NominalCuSScontact grows0.32->0.48mm2per side. The heater, clamps, magnets and all other mesh payloads stay unchanged.
3. CPUclosedsolid/one-pieceSS/overlap checks and both initialGPUzerooverlap checks pass. These are minimal setupchecks. Brief100Acurrent/force/Joulescreens run sequentially before selecting a fluidcandidate.
4. BroaderSSmaybypassliquidcurrent, and thelargerexit maygrowdrops/tails. Neitherdesign has a jetting result yet. Sourceproperties/contact/thermal limits from theConcept1baseline continue to apply.

![Tip sections](proposed-tip-sections.png)

## Brief100A screen results

1. Currenttip:0.21885V/21.885W/downwardliquidforce8.759mN. Sturdy0.4mm:0.19548V/19.548W/11.777mN (+34.5%force). Sturdy0.5mm:0.19425V/19.424W/12.653mN (+44.5%force). These are stationary calculations, not detached-drop results.
2. The0.4mmsturdycandidate is selected first for a100A400µs fluid experiment, retaining the previousoutletdiameter and pulse while improving the manufacturablewall/contactdimensions. The0.5mmcandidate stays ready for subsequent performance experiments. No extra verification runs are queued.

## Fluid startup mesh repair

The first040fluid attempt stopped beforetimestepping: CADsubcell integration rejected invalidorientedlengths. CPUclosedsolids and coarseGPUoccupancy hadpassed; the Booleanagainstimportedf32steel likely introduced tinyfaceartifacts. Staticgridresults remain coarsegeometryscreens, nofluidresult. The nominalshape is being rebuilt fromanalytic primitives in freshv2files; oldinputs/outputspreserved.

Analyticv2CAD preserves the nominaltip dimensions. The040repairedmesh has zeroGPUoverlap and byte-identical coarse bodymasks to the screenedv1geometry, with the same magnetic sources. The staticcurrent/force screen therefore remains applicable to this discretegrid; it was not repeated. The040fluidexperiment now uses the repairedv2input.

The repaired040case has now completed CADfractioninitialization and published its initialacceptedframe; its fluidrun is active. Geometry/performance remains the priority, with no furtherdomain/accuracycomparison queued.
