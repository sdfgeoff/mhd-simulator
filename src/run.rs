//! One physical clock and complete coupled output frames.
use crate::thermal::{ThermalConfig, ThermalFrame, ThermalState};
use crate::{
    domain::Volume,
    em::{EmConfig, EmFrame, EmState, Mode},
    fluid::{FluidConfig, FluidFrame, FluidState, ForceField},
    gpu::GpuContext,
    model::Model,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunConfig {
    pub thermal_enabled: bool,
    pub thermal: ThermalConfig,
    pub dt_s: f64,
    pub fluid_enabled: bool,
}
impl Default for RunConfig {
    fn default() -> Self {
        Self {
            thermal_enabled: false,
            thermal: Default::default(),
            dt_s: 1e-5,
            fluid_enabled: true,
        }
    }
}
impl RunConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !self.dt_s.is_finite() || self.dt_s <= 0. || !(self.dt_s as f32).is_normal() {
            Err("Choose a positive finite common timestep".into())
        } else {
            self.thermal.validate()
        }
    }
}
/// One place determines effective timesteps and force wiring for all run clients.
fn effective(
    volume: &Volume,
    config: &RunConfig,
    em: &EmConfig,
    fluid: &FluidConfig,
) -> Result<(EmConfig, FluidConfig), String> {
    config.validate()?;
    let mut em = em.clone();
    let mut fluid = fluid.clone();
    em.dt_s = config.dt_s;
    fluid.dt_s = config.dt_s;
    fluid.use_em_force = em.has_sources();
    if config.thermal_enabled && config.fluid_enabled {
        let snapshot = volume.inputs.snapshot();
        let candidates: Vec<_> = snapshot
            .materials
            .iter()
            .filter(|m| {
                m.id != 0
                    && m.dynamic_viscosity_pa_s.is_some()
                    && m.thermal.solidus_c.is_some()
                    && m.thermal.liquidus_c.is_some()
                    && snapshot
                        .components
                        .iter()
                        .any(|c| c.included && c.material_id == m.id)
            })
            .collect();
        if candidates.len() > 1 {
            return Err("Phase-driven flow currently supports one liquid material per run; multiple liquid densities need a multiphase extension".into());
        }
        fluid.material_id = candidates.first().map_or(0, |m| m.id);
    }
    Ok((em, fluid))
}
/// Integrated energy accounting. Fluid loss includes viscosity and numerical dissipation,
/// and is inferred from the fluid work balance, not silently deposited as heat.
#[derive(Clone, Debug, Default)]
pub struct EnergyLedger {
    pub surface_j: f64,
    pub surface_work_j: f64,
    pub surface_balance_error_j: f64,
    pub thermal_j: f64,
    pub thermal_deposited_j: f64,
    pub thermal_balance_error_j: f64,
    pub magnetic_j: f64,
    pub kinetic_j: f64,
    pub delta_stored_j: f64,
    pub joule_j: f64,
    pub external_work_j: f64,
    pub induction_numerical_loss_j: f64,
    pub fluid_loss_j: f64,
    pub coupling_defect_j: f64,
    pub em_balance_error_j: f64,
}
impl EnergyLedger {
    pub fn accounted_defect_j(&self) -> f64 {
        self.delta_stored_j + self.joule_j - self.thermal_deposited_j
            + self.induction_numerical_loss_j
            + self.fluid_loss_j
            - self.external_work_j
    }
}
pub struct RunFrame {
    pub surface: Option<crate::surface::SurfaceDiagnostics>,
    pub thermal: Option<ThermalFrame>,
    pub energy: EnergyLedger,
    pub time_s: f64,
    pub em: Option<EmFrame>,
    pub fluid: Option<FluidFrame>,
}
pub struct RunState {
    pub surface: Option<crate::surface::SurfaceState>,
    dynamic_volume: Option<Volume>,
    pub thermal: Option<ThermalState>,
    pub energy: EnergyLedger,
    pub em: Option<EmState>,
    pub fluid: Option<FluidState>,
    pub time_s: f64,
    pub config: RunConfig,
    pub em_config: EmConfig,
    pub fluid_config: FluidConfig,
    failed: bool,
}
impl RunState {
    pub fn new(
        context: GpuContext,
        volume: &Volume,
        model: &Model,
        config: RunConfig,
        em_config: EmConfig,
        fluid_config: FluidConfig,
    ) -> Result<Self, String> {
        let (em_config, fluid_config) = effective(volume, &config, &em_config, &fluid_config)?;
        em_config.validate(model)?;
        fluid_config.validate()?;
        let em_enabled = fluid_config.use_em_force;
        let fluid_present =
            config.fluid_enabled
                && volume
                    .inputs
                    .snapshot()
                    .components
                    .iter()
                    .any(|c| c.included && c.material_id == fluid_config.material_id)
                && volume.inputs.snapshot().materials.iter().any(|m| {
                    m.id == fluid_config.material_id && m.dynamic_viscosity_pa_s.is_some()
                });
        if !em_enabled && !fluid_present && !config.thermal_enabled {
            return Err("No active physics: enable thermal evolution, assign a liquid material with density/viscosity for fluid motion, or configure a current/magnet source.".into());
        }
        // Refuse combined working allocations before creating any solver buffers.
        let [x, y, z] = volume.dims.map(u64::from);
        let n = x * y * z;
        let nf = (x + 1) * (y + 1) * (z + 1);
        let edges = x * (y + 1) * (z + 1) + (x + 1) * y * (z + 1) + (x + 1) * (y + 1) * z;
        let mut working = volume.persistent_bytes;
        if em_enabled {
            working += edges * 52
                + n * 128
                + edges.div_ceil(256) * 64
                + n.div_ceil(256) * model.bodies.len() as u64 * 16
                + model.bodies.len() as u64 * 16
                + em_config.elements(model, 0.)?.len().max(1) as u64 * 48
                + 128;
        }
        if fluid_present {
            working += n * 148
                + nf * 48
                + n.div_ceil(256) * 48
                + 80
                + if em_enabled { n * 64 } else { 64 };
        }
        if config.thermal_enabled {
            working += n * 192 + (n.div_ceil(256) + 1) * 48;
        }
        if fluid_config.surface.enabled {
            working += n * 132 + (n.div_ceil(256) + 1) * 32;
        }
        if working > 3 * 1024 * 1024 * 1024 {
            return Err(format!(
                "Combined solver working buffers need {:.0} MiB, above the 3 GiB budget; increase cell spacing",
                working as f64 / 1048576.
            ));
        }
        if fluid_config.surface.enabled && em_enabled {
            let m = volume
                .inputs
                .snapshot()
                .materials
                .iter()
                .find(|m| m.id == fluid_config.material_id)
                .ok_or("Moving material is missing")?;
            if m.relative_permeability != Some(1.) {
                return Err("Moving EM-coupled liquid currently requires relative permeability 1; magnetic solids can remain fixed".into());
            }
            if em_config.magnets.iter().any(|m| {
                model
                    .bodies
                    .get(m.body)
                    .is_some_and(|b| b.simulation.material_id == fluid_config.material_id)
            }) {
                return Err(
                    "Prescribed permanent magnets must be attached to fixed CAD solids".into(),
                );
            }
        }
        if fluid_config.surface.enabled && !fluid_present {
            return Err("Moving liquid/air boundaries need an assigned material with liquid density and viscosity".into());
        }
        if fluid_config.surface.enabled && !config.fluid_enabled {
            return Err("Moving liquid/air boundaries require Fluid motion".into());
        }
        if fluid_config.surface.enabled
            && config.thermal_enabled
            && volume.inputs.snapshot().components.iter().any(|c| {
                c.material_id == fluid_config.material_id
                    && (c.fixed_temperature_k.is_some() || c.heat_source_w_m3 != 0.)
            })
        {
            return Err("Attach heaters/thermostats to static CAD solids; source attachment to moving liquid is not defined".into());
        }
        let (surface, dynamic_volume) = if fluid_config.surface.enabled && fluid_present {
            let (s, v) = crate::surface::SurfaceState::new(
                context.clone(),
                volume,
                fluid_config.material_id,
            )?;
            (Some(s), Some(v))
        } else {
            (None, None)
        };
        let physics_volume = dynamic_volume.as_ref().unwrap_or(volume);
        let em = if em_enabled {
            Some(EmState::new(
                context.clone(),
                physics_volume,
                model,
                em_config.clone(),
                Mode::Induction,
            )?)
        } else {
            None
        };
        let thermal = if config.thermal_enabled {
            Some(ThermalState::new(
                context.clone(),
                volume,
                config.thermal.clone(),
            )?)
        } else {
            None
        };
        let mut fluid = if fluid_present {
            Some(FluidState::new(
                context,
                volume,
                fluid_config.clone(),
                em.as_ref().map(|s| ForceField {
                    buffer: s.result_buffer(),
                    grid: &s.grid,
                }),
            )?)
        } else {
            None
        };
        if let Some(fluid) = &mut fluid {
            if let Some(surface) = &surface {
                fluid.update_surface(
                    volume,
                    surface.field(),
                    thermal.as_ref().map(|t| t.result_buffer()),
                )?;
            } else if let Some(thermal) = &thermal {
                fluid.update_phase(volume, thermal.result_buffer())?;
            }
        }
        let magnetic_j = em
            .as_ref()
            .map(|e| e.energy_step().map(|e| e.magnetic_j))
            .transpose()?
            .unwrap_or(0.);
        let kinetic_j = fluid
            .as_ref()
            .map_or(0., |f| f.diagnostics.kinetic_energy_j as f64);
        Ok(Self {
            energy: EnergyLedger {
                surface_j: surface.as_ref().map_or(0., |s| {
                    s.diagnostics.area_m2 * fluid_config.surface.tension_n_m as f64
                }),
                thermal_j: thermal.as_ref().map_or(0., |t| t.diagnostics.enthalpy_j),
                magnetic_j,
                kinetic_j,
                ..Default::default()
            },
            em,
            fluid,
            thermal,
            surface,
            dynamic_volume,
            time_s: 0.,
            config,
            em_config,
            fluid_config,
            failed: false,
        })
    }
    pub fn matches(
        &self,
        volume: &Volume,
        config: &RunConfig,
        em: &EmConfig,
        fluid: &FluidConfig,
    ) -> bool {
        self.is_current(volume)
            && &self.config == config
            && effective(volume, config, em, fluid)
                .is_ok_and(|(e, f)| e == self.em_config && f == self.fluid_config)
    }
    pub fn is_current(&self, volume: &Volume) -> bool {
        !self.failed
            && self.thermal.as_ref().is_none_or(|t| t.is_current(volume))
            && self.em.as_ref().is_none_or(|s| s.is_current(volume))
            && self.fluid.as_ref().is_none_or(|s| s.is_current(volume))
    }
    pub fn step(&mut self, model: &Model, volume: &Volume) -> Result<(), String> {
        if !self.is_current(volume) {
            return Err("Run inputs changed or a coupled step failed; restart the run".into());
        }
        let physics_volume = self.dynamic_volume.as_ref().unwrap_or(volume);
        let result = (|| {
            if let Some(em) = &mut self.em {
                if self.surface.is_some() {
                    em.update_material_geometry()?;
                }
                if let Some(fluid) = &self.fluid {
                    em.set_velocity(physics_volume, fluid.result_buffer(), &fluid.grid)?;
                }
                em.step(model, physics_volume)?;
            }
            if let Some(fluid) = &mut self.fluid {
                if let Some(em) = &self.em {
                    let m = volume
                        .inputs
                        .snapshot()
                        .materials
                        .iter()
                        .find(|m| m.id == self.fluid_config.material_id)
                        .unwrap();
                    let b = em.diagnostics.maxima[0];
                    let rho = m.density_kg_m3.unwrap();
                    let sigma = m.conductivity_s_m.unwrap_or(0.);
                    let alfven = b / (4e-7 * std::f32::consts::PI * rho).sqrt();
                    let dt = self.config.dt_s as f32;
                    if dt * alfven / fluid.grid.cell_m > 0.5 || dt * sigma * b * b / rho > 0.5 {
                        return Err("Common timestep exceeds an explicit EM/fluid coupling limit; reduce Δt".into());
                    }
                    fluid.update_force(ForceField {
                        buffer: em.result_buffer(),
                        grid: &em.grid,
                    })?;
                }
                fluid.step(volume)?;
            }
            let mut surface_fluid_energy = if self.surface.is_some() {
                self.fluid
                    .as_ref()
                    .map(FluidState::energy_step)
                    .transpose()?
            } else {
                None
            };
            if let (Some(surface), Some(fluid)) = (&mut self.surface, &self.fluid) {
                surface.advance(
                    volume,
                    fluid.face_buffer(),
                    self.thermal.as_ref().map(|t| t.result_buffer()),
                    self.config.dt_s,
                    fluid.max_face_speed(),
                )?;
                surface.update_materials(volume, physics_volume);
                if let Some(thermal) = &mut self.thermal {
                    thermal
                        .accept_surface_transport(physics_volume, surface.transported_thermal())?;
                }
            }
            if let Some(thermal) = &mut self.thermal {
                if self
                    .fluid
                    .as_ref()
                    .is_some_and(|f| f.config().periodic.iter().any(|p| *p))
                {
                    return Err(
                        "Thermal transport currently requires nonperiodic fluid boundaries".into(),
                    );
                }
                thermal.step(
                    physics_volume,
                    self.config.dt_s,
                    self.em.as_ref().map(|e| crate::thermal::Source {
                        buffer: e.result_buffer(),
                        grid: &e.grid,
                    }),
                    self.fluid
                        .as_ref()
                        .filter(|_| self.surface.is_none())
                        .map(|f| crate::thermal::Flow {
                            buffer: f.face_buffer(),
                            grid: &f.grid,
                            max_face_speed: f.max_face_speed(),
                        }),
                )?;
            }
            if let (Some(surface), Some(fluid)) = (&self.surface, &mut self.fluid) {
                fluid.update_surface(
                    volume,
                    surface.field(),
                    self.thermal.as_ref().map(|t| t.result_buffer()),
                )?;
                if let Some(f) = &mut surface_fluid_energy {
                    let after = fluid.diagnostics.kinetic_energy_j as f64;
                    f.delta_kinetic_j += after - f.kinetic_j;
                    f.kinetic_j = after;
                }
            } else if let Some(thermal) = &self.thermal {
                if let Some(fluid) = &mut self.fluid {
                    fluid.update_phase(volume, thermal.result_buffer())?;
                } else if self.config.fluid_enabled && thermal.diagnostics.liquid_volume_m3 > 0. {
                    return Err("A melted material needs liquid viscosity before fluid motion can be simulated".into());
                }
            }
            let e = self
                .em
                .as_ref()
                .map(EmState::energy_step)
                .transpose()?
                .unwrap_or_default();
            let f = if let Some(f) = surface_fluid_energy {
                f
            } else {
                self.fluid
                    .as_ref()
                    .map(FluidState::energy_step)
                    .transpose()?
                    .unwrap_or_default()
            };
            let ledger = &mut self.energy;
            if let Some(surface) = &self.surface {
                let now =
                    surface.diagnostics.area_m2 * self.fluid_config.surface.tension_n_m as f64;
                let delta = now - ledger.surface_j;
                ledger.surface_j = now;
                ledger.surface_work_j += f.surface_work_j;
                ledger.delta_stored_j += delta;
                ledger.surface_balance_error_j += delta + f.surface_work_j;
            }

            ledger.magnetic_j = e.magnetic_j;
            ledger.kinetic_j = f.kinetic_j;
            ledger.delta_stored_j += e.delta_magnetic_j + f.delta_kinetic_j;
            if let Some(t) = &self.thermal {
                ledger.thermal_j = t.diagnostics.enthalpy_j;
                ledger.delta_stored_j += t.diagnostics.delta_enthalpy_j;
                ledger.thermal_deposited_j += t.diagnostics.joule_j;
                ledger.external_work_j += t.diagnostics.external_j;
                ledger.thermal_balance_error_j += t.diagnostics.energy_error_j;
            }
            ledger.joule_j += e.joule_j;
            ledger.external_work_j += e.external_j + f.acceleration_work_j;
            ledger.induction_numerical_loss_j += e.backward_euler_j;
            ledger.fluid_loss_j +=
                f.em_work_j + f.acceleration_work_j + f.surface_work_j - f.delta_kinetic_j;
            ledger.coupling_defect_j += f.em_work_j - e.mechanical_j;
            ledger.em_balance_error_j += e.balance_error_j;
            Ok(())
        })();
        if result.is_err() {
            self.failed = true;
            return result;
        }
        self.time_s += self.config.dt_s;
        Ok(())
    }
    pub fn snapshot(&self) -> Result<RunFrame, String> {
        if self.failed {
            return Err("A failed coupled tick cannot be recorded".into());
        }
        Ok(RunFrame {
            surface: self.surface.as_ref().map(|s| s.diagnostics.clone()),
            thermal: self.thermal.as_ref().map(ThermalState::snapshot),
            energy: self.energy.clone(),
            time_s: self.time_s,
            em: self.em.as_ref().map(EmState::snapshot),
            fluid: self.fluid.as_ref().map(FluidState::snapshot),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn velocity_feedback_changes_induction_and_preserves_current_continuity() {
        let _lock = crate::gpu_test_lock();
        let p = crate::project::load(std::path::Path::new("assets/hotend-cad.mhd.json")).unwrap();
        let gpu = crate::domain::GpuDomain::new().unwrap();
        let volume = gpu.build(&p.model, &p.settings, &p.materials).unwrap();
        let mut config = EmConfig::default();
        config.uniform.push(crate::em::UniformField {
            tesla: [0., 0., 0.1],
            scale: crate::em::Waveform::Constant { value: 1. },
        });
        let mut rest = EmState::new(
            gpu.context.clone(),
            &volume,
            &p.model,
            config.clone(),
            Mode::Induction,
        )
        .unwrap();
        let mut moving = EmState::new(
            gpu.context.clone(),
            &volume,
            &p.model,
            config,
            Mode::Induction,
        )
        .unwrap();
        let grid = moving.grid.clone();
        let count = grid.dims.iter().map(|v| *v as usize).product::<usize>();
        let mut cells = vec![crate::fluid::FluidCell::default(); count];
        for (i, c) in cells.iter_mut().enumerate() {
            let z = (i / (grid.dims[0] * grid.dims[1]) as usize) as f32;
            c.velocity = [(z * 0.4).sin() * 0.1, 0., 0., 0.];
            c.reserved[0] = 1.;
        }
        use wgpu::util::DeviceExt;
        let velocity = gpu
            .context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Analytic shear velocity"),
                contents: bytemuck::cast_slice(&cells),
                usage: wgpu::BufferUsages::STORAGE,
            });
        moving.set_velocity(&volume, &velocity, &grid).unwrap();
        rest.step(&p.model, &volume).unwrap();
        moving.step(&p.model, &volume).unwrap();
        eprintln!(
            "rest J={} moving J={} divergence={} force={:?}",
            rest.diagnostics.maxima[1],
            moving.diagnostics.maxima[1],
            moving.diagnostics.divergence_j_scaled,
            moving.diagnostics.force_n
        );
        assert!(moving.diagnostics.maxima[1] > rest.diagnostics.maxima[1] + 1.);
        assert!(
            moving.diagnostics.divergence_j_scaled / moving.diagnostics.max_edge_j.max(1e-30)
                < 5e-5,
            "{:?}",
            moving.diagnostics
        );
        // No velocity feedback should give the stationary solver exactly, not leave stale EMF.
        cells.fill(crate::fluid::FluidCell::default());
        let zero = gpu
            .context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&cells),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let mut zero_em = EmState::new(
            gpu.context.clone(),
            &volume,
            &p.model,
            rest.config().clone(),
            Mode::Induction,
        )
        .unwrap();
        zero_em.set_velocity(&volume, &zero, &grid).unwrap();
        zero_em.step(&p.model, &volume).unwrap();
        assert_eq!(rest.diagnostics.maxima, zero_em.diagnostics.maxima);
    }
    #[test]
    fn coupled_tick_fails_without_publishing_a_partial_time() {
        let _lock = crate::gpu_test_lock();
        let p = crate::project::load(std::path::Path::new("assets/hotend-cad.mhd.json")).unwrap();
        let gpu = crate::domain::GpuDomain::new().unwrap();
        let volume = gpu.build(&p.model, &p.settings, &p.materials).unwrap();
        let mut fc = p.fluid;
        fc.max_iterations = 1;
        fc.relative_tolerance = 1e-7;
        fc.absolute_divergence_tolerance = 0.;
        let mut state = RunState::new(
            gpu.context.clone(),
            &volume,
            &p.model,
            RunConfig::default(),
            p.electromagnetics,
            fc,
        )
        .unwrap();
        assert!(state.step(&p.model, &volume).is_err());
        assert_eq!(state.time_s, 0.);
        assert!(!state.is_current(&volume));
        assert!(state.snapshot().is_err());
        assert_eq!(state.energy.delta_stored_j, 0.);
        assert!(state.step(&p.model, &volume).is_err());
    }
}

