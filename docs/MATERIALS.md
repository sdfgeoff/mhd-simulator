# Material defaults

Named presets provide usable starting scalars. **Unassigned** is deliberately empty and requires a material choice. These are constants: the current solver does not derive temperature-dependent conductivity, permeability or density from the component's temperature input.

| Preset | Conductivity (S/m) | Relative permeability | Density (kg/m³) | Basis |
| --- | ---: | ---: | ---: | --- |
| Air / void | 0 | 1 | 1.2 | EM vacuum/insulator approximation; approximate room-temperature air density |
| Copper | 58,000,000 | 1 | 8,890 | 100% IACS reference at 20 °C; unit-permeability approximation. [Copper information centre](https://help.copper.fyi/hc/en-us/articles/4410229175442-How-does-copper-set-the-standard-for-electrical-conductivity) |
| Sn60Pb40 | 6,671,114 | 1 | 8,520 | Bulk solder resistivity 14.99 µΩ·cm, density 8.52 Mg/m³; unit-permeability approximation. [AMETEK datasheet](https://www.ametekinterconnect.com/-/media/ametek-ecp/v2/files/cw_datasheets_sds_cfsi/datasheets/sn60pb40%20data%20sheet.pdf?la=en) |
| Sn99.3Cu0.7 | 8,340,284 | 1 | 7,420 | Bulk solder resistivity 11.99 µΩ·cm, density 7.42 g/cm³; unit-permeability approximation. [ALPHA VACULOY datasheet](https://www.macdermidalpha.com/sites/default/files/2024-04/ALPHA-VACULOY-Sn99.3Cu0.7-Solid-Solder-EN-12Apr24-TB.pdf) |
| Permanent magnet | 625,000 | 1 | 7,500 | Representative NdFeB resistivity 160 µΩ·cm and density at the upper end of the stated 7.4–7.5 g/cm³ range. **μr=1 is the solver approximation**; the supplier lists recoil permeability 1.05. [K&J specifications](https://www.kjmagnetics.com/neodymium-magnet-specifications.asp) |
| 316 stainless steel | 1,333,333 | 1 | 8,000 | Resistivity 0.75 Ω·mm²/m and density at 20 °C. Unit permeability approximates annealed/nonmagnetic material; processing matters. [thyssenkrupp datasheet](https://www.thyssenkrupp-materials.co.uk/stainless-steel-316-14401.html/index.html) |
| Alumina | 0 | 1 | 3,900 | Representative CoorsTek AD-995 (99.5%) density. **σ=0 and μr=1 are modeling approximations**; room-temperature resistivity exceeds 10¹⁴ Ω·cm. Grade and temperature matter. [CoorsTek datasheet](https://www2.coorstek.com/media/4235/advanced-alumina.pdf) |

Conductivity is converted as σ=1/ρₑ. One µΩ·cm is 10⁻⁸ Ω·m. The solder values are bulk datasheet references, not calibrated molten-metal properties. The magnet preset does not imply a particular strength or orientation: assign magnetization independently on each CAD solid. The model does not include demagnetization evolution or temperature effects.

Existing saved values and custom materials are preserved on load. Named materials already contain their initial defaults; reference notes are visible beside the shared material editor.

## Liquid viscosity

The solder presets include **0.002 Pa·s (2 mPa·s)** as an illustrative starting assumption for the fixed-geometry fluid solver. It is not a measured fit for either alloy at the selected temperature. Other presets leave liquid viscosity unset. Choose operating-temperature density and viscosity before interpreting a fluid experiment; temperature inputs do not apply property curves automatically. [Fluid configuration, numerical assumptions and tests](FLUIDS.md) describes the solver. Existing projects with unset solder viscosity can set it directly in the shared material editor.

## Shared entities and thermal setup

Objects reference material IDs; assignment does not copy scalar values. Editing a material updates all its users. Older projects gain missing built-in library entries without replacing their saved material values. There is no fill/copy step in the UI.

**Aluminium 6061 (T6)** defaults to density 2700 kg/m³, conductivity 25 MS/m and unit permeability. Thermal setup uses 167 W/(m K), heat capacity 896 J/(kg K) at 100 °C, and melting interval 582–652 °C, from [Kaiser Aluminum technical data](https://online.kaiseraluminum.com/depot/PublicProductInformation/Document/1006/Kaiser_Aluminum_Shapes_Soft_Alloy.pdf). The heat capacity is converted from the stated 0.214 BTU/(lb °F); the metric unit label in that document is inconsistent. Latent heat and liquid viscosity remain unknown. These constants are representative; temper and operating temperature matter.

Materials can store solidus/liquidus temperatures, heat capacity, thermal conductivity and latent heat. Equal solidus/liquidus represent a single melting point. The initial phase percentage in Setup is only a preview; heat transport and phase-dependent fluid/solid evolution remain unimplemented.
