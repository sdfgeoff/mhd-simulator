use glam::{Quat, Vec3};
/// Local X is the cut normal; Y/Z span the plane. Position is the gizmo pivot.
#[derive(Clone, Copy, Debug)]
pub struct SectionPlane {
    pub position: Vec3,
    pub orientation: Quat,
}
impl SectionPlane {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            orientation: Quat::IDENTITY,
        }
    }
    pub fn normal(self) -> Vec3 {
        self.orientation * Vec3::X
    }
    pub fn u(self) -> Vec3 {
        self.orientation * Vec3::Y
    }
    pub fn v(self) -> Vec3 {
        self.orientation * Vec3::Z
    }
    pub fn rotate_world(&mut self, axis: Vec3, angle: f32) {
        self.orientation =
            (Quat::from_axis_angle(axis.normalize(), angle) * self.orientation).normalize();
    }
}