#[cfg(test)]
mod refinement_tests {
    use super::*;
    #[test]
    fn coupled_em_and_velocity_time_refine_together() {
        let _lock = crate::gpu_test_lock();
        let p = crate::project::load(std::path::Path::new("assets/hotend-cad.mhd.json")).unwrap();
        let gpu = crate::domain::GpuDomain::new().unwrap();
        let volume = gpu.build(&p.model, &p.settings, &p.materials).unwrap();
        let mut samples = Vec::new();
        for n in [4, 8, 16] {
            let mut s = RunState::new(
                gpu.context.clone(),
                &volume,
                &p.model,
                RunConfig {
                    dt_s: 40e-6 / n as f64,
                    fluid_enabled: true,
                    ..Default::default()
                },
                p.electromagnetics.clone(),
                p.fluid.clone(),
            )
            .unwrap();
            for i in 0..n {
                s.step(&p.model, &volume)
                    .unwrap_or_else(|e| panic!("n={n} step={i}: {e}"));
            }
            let ledger = &s.energy;
            let independent = ledger.coupling_defect_j - ledger.em_balance_error_j;
            assert!(
                (ledger.accounted_defect_j() - independent).abs() < 1e-11,
                "{ledger:?}"
            );
            let f = s.fluid.as_ref().unwrap();
            let data = crate::gpu::read_bytes(
                &gpu.context,
                f.result_buffer(),
                0,
                f.result_buffer().size(),
            )
            .unwrap();
            let cells = bytemuck::cast_slice::<u8, crate::fluid::FluidCell>(&data);
            samples.push(
                cells
                    .iter()
                    .flat_map(|c| c.velocity[..3].iter().copied())
                    .collect::<Vec<f32>>(),
            );
        }
        let norm = |a: &[f32], b: &[f32]| {
            a.iter()
                .zip(b)
                .map(|(a, b)| (*a as f64 - *b as f64).powi(2))
                .sum::<f64>()
                .sqrt()
        };
        let coarse = norm(&samples[0], &samples[2]);
        let medium = norm(&samples[1], &samples[2]);
        eprintln!("Coupled velocity Δt errors vs 2.5 us: coarse={coarse:.5e}, medium={medium:.5e}");
        assert!(medium < coarse * 0.7, "coarse={coarse} medium={medium}");
    }
}

