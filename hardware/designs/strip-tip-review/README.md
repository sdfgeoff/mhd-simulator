# Replaceable strip electrical comparison

1. Each project is a cold static-current research input. A 0.4 mm hole passes
   through a continuous strip between copper terminals. The existing raised pool
   and magnet geometry are retained. The lower stainless boss is cut back to
   leave a nominal 0.1 mm insulating separation above the strip. This separation
   is an electrical assumption: no gasket, clamping or leak-tight seal is designed.
2. `steel08` and `copper08` use 0.8 mm wide, 0.5 mm thick strips; `steel28` and
   `copper28` use 2.8 mm widths. The central material changes within each pair;
   the outer copper extensions have identical geometry and conductivity.
   The `t01` variants use 0.1 mm thickness and a correspondingly lower upper-body
   cutoff. They have only two cells through thickness and are preliminary.
3. At 30 A, native 50 µm outlet-region forces are 2.843/0.535 mN for narrow
   stainless/copper and 0.904/0.126 mN for wide stainless/copper. The 0.1 mm
   variants give 3.024/0.713 mN and 1.003/0.171 mN respectively. Thinning changes
   both the liquid-current geometry and strip resistance; it is not a pure
   material substitution against the thicker case.
4. Fixed conductivities are 1.1 MS/m stainless, 25 MS/m copper and 2.2 MS/m
   solder. Ideal contacts, fixed magnetization and no material/temperature feedback
   are assumed. The outlet metric integrates liquid Lorentz force from z=0 to
   0.6 mm. These are force comparisons, not drop or lifetime predictions.
5. Open the projects for inspection. They do not configure terminal-current
   drive through the main GUI. From the research checkout, reproduce a native
   solve using `target/debug/examples/conduction_probe PROJECT 30 OUTPUT 0 x-ends 1e-6`.
   Generate new inputs with `scripts/make_strip_tip_cases.py`; see its arguments
   for thickness/output. The analysis and current-slice plotting scripts read
   the recorded native fields. No duplicate CPU field solver is used.
6. Continuous copper substantially bypasses liquid current in this family.
   A replaceable stainless strip or a deliberately split electrode assembly is
   therefore worth further design. Do not infer that every copper geometry fails.
   Neither sealing, hot resistance, electrode recession nor mechanical mounting
   has been qualified. Literature and coupon-test concerns are recorded in
   `hardware/strip-contact-materials-screening.json`.

7. The `steel08s0` and `steel28s0` cases instead seat the stainless strip directly
   against the upper stainless body, with an ideal metal interface. They give
   2.126 and 0.806 mN at 30 A: 75% and 89% of their isolated counterparts.
   The narrow case is within 3% of the prior integral rooted-boss force metric.
   Electrical isolation of this strip/body joint is therefore not essential to
   retain comparable cold force in that narrow geometry. It is still not a seal
   qualification. The small upper-body sealing footprint, access for clamps and
   contacts, surface finish and actual interface resistance remain unresolved.
