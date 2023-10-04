use std::collections::VecDeque;

pub use anyhow;
pub use bitflags;
pub use glam;
pub use hecs;
use hecs::Entity;
pub use log;
pub use rand;
pub use rapier3d;
pub use thunderdome;
pub use winit;
pub use yakui;

#[derive(Clone, Debug, Copy)]
pub struct GeometryOffsets {
    pub index_offset: u32,
    pub index_count: u32,
    pub vertex_offset: u32,
    pub vertex_count: u32,
}

impl GeometryOffsets {
    pub fn new(
        index_offset: usize,
        index_count: usize,
        vertex_offset: usize,
        vertex_count: usize,
    ) -> Self {
        Self {
            index_offset: index_offset as _,
            index_count: index_count as _,
            vertex_offset: vertex_offset as _,
            vertex_count: vertex_count as _,
        }
    }
}

#[derive(Clone, Default, Debug, Copy)]
pub struct Camera {
    pub position: glam::Vec3,
    pub pitch: f32,
    pub yaw: f32,
    pub distance: f32,
    pub focus_point: glam::Vec3,
    pub target: glam::Vec3,
    pub desired_distance: f32,
    pub start_distance: f32,
    pub projection: glam::Mat4,
    pub screen_size: glam::Vec2,
}

impl Camera {
    pub fn matrix(&self) -> glam::Affine3A {
        self.transform().inverse()
    }

    pub fn transform(&self) -> glam::Affine3A {
        let rotation = glam::Quat::from_euler(glam::EulerRot::YXZ, self.yaw, self.pitch, 0.);
        glam::Affine3A::from_rotation_translation(rotation, self.position)
    }

    pub fn resized(&mut self, window_size: winit::dpi::PhysicalSize<u32>) {
        let aspect_ratio = window_size.width as f32 / window_size.height as f32;
        let mut perspective =
            glam::Mat4::perspective_rh(60_f32.to_radians(), aspect_ratio, 0.01, 1000.);
        perspective.y_axis[1] *= -1.;
        self.projection = perspective;
        self.screen_size = [window_size.width as f32, window_size.height as f32].into();
    }

    pub fn create_ray(&self, click_in_screen: glam::Vec2) -> rapier3d::geometry::Ray {
        // Normalize the click position to NDC
        let ndc_x = (click_in_screen.x / self.screen_size.x - 0.5) * 2.0;
        let ndc_y = (click_in_screen.y / self.screen_size.y - 0.5) * 2.0;
        let click_in_clip = glam::Vec4::new(ndc_x, ndc_y, -1., 1.0);

        // Unproject the clip space coordinates to NDC space
        let view_from_clip = self.projection.inverse();
        let view_in_ndc = view_from_clip * click_in_clip;

        // Normalize the view space coordinates
        let direction_in_view = glam::Vec3::new(
            view_in_ndc.x / view_in_ndc.w,
            view_in_ndc.y / view_in_ndc.w,
            view_in_ndc.z / view_in_ndc.w,
        );

        // Transform the view space direction to world space
        let ray_in_world = self
            .transform()
            .transform_vector3(direction_in_view)
            .normalize();
        // Create the ray
        rapier3d::geometry::Ray::new(
            self.position.to_array().into(),
            ray_in_world.to_array().into(),
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct GUIState {
    pub game_over: bool,
    pub paperclips: usize,
    pub idle_workers: usize,
    pub selected_item: Option<(Entity, SelectedItemInfo)>,
    pub command_queue: VecDeque<GUICommand>,
    pub bars: BarState,
    pub clock: String,
    pub clock_description: String,
    pub total_deaths: usize,
}

#[derive(Debug, Clone, Default)]
pub struct BarState {
    pub health_percentage: f32,
    pub energy_percentage: f32,
}

#[derive(Debug, Clone)]
pub enum SelectedItemInfo {
    Viking(VikingInfo),
    PlaceOfWork(PlaceOfWorkInfo),
    Storage(StorageInfo),
}

#[derive(Debug, Clone, Default)]
pub struct VikingInfo {
    pub name: String,
    pub state: String,
    pub inventory: String,
    pub place_of_work: String,
    pub intelligence: usize,
    pub strength: usize,
    pub stamina: usize,
    pub needs: String,
    pub rest_state: String,
}

#[derive(Debug, Clone, Default)]
pub struct PlaceOfWorkInfo {
    pub name: String,
    pub task: String,
    pub workers: usize,
    pub max_workers: usize,
    pub stock: String,
}

#[derive(Debug, Clone, Default)]
pub struct StorageInfo {
    pub stock: String,
}

pub trait Renderer {
    fn init(window: winit::window::Window) -> Self;
    fn unload_assets(&mut self);
    fn update_assets(&mut self, world: &mut hecs::World);
    fn render(
        &mut self,
        world: &hecs::World,
        lines: &[Line],
        camera: Camera,
        yak: &mut yakui::Yakui,
        time_of_day: f32,
    );
    fn resized(&mut self, size: winit::dpi::PhysicalSize<u32>);
    fn cleanup(&mut self);
    fn window(&'_ self) -> &'_ winit::window::Window;
}

pub struct Line {
    pub start: glam::Vec3,
    pub end: glam::Vec3,
    pub colour: glam::Vec3,
}

impl Line {
    pub fn new(start: glam::Vec3, end: glam::Vec3, colour: glam::Vec3) -> Self {
        Self { start, end, colour }
    }
}

#[derive(Debug, Clone)]
pub enum GUICommand {
    SetWorkerCount(Entity, usize),
    Liquify(Entity),
    Restart,
    ConstructBuilding(&'static str), // this is awful
}

pub const BUILDING_TYPE_MINE: &str = "mine";
pub const BUILDING_TYPE_FORGE: &str = "forge";
pub const BUILDING_TYPE_FACTORY: &str = "factory";
pub const BUILDING_TYPE_HOUSE: &str = "house";