#[cfg(test)]
mod thermal_run_tests {
    use super::*;
    use crate::{
        domain::GpuDomain,
        simulation::{DomainSettings, ThermalProperties, default_materials},
    };
    fn setup(phase: bool, cooling: bool) -> (GpuDomain, Volume, Model) {
        let gpu = GpuDomain::new().unwrap();
        let mut m = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        m.bodies[0].simulation.material_id = 2;
        m.bodies[0].simulation.temperature_c = Some(if phase {
            if cooling { 1. } else { -1. }
        } else {
            20.
        });
        m.bodies[0].simulation.heat_source_w_m3 = if phase {
            if cooling { -1e7 } else { 1e7 }
        } else {
            1e9
        };
        let mut materials = default_materials();
        if phase {
            materials[2].density_kg_m3 = Some(1000.);
            materials[2].dynamic_viscosity_pa_s = Some(0.002);
            materials[2].thermal = ThermalProperties {
                solidus_c: Some(0.),
                liquidus_c: Some(0.),
                heat_capacity_j_kg_k: Some(100.),
                conductivity_w_m_k: Some(1.),
                latent_heat_j_kg: Some(10000.),
            };
        }
        let v = gpu
            .build(
                &m,
                &DomainSettings {
                    min_mm: [-8.; 3],
                    max_mm: [8.; 3],
                    cell_mm: 1.,
                },
                &materials,
            )
            .unwrap();
        (gpu, v, m)
    }
    #[test]
    fn moving_liquid_carries_heat_and_materials_on_one_clock() {
        let _lock = crate::gpu_test_lock();
        let gpu = GpuDomain::new().unwrap();
        let mut model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        model.bodies[0].simulation.material_id = 3;
        model.bodies[0].simulation.temperature_c = Some(250.);
        let volume = gpu
            .build(
                &model,
                &DomainSettings {
                    min_mm: [-8.; 3],
                    max_mm: [8.; 3],
                    cell_mm: 1.,
                },
                &default_materials(),
            )
            .unwrap();
        let original =
            crate::gpu::read_bytes(&gpu.context, volume.material_ids(), 0, 16u64.pow(3) * 4)
                .unwrap();
        let fc = FluidConfig {
            surface: crate::surface::SurfaceConfig {
                enabled: true,
                tension_n_m: 0.,
                ..Default::default()
            },
            acceleration_m_s2: [0., 0., -0.1],
            ..Default::default()
        };
        let mut state = RunState::new(
            gpu.context.clone(),
            &volume,
            &model,
            RunConfig {
                thermal_enabled: true,
                dt_s: 1e-3,
                ..Default::default()
            },
            EmConfig::default(),
            fc,
        )
        .unwrap();
        let initial = state.surface.as_ref().unwrap().diagnostics.volume_m3;
        let heat = state.thermal.as_ref().unwrap().diagnostics.enthalpy_j;
        let retained = state.snapshot().unwrap();
        for i in 0..10 {
            state
                .step(&model, &volume)
                .unwrap_or_else(|e| panic!("tick{i}: {e}"));
        }
        let s = state.surface.as_ref().unwrap();
        let t = state.thermal.as_ref().unwrap();
        eprintln!(
            "moving thermal: mass={:.3e} H={:.3e} T={:.5}…{:.5} U={:.3e}",
            s.diagnostics.volume_m3 / initial - 1.,
            t.diagnostics.enthalpy_j / heat - 1.,
            t.diagnostics.min_temperature_k - 273.15,
            t.diagnostics.max_temperature_k - 273.15,
            state.fluid.as_ref().unwrap().diagnostics.max_speed_m_s
        );
        assert!((s.diagnostics.volume_m3 / initial - 1.).abs() < 2e-5);
        assert!((t.diagnostics.enthalpy_j / heat - 1.).abs() < 2e-5);
        let temperatures =
            crate::gpu::read_bytes(&gpu.context, t.result_buffer(), 0, t.result_buffer().size())
                .unwrap();
        let cells = bytemuck::cast_slice::<u8, crate::thermal::ThermalCell>(&temperatures);
        let max = cells
            .iter()
            .max_by(|a, b| a.state[0].total_cmp(&b.state[0]))
            .unwrap();
        eprintln!("hottest {:?}", max.state);
        assert!((t.diagnostics.max_temperature_k - 523.15).abs() < 0.02);
        assert!((t.diagnostics.min_temperature_k - 523.15).abs() < 0.02);
        assert_eq!(t.time_s, state.time_s);
        assert_eq!(state.fluid.as_ref().unwrap().time_s, state.time_s);
        let evolving = crate::gpu::read_bytes(
            &gpu.context,
            state.dynamic_volume.as_ref().unwrap().material_ids(),
            0,
            original.len() as u64,
        )
        .unwrap();
        assert!(
            bytemuck::cast_slice::<u8, u32>(&original)
                .iter()
                .zip(bytemuck::cast_slice::<u8, u32>(&evolving))
                .any(|(old, new)| *old == 0 && *new != 0)
        );
        assert_eq!(
            original,
            crate::gpu::read_bytes(
                &gpu.context,
                volume.material_ids(),
                0,
                original.len() as u64
            )
            .unwrap()
        );
        assert_eq!(retained.time_s, 0.);
        assert_eq!(retained.surface.unwrap().volume_m3, initial);
        assert!(state.snapshot().unwrap().surface.is_some());
        let e = &state.energy;
        let independent = e.coupling_defect_j - e.em_balance_error_j
            + e.thermal_balance_error_j
            + e.surface_balance_error_j;
        assert!(
            (e.accounted_defect_j() - independent).abs() < 1e-10,
            "{e:?}"
        );
    }
    #[test]
    fn source_free_thermal_run_shares_clock_and_accounts_heat() {
        let _lock = crate::gpu_test_lock();
        let (gpu, v, m) = setup(false, false);
        let config = RunConfig {
            thermal_enabled: true,
            fluid_enabled: false,
            ..Default::default()
        };
        let mut s = RunState::new(
            gpu.context.clone(),
            &v,
            &m,
            config,
            EmConfig::default(),
            FluidConfig::default(),
        )
        .unwrap();
        assert!(s.em.is_none() && s.fluid.is_none());
        for _ in 0..10 {
            s.step(&m, &v).unwrap();
        }
        let f = s.snapshot().unwrap();
        assert_eq!(f.time_s, f.thermal.as_ref().unwrap().time_s);
        assert!(f.thermal.as_ref().unwrap().diagnostics.max_temperature_k > 293.16);
        assert!(s.energy.external_work_j > 0. && s.energy.joule_j == 0.);
        assert!(
            s.energy.accounted_defect_j().abs() / s.energy.external_work_j < 1e-4,
            "{:?}",
            s.energy
        );
    }
    #[test]
    fn phase_selects_material_automatically_and_solid_cells_do_not_flow() {
        let _lock = crate::gpu_test_lock();
        for cooling in [false, true] {
            let (gpu, v, m) = setup(true, cooling);
            let config = RunConfig {
                thermal_enabled: true,
                dt_s: 0.01,
                ..Default::default()
            };
            let fluid = FluidConfig {
                acceleration_m_s2: [0.; 3],
                ..Default::default()
            };
            let mut s = RunState::new(
                gpu.context.clone(),
                &v,
                &m,
                config,
                EmConfig::default(),
                fluid,
            )
            .unwrap();
            assert_eq!(s.fluid_config.material_id, 2);
            if !cooling {
                assert_eq!(s.fluid.as_ref().unwrap().fluid_cells, 0);
                assert_eq!(s.fluid.as_ref().unwrap().diagnostics.fluid_mass_kg, 0.);
            }
            for _ in 0..if cooling { 102 } else { 3 } {
                s.step(&m, &v).unwrap();
            }
            let liquid = s.thermal.as_ref().unwrap().diagnostics.liquid_volume_m3;
            let fluid = s.fluid.as_ref().unwrap();
            assert_eq!(fluid.time_s, s.time_s);
            assert_eq!(s.thermal.as_ref().unwrap().time_s, s.time_s);
            assert!(fluid.diagnostics.rms_divergence_s_inv.is_finite());
            if cooling {
                assert_eq!(fluid.fluid_cells, 0);
                assert_eq!(liquid, 0.);
            } else {
                assert!(liquid > 0.);
                assert!(
                    (fluid.diagnostics.fluid_mass_kg as f64 / (liquid * 1000.) - 1.).abs() < 1e-4
                );
            }
        }
    }
}
