# Plain bore pair: CAD review before simulation

Requested in topic 25, replies 797–801. **No GPU simulation has started.** The user requested seeing the CAD before launch.

**Manufacturing correction, reply 804:** these drafts are custom M6-compatible shapes, not subtractive conversions verified against a stock V6 solid. Their 10 mm body and stockier lower nose differ from a standard 12.5 mm V6. Closed meshes and absence of overlaps do not establish stock containment. Do not launch these drafts as a stock-V6 conversion test. A subtractive-only revision must preserve the stock mounting and outlet, start from the correct smaller-bore variant, and check all proposed cuts and remaining walls. An existing 3.2 mm bore cannot be reduced to 2 or 3 mm by cutting.

![Previous geometry and proposed straight bores](cad-comparison.png)

1. Previous reference: a 3.2-to-2 mm internal shoulder, 2.8 mm neck width and 3.4 mm magnet pole gap. The shoulder was introduced in our custom model; it is not in the user's standard-nozzle drawing.
2. Proposed pair: straight **2 mm** and **3 mm** chambers, both with the same **3.4 mm neck width and 4.0 mm pole gap**. The wider common neck leaves 0.2 mm minimum side wall around the 3 mm chamber. Both retain 0.3 mm clearance from each magnet face to the neck.
3. Common features: M6 mounting envelope, 0.4 mm outlet, 0.6 mm outlet land, 118° lower drill taper, 0.8 mm square tip boss with R0.1 root, copper contacts, SmCo source settings, materials, 6 mm initial fill height, 50 µm grid, gravity, domain and electrical boundary conditions. No feed wire. The heater is shown in CAD but excluded from flow, matching the earlier experiment.
4. Proposed waveform: +110 A / 250 µs forward, 25 µs gap, reverse 27.5 A / 83.33 µs, uniform 1.5 µs time steps through 3.6 ms. This is a proposal copied from the completed reference, not a performance result for either new geometry.
5. Same fill height does not mean equal liquid volume. Bore diameter also changes the length of the 118° outlet taper. These are inherent effects of this diameter comparison. Since both cases use a wider exterior and magnet gap, comparisons to the historical narrow-neck result would not isolate bore diameter alone.
6. Checks completed: both cases have closed solids with positive volume and no generator-detected overlaps; the native CAD importer reports closed bodies. All mesh position payloads after the nozzle/liquid bodies are byte-identical between the two proposed cases. Effective flow settings are identical except geometry and the nozzle/liquid triangle counts. These are geometry/input checks, not GPU tests.

Loadable projects and geometry: [2 mm](../designs/plain-bore-2mm/flow-h0050.mhd.json), [3 mm](../designs/plain-bore-3mm/flow-h0050.mhd.json). The dedicated conduction-flow experiment uses the prescribed terminal pulse; opening a project in the GUI does not reproduce those electrode boundary conditions by itself.

The comparison preparation script is `scripts/prepare_plain_bore_review.py`. CAD is generated using `scripts/make_stub_m6_cases.py` with `--chamber-bore-mm 2` or `3`, `--neck-flat-halfwidth-mm 1.7`, `--pole-gap-mm 4`, `--heater-clearance-mm 1`, `--lower-drill-half-angle-deg 59`, `--boss-width-mm .8`, `--tip-root-radius-mm .1` and `--contact-height-mm .35`; `scripts/make_hot_contact_case.py` adds the shared contacts/heater fixture with anchor width 10 mm. Generated source cases are preserved under `artifacts/hardware-research/cases/plain-bore-v1-{2,3}mm-source`.
