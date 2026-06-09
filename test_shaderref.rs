use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;

// ── Toon Material ─────────────────────────────────────────────────────────────
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ToonMaterial {
    #[uniform(0)]
    pub color: LinearRgba,
}

impl Material for ToonMaterial {
    
}
