//! Section styles shared by the two native-resolution views.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Style {
    HatchedCad,
    Outlines,
    Voxels,
    Field,
}
impl Style {
    pub fn label(self) -> &'static str {
        match self {
            Self::HatchedCad => "Hatched CAD",
            Self::Outlines => "CAD outlines",
            Self::Voxels => "Voxel cells",
            Self::Field => "Analysis field",
        }
    }
}
#[derive(Clone, Copy)]
pub struct Options {
    pub style: Style,
    pub edges: bool,
    pub material_colors: bool,
}
pub type SectionRenderer = crate::render::Viewport;
