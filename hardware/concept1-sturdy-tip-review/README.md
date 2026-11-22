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

The repaired040case has now completed CADfractioninitialization and published its initialacceptedframe; its fluidrun subsequently passed moving exports and was stopped early for the next geometry. Geometry/performance remains the priority, with no furtherdomain/accuracycomparison queued.

## First moving040frame and force location

1. At0.6ms the sturdytip has61.99nLbelowexit and0.89175m/sdownward outletflow, versus93.82nL/1.24567m/s for the earlier0.8mmtip in the sameflightdomain/pulse. All connectivitythresholds remain attached. This is an earlyresponse comparison, not the finaldrop result.
2. A read-only breakdown of savedstationaryfields finds that force in the straightoutlet falls4.856->2.548mN while taperforce rises1.986->4.093mN and upperpoolforce1.917->5.136mN. Thus the greaterintegratedforce is distributed fartherupstream; this is consistent with weaker earlyoutletmotion, though not a complete causalpressure decomposition.
3. The0.5mmbore has3.484mN in its straightoutlet in thatscreen and lower hydraulicresistance is expected. It is the preparednextgeometrycandidate, subject to current040drop/tail results. No newverificationcase is queued.

![Shared0.6ms geometry response](flow-sturdy-040-comparison-prefix-step200.png)

## Partial040 screening decision and050 launch

1. At1.8ms the040 ligament remains attached at alpha0.01/0.25/0.5/0.75. Below-exit liquid132.007nL; outlet mean velocity0.19052m/s downward, front0.95mm belowexit, no boundary contact. Stopped at1.98ms to prioritize geometry performance per human911. This partial screen does not prove no later release. Raw fields are preserved.
2. The050 repaired mesh passes zero-overlap and all coarse bodymasks match its prior stationaryscreen exactly. No repeated fieldsolve. A fresh050 fluidrun now uses the same100A400us waveform/settings; only outletgeometry differs from040. Static force alone is not a jetting claim.

![Partial040 progression through1.8ms](040-screen-prefix1800us.png)

## First050 moving-frame comparison

At0.6ms the0.5mm outlet has80.71nL belowexit versus61.99nL for0.4mm (+30.2%), with similar downward outlet velocities0.90687 versus0.89175m/s. Both remain attached at all four connectivity thresholds, with no boundary contact. This is more initial expelled volume, not evidence of a released drop. The050run continues to assess release/tail performance.

![Shared0.6ms outlet comparison](flow-sturdy-050-comparison-prefix-step200.png)
