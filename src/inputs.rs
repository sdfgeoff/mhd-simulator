//! GPU input tables and field validity are independent of voxel geometry.
//!
//! Every table row is 32 bytes: `vec4<f32>` values then `vec4<u32>` flags.
//! Material rows are indexed by material ID: values = conductivity S/m,
//! relative permeability, density kg/m³, padding; flags.xyz indicate presence.
//! Component rows are indexed by body: values = initial K, prescribed A, padding;
//! flags = initial temperature present, fixed temperature present, included, material ID.
//! Zero placeholders require checking presence flags; solver_tables validates the
//! properties explicitly requested by a kernel before exposing its input tables.
use crate::{
    gpu::GpuContext,
    simulation::{FieldLocation, MaterialProperty, SimulationSnapshot},
};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct TableEntry {
    pub values: [f32; 4],
    pub flags: [u32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputVersions {
    pub geometry: u64,
    pub materials: u64,
    pub initial_conditions: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputChanges {
    pub materials: bool,
    pub initial_conditions: bool,
    pub assignments: bool,
}
#[derive(Clone)]
pub struct GpuInputs {
    snapshot: SimulationSnapshot,
    versions: InputVersions,
    pub(crate) material_table: wgpu::Buffer,
    pub(crate) component_table: wgpu::Buffer,
}
fn table(context: &GpuContext, label: &str, entries: &[TableEntry]) -> wgpu::Buffer {
    context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(entries),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        })
}
fn material_table(context: &GpuContext, s: &SimulationSnapshot) -> wgpu::Buffer {
    let mut entries = vec![
        TableEntry::zeroed();
        s.materials.iter().map(|m| m.id).max().unwrap_or(0) as usize + 1
    ];
    for m in &s.materials {
        entries[m.id as usize] = TableEntry {
            values: [
                m.conductivity_s_m.unwrap_or(0.),
                m.relative_permeability.unwrap_or(0.),
                m.density_kg_m3.unwrap_or(0.),
                m.dynamic_viscosity_pa_s.unwrap_or(0.),
            ],
            flags: [
                m.conductivity_s_m.is_some() as u32,
                m.relative_permeability.is_some() as u32,
                m.density_kg_m3.is_some() as u32,
                m.dynamic_viscosity_pa_s.is_some() as u32,
            ],
        };
    }
    table(context, "Material SI values and presence flags", &entries)
}
fn component_table(context: &GpuContext, s: &SimulationSnapshot) -> wgpu::Buffer {
    let mut entries: Vec<_> = s
        .components
        .iter()
        .map(|c| TableEntry {
            values: [
                c.initial_temperature_k.unwrap_or(0.),
                c.fixed_temperature_k.unwrap_or(0.),
                c.heat_source_w_m3,
                0.,
            ],
            flags: [
                c.initial_temperature_k.is_some() as u32,
                c.fixed_temperature_k.is_some() as u32,
                c.included as u32,
                c.material_id,
            ],
        })
        .collect();
    if entries.is_empty() {
        entries.push(TableEntry::zeroed());
    }
    table(
        context,
        "Component temperatures K and heating W/m³",
        &entries,
    )
}
impl GpuInputs {
    pub fn snapshot(&self) -> &SimulationSnapshot {
        &self.snapshot
    }
    pub fn versions(&self) -> InputVersions {
        self.versions
    }
    pub fn new(
        context: &GpuContext,
        snapshot: SimulationSnapshot,
        geometry: u64,
    ) -> Result<Self, String> {
        snapshot.validate()?;
        Ok(Self {
            material_table: material_table(context, &snapshot),
            component_table: component_table(context, &snapshot),
            snapshot,
            versions: InputVersions {
                geometry,
                materials: 1,
                initial_conditions: 1,
            },
        })
    }
    pub fn changes(&self, next: &SimulationSnapshot) -> Result<InputChanges, String> {
        next.validate()?;
        if self.snapshot.grid != next.grid
            || self.snapshot.geometry_ids != next.geometry_ids
            || self.snapshot.components.len() != next.components.len()
            || self
                .snapshot
                .components
                .iter()
                .zip(&next.components)
                .any(|(a, b)| a.included != b.included)
        {
            return Err(
                "Geometry, inclusion or grid changed; rebuild the domain before updating inputs"
                    .into(),
            );
        }
        let assignments = self
            .snapshot
            .components
            .iter()
            .zip(&next.components)
            .any(|(a, b)| a.material_id != b.material_id);
        Ok(InputChanges {
            assignments,
            materials: assignments || self.snapshot.materials != next.materials,
            initial_conditions: self.snapshot.components.iter().zip(&next.components).any(
                |(a, b)| {
                    a.initial_temperature_k != b.initial_temperature_k
                        || a.fixed_temperature_k != b.fixed_temperature_k
                        || a.heat_source_w_m3 != b.heat_source_w_m3
                },
            ),
        })
    }
    pub(crate) fn update(
        &mut self,
        context: &GpuContext,
        next: SimulationSnapshot,
        change: InputChanges,
    ) {
        if change.materials {
            self.material_table = material_table(context, &next);
            self.versions.materials += 1;
        }
        if change.initial_conditions || change.assignments {
            self.component_table = component_table(context, &next);
        }
        if change.initial_conditions {
            self.versions.initial_conditions += 1;
        }
        self.snapshot = next;
    }
    /// Required presence checks belong before dispatch, never substitute zeros for unknowns.
    pub fn solver_tables(
        &self,
        required: &[MaterialProperty],
        include_air: bool,
    ) -> Result<(&wgpu::Buffer, &wgpu::Buffer), String> {
        self.snapshot.require_properties(required, include_air)?;
        Ok((&self.material_table, &self.component_table))
    }
    pub fn bytes(&self) -> u64 {
        self.material_table.size() + self.component_table.size()
    }
}
/// Allocations can be reused after invalidation; stale results must not be displayed.
pub struct LocatedField {
    pub buffer: wgpu::Buffer,
    pub location: FieldLocation,
    pub components: u32,
}
#[derive(Default)]
pub struct FieldState {
    pub fields: Vec<LocatedField>,
    pub computed_from: Option<InputVersions>,
}
impl FieldState {
    pub fn invalidate(&mut self) {
        self.computed_from = None;
    }
    pub fn is_current(&self, inputs: &GpuInputs) -> bool {
        self.computed_from == Some(inputs.versions)
    }
}
