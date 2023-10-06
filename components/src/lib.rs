use std::sync::Arc;

use common::{
    glam::{UVec2, Vec2, Vec3, Vec4},
    hecs::Entity,
};
mod transform;
pub use transform::Transform;

#[derive(Debug, Clone)]
pub struct GLTFAsset {
    pub name: String,
}

impl GLTFAsset {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self { name: name.into() }
    }
}

/// tag component to indicate that we'd like a collider based on our geometry, please
#[derive(Debug, Clone, Default)]
pub struct Collider {
    pub y_offset: f32,
}

pub struct Parent {
    pub entity: Entity,
    pub offset: Transform,
}

#[derive(Debug, Clone, Default)]
pub struct Velocity {
    pub linear: Vec3,
}

#[derive(Debug, Clone, Default)]
pub struct Info {
    pub name: String,
}

impl Info {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self { name: name.into() }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Selected;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Vertex {
    pub position: Vec4,
    pub normal: Vec4,
    pub uv: Vec2,
}

#[derive(Debug, Clone)]
pub struct GLTFModel {
    pub primitives: Arc<Vec<Primitive>>,
}

#[derive(Debug, Clone)]
pub struct Material {
    pub base_colour_texture: Option<Texture>,
    pub base_colour_factor: Vec4,
    pub normal_texture: Option<Texture>,
    pub metallic_roughness_ao_texture: Option<Texture>,
    pub emissive_texture: Option<Texture>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_colour_texture: Default::default(),
            base_colour_factor: Vec4::ONE,
            normal_texture: Default::default(),
            metallic_roughness_ao_texture: Default::default(),
            emissive_texture: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Texture {
    /// x, y
    pub dimensions: UVec2,
    /// data is assumed to be R8G8B8A8
    pub data: Vec<u8>,
}

impl Vertex {
    pub fn new<T: Into<Vec4>, U: Into<Vec2>>(position: T, normal: T, uv: U) -> Self {
        Self {
            position: position.into(),
            normal: normal.into(),
            uv: uv.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Primitive {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material: Material,
}

#[derive(Debug, Clone)]
pub struct MaterialOverrides {
    pub base_colour_factor: Vec4,
}

#[derive(Debug, Clone, enum_iterator::Sequence, Copy)]
pub enum Resource {
    Wood,
    Coal,
    Uranium,
    Boots,
    Fish,
    Bread,
    HorseMeat,
    Crabs,
    Amethyst,
    GolfBalls,
}

#[derive(Debug, Clone)]
pub struct Business {
    pub name: String,
    pub contract: Contract,
}

#[derive(Debug, Clone)]
pub struct Contract {
    pub quotas: Vec<Quota>,
}

#[derive(Debug, Clone)]
pub struct Quota {
    pub resource: Resource,
    pub amount_per_day: usize,
}
