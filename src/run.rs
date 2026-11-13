//! One physical clock and complete coupled output frames.
use crate::{
    domain::Volume,
    em::{EmConfig, EmFrame, EmState, Mode},
    fluid::{FluidConfig, FluidFrame, FluidState, ForceField},
    gpu::GpuContext,
    model::Model,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunConfig {
    pub dt_s: f64,
    pub fluid_enabled: bool,
}
impl Default for RunConfig {
    fn default() -> Self {
        Self {
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
            Ok(())
        }
    }
}
/// One place determines effective timesteps and force wiring for all run clients.
fn effective(
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
    Ok((em, fluid))
}
pub struct RunFrame {
    pub time_s: f64,
    pub em: Option<EmFrame>,
    pub fluid: Option<FluidFrame>,
}
pub struct RunState {
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
        let (em_config, fluid_config) = effective(&config, &em_config, &fluid_config)?;
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
        if !em_enabled && !fluid_present {
            return Err("No active physics: assign a liquid material with density/viscosity for fluid motion, or configure a current/magnet source. Thermal evolution is not implemented yet.".into());
        }
        let em = if em_enabled {
            Some(EmState::new(
                context.clone(),
                volume,
                model,
                em_config.clone(),
                Mode::Induction,
            )?)
        } else {
            None
        };
        let fluid = if fluid_present {
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
        Ok(Self {
            em,
            fluid,
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
            && effective(config, em, fluid)
                .is_ok_and(|(e, f)| e == self.em_config && f == self.fluid_config)
    }
    pub fn is_current(&self, volume: &Volume) -> bool {
        !self.failed
            && self.em.as_ref().is_none_or(|s| s.is_current(volume))
            && self.fluid.as_ref().is_none_or(|s| s.is_current(volume))
    }
    pub fn step(&mut self, model: &Model, volume: &Volume) -> Result<(), String> {
        if !self.is_current(volume) {
            return Err("Run inputs changed or a coupled step failed; restart the run".into());
        }
        let result = (|| {
            if let Some(em) = &mut self.em {
                if let Some(fluid) = &self.fluid {
                    em.set_velocity(volume, fluid.result_buffer(), &fluid.grid)?;
                }
                em.step(model, volume)?;
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
                },
                p.electromagnetics.clone(),
                p.fluid.clone(),
            )
            .unwrap();
            for i in 0..n {
                s.step(&p.model, &volume)
                    .unwrap_or_else(|e| panic!("n={n} step={i}: {e}"));
            }
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
