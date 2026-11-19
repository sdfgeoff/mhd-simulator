# Simplified actuator families

Concept diagrams requested in board topic 19, reply 780. Numbers match the six approaches in reply 778. These illustrate intended mechanisms, not simulated outcomes, machining drawings or proposed operating currents. No experiment was launched to make them.

1. [Single-pulse induction and parallel-return repulsion](mechanisms-1.png).
2. [Separate metering chamber and transformer-coupled liquid loop](mechanisms-2.png).
3. [Travelling-field pump and electrohydrodynamic extraction](mechanisms-3.png).

Blue is liquid; green is current; purple is magnetic or electric field as labelled; red is intended liquid force. The circled dot means a field pointing out of the page. A shown droplet indicates the intended outlet direction, not demonstrated jetting. Refill ports, walls, seals, magnetic return paths and electrical connections are simplified. SVG versions are provided beside each PNG.

The transformer diagram uses symbolic flux linkage: the box is not a literal core construction. Only the lower loop leg is in the static bias field, avoiding equal opposing forces on the return. The drawing shows one instantaneous induced-current direction; reversal during pulse reset requires consideration. This remains an unqualified proposal for our solder head.

The separate chamber is an architecture that can use several actuation mechanisms. It does not imply a valve or pneumatic supply. Its refill restriction must balance firing backflow against replenishment.

The parallel-return diagram is rotated to put liquid below the copper return and direct repulsion downward. Its primary precedent is [Pan, Hanson and Lee, 2010](https://library.imaging.org/jist/articles/54/1/art00006). The construction, temperatures and droplet sizes in that work do not qualify our adaptation.

Other background: [single-pulse MHD jetting](https://doi.org/10.1038/s44334-025-00029-w), [cored induction pump experiment](https://doi.org/10.1016/j.anucene.2011.01.008), [throttle-fed electromagnetic cavity](https://www.sciencedirect.com/science/article/abs/pii/S0042207X18309576), [EHD drop-on-demand experiment](https://www.sciencedirect.com/science/article/pii/S1526612524000392).

Regenerate the diagrams from the repository root:

```sh
TMPDIR="$PWD/artifacts/build-temp" uv run --offline --with matplotlib python scripts/draw_actuator_families.py
```

## Selected completed coil run

![Actual CAD, accepted fluid sections and pulse history](selected-coil-run.png)

This lower-fill custom chamber showed the clearest final phase redistribution among the three matched single-pulse coil comparisons. That selection does not establish the best possible coil geometry or successful jetting. Neither driven nor unpowered control produced a detached region. Some motion also occurs in the unpowered startup; the pictured speed is total simulated speed, not the difference caused by the coil.

The CAD comes from the exact geometry referenced by `artifacts/hardware-research/cases/coil-low-fill-flow-v1/low-fill-250A-flow-h020.mhd.json`. Fluid panels use that run's accepted steps 300 and 1800 (150 µs and 2.1 ms). The pulse ends at 110 µs; no full fluid export was saved during the active pulse. The chart shows the prescribed current table and computed whole-domain maximum magnetic field from the same run's diagnostics. It does not substitute a stationary high-fill case's spatial fields.

The velocity colour scale is shared by both fluid panels. Solid mask, velocity and liquid fraction are averaged over the two central y-cell layers. Liquid speed is shown only above liquid fraction 0.5 and open fraction 0.5. Geometry lines and coil envelopes are in the CAD panel; the simulation panels preserve the coarse native grid. [Matched-control assessment](../coil-low-fill-flow-comparison.json).

Regenerate with `TMPDIR="$PWD/artifacts/build-temp" uv run --offline --with manifold3d --with matplotlib python scripts/plot_coil_selected_run.py`. This reads saved data and performs no new simulation.
