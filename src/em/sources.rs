use crate::model::Model;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Waveform {
    Constant {
        value: f64,
    },
    Sine {
        amplitude: f64,
        frequency_hz: f64,
        phase_rad: f64,
    },
    Pulse {
        peak: f64,
        delay_s: f64,
        rise_s: f64,
        hold_s: f64,
        fall_s: f64,
    },
    Table {
        samples: Vec<[f64; 2]>,
    },
}
impl Default for Waveform {
    fn default() -> Self {
        Self::Constant { value: 1. }
    }
}
impl Waveform {
    pub fn value(&self, t: f64) -> f32 {
        (match self {
            Self::Constant { value } => *value,
            Self::Sine {
                amplitude,
                frequency_hz,
                phase_rad,
            } => amplitude * (std::f64::consts::TAU * frequency_hz * t + phase_rad).sin(),
            Self::Pulse {
                peak,
                delay_s,
                rise_s,
                hold_s,
                fall_s,
            } => {
                let t = t - delay_s;
                if t < 0. {
                    0.
                } else if t < *rise_s {
                    peak * t / rise_s
                } else if t < rise_s + hold_s {
                    *peak
                } else if t < rise_s + hold_s + fall_s {
                    peak * (1. - (t - rise_s - hold_s) / fall_s)
                } else {
                    0.
                }
            }
            Self::Table { samples } => {
                if samples.is_empty() {
                    0.
                } else if t <= samples[0][0] {
                    samples[0][1]
                } else {
                    let mut v = samples.last().unwrap()[1];
                    for pair in samples.windows(2) {
                        if t <= pair[1][0] {
                            let u = (t - pair[0][0]) / (pair[1][0] - pair[0][0]);
                            v = pair[0][1] * (1. - u) + pair[1][1] * u;
                            break;
                        }
                    }
                    v
                }
            }
        }) as f32
    }
    pub fn validate(&self) -> Result<(), String> {
        let okay = match self {
            Self::Constant { value } => value.is_finite(),
            Self::Sine {
                amplitude,
                frequency_hz,
                phase_rad,
            } => {
                amplitude.is_finite()
                    && frequency_hz.is_finite()
                    && *frequency_hz > 0.
                    && phase_rad.is_finite()
            }
            Self::Pulse {
                peak,
                delay_s,
                rise_s,
                hold_s,
                fall_s,
            } => {
                [peak, delay_s, rise_s, hold_s, fall_s]
                    .iter()
                    .all(|v| v.is_finite())
                    && *delay_s >= 0.
                    && *rise_s > 0.
                    && *hold_s >= 0.
                    && *fall_s > 0.
            }
            Self::Table { samples } => {
                !samples.is_empty()
                    && samples.iter().flatten().all(|v| v.is_finite())
                    && samples.windows(2).all(|p| p[1][0] > p[0][0])
            }
        };
        if okay {
            Ok(())
        } else {
            Err(
                "Invalid source waveform (times must be finite, ordered and rise/fall positive)"
                    .into(),
            )
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wire {
    /// Centreline in the main CAD assembly.
    pub cad_curve: usize,
    #[serde(default)]
    pub reverse: bool,
    pub name: String,
    pub core_radius_mm: f32,
    pub current: Waveform,
}
impl Wire {
    pub fn for_curve(index: usize, curve: &crate::model::CurvePath) -> Self {
        Self {
            cad_curve: index,
            reverse: false,
            name: curve.name.clone(),
            core_radius_mm: 0.15,
            current: Waveform::Constant { value: 0. },
        }
    }
    pub fn points<'a>(
        &'a self,
        model: &'a Model,
    ) -> Result<std::borrow::Cow<'a, [[f32; 3]]>, String> {
        let points = &model
            .curves
            .get(self.cad_curve)
            .ok_or_else(|| {
                format!(
                    "Wire {} refers to missing CAD curve {}",
                    self.name, self.cad_curve
                )
            })?
            .points_mm;
        if self.reverse {
            Ok(std::borrow::Cow::Owned(
                points.iter().rev().copied().collect(),
            ))
        } else {
            Ok(std::borrow::Cow::Borrowed(points))
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Magnet {
    pub body: usize,
    pub magnetization_a_m: [f32; 3],
    pub subdivisions: u32,
    pub smoothing_mm: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UniformField {
    pub tesla: [f32; 3],
    pub scale: Waveform,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmConfig {
    pub wires: Vec<Wire>,
    pub magnets: Vec<Magnet>,
    pub uniform: Vec<UniformField>,
    pub dt_s: f64,
    pub relative_tolerance: f32,
    pub max_iterations: u32,
}
impl Default for EmConfig {
    fn default() -> Self {
        Self {
            wires: Vec::new(),
            magnets: Vec::new(),
            uniform: Vec::new(),
            dt_s: 1e-5,
            relative_tolerance: 1e-5,
            max_iterations: 1000,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Element {
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub c: [f32; 4],
}
impl EmConfig {
    pub fn validate(&self, model: &Model) -> Result<(), String> {
        if !(self.dt_s as f32).is_finite()
            || self.dt_s as f32 <= 0.
            || !self.dt_s.is_finite()
            || self.dt_s <= 0.
            || !self.relative_tolerance.is_finite()
            || !(1e-7..=1e-2).contains(&self.relative_tolerance)
            || self.max_iterations == 0
            || self.max_iterations > 20000
        {
            return Err("Invalid EM timestep, tolerance or iteration limit".into());
        }
        for wire in &self.wires {
            let points = wire.points(model)?;
            wire.current.validate()?;
            if points.len() < 2
                || points.iter().flatten().any(|v| !v.is_finite())
                || !wire.core_radius_mm.is_finite()
                || wire.core_radius_mm <= 0.
                || points
                    .windows(2)
                    .any(|p| Vec3::from(p[0]).distance(Vec3::from(p[1])) < 1e-6)
            {
                return Err(format!(
                    "Wire {} needs finite nonzero segments and a positive smoothing/core radius",
                    wire.name
                ));
            }
        }
        for magnet in &self.magnets {
            if magnet.body >= model.bodies.len()
                || !Vec3::from(magnet.magnetization_a_m).is_finite()
                || magnet.subdivisions > 4
                || !magnet.smoothing_mm.is_finite()
                || magnet.smoothing_mm <= 0.
                || !model.bodies[magnet.body].closed()
            {
                return Err("Magnet needs a closed CAD body, finite magnetization, positive smoothing and subdivisions 0..4".into());
            }
        }
        let count = self
            .wires
            .iter()
            .map(|w| w.points(model).map(|p| p.len().saturating_sub(1) as u64))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .sum::<u64>()
            .saturating_add(
                self.magnets
                    .iter()
                    .map(|m| {
                        (model.bodies[m.body].triangles().len() as u64)
                            .saturating_mul(4u64.pow(m.subdivisions))
                    })
                    .sum::<u64>(),
            )
            .saturating_add(self.uniform.len() as u64);
        if count > 1_000_000 {
            return Err("Source discretization exceeds one million elements; reduce magnet refinement or source geometry".into());
        }
        for source in &self.uniform {
            source.scale.validate()?;
            if !Vec3::from(source.tesla).is_finite() {
                return Err("Uniform B must be finite".into());
            }
        }
        Ok(())
    }
    pub(crate) fn elements(&self, model: &Model, t: f64) -> Result<Vec<Element>, String> {
        self.validate(model)?;
        let mut out = Vec::new();
        for wire in &self.wires {
            let current = wire.current.value(t);
            for pair in wire.points(model)?.windows(2) {
                out.push(Element {
                    a: (Vec3::from(pair[0]) * 1e-3).extend(0.).to_array(),
                    b: (Vec3::from(pair[1]) * 1e-3).extend(0.).to_array(),
                    c: [wire.core_radius_mm * 1e-3, 0., current, 0.],
                });
            }
        }
        for magnet in &self.magnets {
            let body = &model.bodies[magnet.body];
            let m = Vec3::from(magnet.magnetization_a_m);
            if m == Vec3::ZERO {
                continue;
            }
            for triangle in body.triangles() {
                let mut triangles = vec![triangle.map(|p| p * 1e-3)];
                for _ in 0..magnet.subdivisions {
                    triangles = triangles
                        .into_iter()
                        .flat_map(|[a, b, c]| {
                            let ab = (a + b) * 0.5;
                            let bc = (b + c) * 0.5;
                            let ca = (c + a) * 0.5;
                            [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]
                        })
                        .collect();
                }
                for [a, b, c] in triangles {
                    let area = (b - a).cross(c - a) * 0.5 * body.winding;
                    let k_area = m.cross(area);
                    out.push(Element {
                        a: ((a + b + c) / 3.).extend(0.).to_array(),
                        b: k_area.extend(0.).to_array(),
                        c: [magnet.smoothing_mm * 1e-3, 1., 1., 0.],
                    });
                }
            }
        }
        for field in &self.uniform {
            out.push(Element {
                a: [0.; 4],
                b: Vec3::from(field.tesla).extend(0.).to_array(),
                c: [0., 2., field.scale.value(t), 0.],
            });
        }
        if out.len() > 1_000_000
            || out
                .iter()
                .any(|e| e.a.iter().chain(&e.b).chain(&e.c).any(|v| !v.is_finite()))
        {
            return Err("Source discretization is too large or has nonfinite values".into());
        }
        Ok(out)
    }
}
/// Imported CAD curves are prescribed-current paths, not conducting solid bodies.
pub fn cad_wires(model: &Model) -> Vec<Wire> {
    model
        .curves
        .iter()
        .enumerate()
        .map(|(i, curve)| Wire::for_curve(i, curve))
        .collect()
}
