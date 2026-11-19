# Short ceramic throat and metal reservoir

1. Geometry candidate only. A 2 mm-long piece of the same stock OD2/ID1 alumina tube sits below a 1.8 mm stainless reservoir bore. The tube top is z=2.5 mm and the initial liquid level z=9 mm. The outlet still uses two copper plates with a 0.2 mm gap. No new electrical, thermal or fluid result is attached to this variant.

2. The holder uses a 2 mm counterbore from below and a 1.8 mm bore above, leaving a nominal 0.1 mm radial locating shoulder. That is a geometric stop, not qualified ceramic retention or a pressure seal. Tolerances, edge chipping, clamp load and hot leakage need assessment. Thread geometry remains an envelope.

3. The intended benefits are a shorter narrow liquid section and direct heating of the wider pool through metal. The higher fill level increases overlap with the heater. Those benefits need calculation with the actual three-dimensional fields and thermal boundaries; neither is established by the CAD.

The simple plug-flow comparison in `verification.json` gives about 20% lower hydraulic inertance, corresponding to only about 11% shorter displacement time at equal net pressure. Its radial thermal-resistance estimate omits axial paths, interfaces, plate cooling and feed. It is an experiment-selection scale, not a pool-temperature prediction.

4. Conducting reservoir walls can provide a bypass reached through the liquid above the throat. A short ceramic insert does not make every liquid current path useful for outlet forcing. Compare this against the full-length insulating tube before choosing it. The existing reused-field crop and hard-coded baseline slit assessment do not cover this wider, taller liquid region.

5. A 3.2 mm reservoir with the same low transition was rejected: its wider holder intersects the existing poles. The present 1.8 mm version passes the generator's pairwise overlap, connected-volume and closed-mesh checks. The original slit projects and geometry remain byte-identical under the generator defaults.

6. Reproduce all material/bypass variants with `scripts/make_split_slit_cases.py NEW_DIRECTORY --tube-top-mm 2.5 --reservoir-id-mm 1.8 --pool-top-mm 9`, using the manifold3d/matplotlib environment and workspace TMPDIR described in the baseline slit package. This directory retains just the alumina candidate and its cold/observer projects. Magnets, electrodes and heater retain the baseline geometry.
