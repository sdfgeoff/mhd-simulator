# Side-heated body with the heater closer to the pool

Manufacture-first revision under topic 25 replies 842/838. The nominal electrical screen remains 100 A. No copper busbars are added.

![Original and revised cuts](proposed-cuts.png)

1. Move the heater centre from x=−6, z=10 mm to x=−4, z=8.5 mm. The 3.2 mm bore retains nominal 0.9 mm steel ligaments to the lower block face and its inner side face. The liquid cavity, 2 mm chamber, 0.4 mm outlet / 0.6 mm land, initial fill, window roots and magnets are unchanged.
2. Move the sensor to x=−7, z=8.5 mm so the heater and sensor bores retain a nominal 0.8 mm separation. Raise the left cable terminal to z=11 mm; a 3.1 mm-deep straight M3 tap-drill envelope and its 118° drill point stay separate from the heater. The right terminal is centred at z=9 mm in a taller 6 mm-high pad, with a through tap-drill hole. These changes provide nominal room around ordinary M3 hardware rather than leaving the earlier shallow blind holes.
3. This is a coupled machining-layout revision, not an isolated heater-position experiment: sensor/terminal holes and the return pad also change. Current sharing, Joule heat and force must be recalculated.
4. CPU checks: one connected stainless part; no significant inter-body overlaps; native project loading passes. The shared generator reproduces both completed original concept geometries byte for byte. [Baseline regression](generator-regression.json). Initial GPU voxelization of this revision has zero overlaps; no physical performance is inferred from that check.
5. Next: full-body 100 A static electrical screening and the same optimistic heater-wall-to-isothermal-pool conductance comparison. The thermal boundary selector now accepts explicit sensor coordinates rather than assuming that it moves with the heater. All previous limits on contacts, losses, finite heater power, feed and magnet temperatures remain.

[Loadable compact project](../../designs/integral-side-heated-near-v1/electrical-h0100-compact.mhd.json), [geometry manifest](../../designs/integral-side-heated-near-v1/manifest.json). Dimensions are nominal CAD, not tolerance/strength qualification. Generate a fresh directory with `scripts/make_integral_heads.py --only side-heated-near --review-root hardware/integral-head-review/near-heater`.
