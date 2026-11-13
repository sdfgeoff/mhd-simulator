# Temperature, latent heat and phase-driven flow

## Workflow

1. In Setup, assign materials and initial object temperatures. The material editor contains density, heat capacity, thermal conductivity, solidus/liquidus and latent heat. Objects can also impose a fixed temperature or volumetric heater (W/m³).
2. In Simulation, enable **Thermal / phase evolution**. Set the common timestep and thermal boundaries: insulated, specified temperature, inward heat flux or convection. Six domain faces and exposed CAD/air faces are independently configured. No electrical source is required.
3. Enable Fluid motion to let temperature select the liquid material automatically. Exactly one assigned material with melting data and liquid viscosity is supported by this uniform-density flow solver. Solid cells stop; partially molten cells resist motion. With thermal evolution disabled, the manual fixed-liquid-region diagnostic remains available.
4. Simulate and select Temperature, Liquid fraction or Heat flux in Section view. Both viewports, probes and all physics use the common timeline. `assets/hotend-thermal.mhd.json` is a source-free example: a 250 °C chamber heats initially 20 °C solder. The example's liquid/air geometry is still fixed in this stage.

## Numerical model

Cell enthalpy per volume is `H = ρ cp (T − 273.15 K) + ρ L f`. For a single melting point, latent heat changes liquid fraction at constant temperature. For an alloy, fraction varies linearly across the solidus/liquidus interval. Below/above the transition it is zero/one. Missing latent heat is accepted below solidus but crossing the transition fails explicitly; unknown heat capacity or conductivity fails setup. Starting exactly at a pure melting point initializes solid unless subsequent energy melts it.

The GPU advances conservative harmonic face conduction, upwind face-velocity enthalpy transport, Joule heating, body heaters and boundary heat. Explicit thermal substeps fit within the common tick using diffusion, convection and advection bounds. A split storage-rounded compensation preserves small heat increments against large baseline enthalpy in f32. Diagnostics separately integrate supplied heat and measured enthalpy change; a failed step restores the accepted state.

Fluid mobility uses `β = 1/[1 + Δt C (1−f)²/(f³+0.001)]`, with editable mushy resistance C (default 10⁶ s⁻¹). Predictor and pressure operator use the same mobility. Fully solid cells leave the pressure region; surviving velocities are preserved and reprojected when topology changes. This is an enthalpy/porosity model, not a resolved dendrite model. See the primary [OpenFOAM solidification/melting model](https://cpp.openfoam.org/v13/classFoam_1_1fv_1_1solidificationMelting.html) for this modeling approach.

A common tick advances induction, fluid, then thermal energy and phase. Joule heat comes from the new EM field and enthalpy advection from the new velocity. Phase changes update fluid mobility before the complete frame is retained. This remains first-order partitioned coupling; refine time and space. Energy accounting avoids counting deposited Joule heat twice. Fluid split/viscous loss is reported, but is not yet returned as heat.

## Validation and limits

1. Insulated cosine conduction matches the discrete eigenmode; continuum RMS error falls from 0.00227 K to 0.000571 K at 16/32 cells.
2. A one-phase Stefan melting front at 64/128 cells has relative front error 0.197%/0.0243%; supplied boundary heat closes against enthalpy.
3. Tests cover pure latent plateaus, alloy melt/freeze flow transitions, missing-data rollback, prescribed Joule input, sub-ULP heat accumulation, phase-weighted hydrostatics, source-free common runs and offscreen thermal UI rendering.
4. Air heat capacity/transport, radiation, thermal expansion, temperature-dependent electromagnetic/transport coefficients and moving feedstock are absent. Density, cp, conductivity and latent heat are constants. Thermal/phase flow currently uses nonperiodic boundaries. The liquid/air surface is fixed until the next interface stage.

## Default data

These constants are starting values, not calibrated molten-metal properties. **Solder latent heats (45/59 kJ/kg), Sn60Pb40 cp (190 J/kg K) and magnet conductivity (9 W/m K) are illustrative assumptions.** Edit them for the experiment. Other latent heats remain unknown rather than guessed.

| Material | cp J/(kg K) | k W/(m K) | Solidus–liquidus °C | Basis |
| --- | ---: | ---: | ---: | --- |
| Copper | 394 | 394 | 1084.62 | [KME Cu-ETP](https://www.kme.com/fileadmin/DOWNLOADCENTER/COPPER%20DIVISON/4%20Industrial%20Rolled/3_Datasheets/Datasheets_NEW_2021/Cu-ETP_01_2021_e.pdf), room-temperature cp/k |
| Sn60Pb40 | 190* | 49 | 183–190 | [Cynel](https://cynel.com.pl/images/cnl-pliki/files/tds/TDS-Sn60Pb40-PRO-en.pdf), cp illustrative |
| Sn99.3Cu0.7 | 214 | 60.5 | 227 | [ALPHA](https://www.macdermidalpha.com/sites/default/files/2024-04/ALPHA-VACULOY-Sn99.3Cu0.7-Solid-Solder-EN-12Apr24-TB.pdf) |
| NdFeB approximation | 450 | 9* | unknown | cp within [K&J](https://www.kjmagnetics.com/neodymium-magnet-specifications.asp) 350–500 range; k illustrative |
| 316 stainless | 500 | 15 | 1375–1400 | [thyssenkrupp](https://www.thyssenkrupp-materials.co.uk/stainless-steel-316-14401.html), room-temperature cp/k |
| Alumina AD-995 | 880 | 30 | unknown | [CoorsTek](https://www2.coorstek.com/media/4235/advanced-alumina.pdf), cp at 100 °C, k at 20 °C |
| Aluminium 6061 | 896 | 167 | 582–652 | [Kaiser](https://online.kaiseraluminum.com/depot/PublicProductInformation/Document/1006/Kaiser_Aluminum_Shapes_Soft_Alloy.pdf), cp at 100 °C |

Project version 2 preserves explicit unknowns and overrides. Loading a version 1 project fills only unset thermal fields of matching named presets and records a migration note; existing scalar values remain intact. Thermal evolution defaults off for older projects.
