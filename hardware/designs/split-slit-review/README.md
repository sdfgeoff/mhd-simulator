# Split-plate slit comparison

1. These four loadable CAD/project pairs implement the proposed separate copper plates beneath a straight chamber tube. They are electrical comparison inputs, with no demonstrated slit jetting or qualified mechanical assembly.

2. The plates are 0.5 mm thick, separated by 0.2 mm. A 1 mm chamber bore gives an initially wetted slit area of about 0.199 mm². The tube is 2 mm OD, 1 mm ID and 9.5 mm long. Its bottom directly contacts both plates. The metal M6 holder ends 0.5 mm above them. Thread roots and fits are omitted.

3. `alumina` uses an insulating tube; `steel` uses stainless. The `_touch` variants deliberately bridge both plates to the conducting holder. They test whether the mounting defeats the ceramic insulation. They are not recommended mounting details. All bodies pass the generator's closed-mesh, connected-volume and pairwise overlap checks.

4. Almath lists [ALM21, 2 mm OD / 1 mm ID, open-ended 99.7% alumina tube](https://almathcrucibles.com/product/alm21/) in 1000 mm lengths (verified 19 November 2026). A short cut/lapped segment could avoid a custom ceramic orifice. Tube cutting, tolerances, retention and a leak-tight hot joint remain unresolved.

5. `*-charge-h010.mhd.json` uses 100 µm cells and includes the full heated-anchor width. `*-charge.mhd.json` uses 50 µm cells and crops that width. The serial screen includes a matching narrow 100 µm control to distinguish domain changes from resolution changes. Charge projects omit the magnetic calculation; `*-observer.mhd.json` evaluates the complete permanent magnets over the outlet. The applied-field assumption excludes magnetic reaction and permeability variation.

6. The existing copper anchors, mica, heater and magnets are retained for comparison. The folded connections are test geometry, not a finalized assembly. The fixed screening conductivities are Cu 25 MS/m, stainless 1.1 MS/m and liquid solder 2.2 MS/m. Ideal electrical joints are assumed. These cases do not quantify the benefits or problems of aluminium electrodes.

7. Next gates are electrical closure, bypass/current distribution, grid/domain sensitivity, then unforced retention and driven breakup. Surface tension alone does not establish retention: wetting, contact-line pinning and oxide films matter. Copper dissolution, thermal contact, feed heating and electrode insulation also need assessment. Previous circular-nozzle pulse settings do not qualify this different hydraulic geometry.

8. Reproduce CAD from the repository root with `uv run --with manifold3d --with matplotlib python scripts/make_split_slit_cases.py NEW_DIRECTORY`. Set `TMPDIR` to `artifacts/build-temp`. Draw it with `scripts/plot_split_slit.py CASE_DIRECTORY OUTPUT.png`. Inspect the serial plan using `python scripts/run_split_slit_screen.py --prepare-only`; the live queue waits for existing native GPU probes before starting and stops at 40% weekly remaining. Existing outputs are never automatically overwritten or resumed.
