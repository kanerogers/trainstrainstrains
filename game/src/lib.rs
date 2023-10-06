mod input;
mod map_generation;
mod systems;
pub mod time;
use common::{
    bitflags::bitflags,
    glam::{Quat, Vec2, Vec3},
    hecs::{self, RefMut},
    rapier3d::prelude::Ray,
    winit::{self},
    Camera, GUIState, Line,
};
use components::{GLTFAsset, Transform, Velocity};
use map_generation::generate_map;
use std::time::Instant;
use systems::{
    from_na,
    train::{train_system, TrackSegment, Train},
    transform_hierarchy::transform_hierarchy_system,
    update_position::update_position_system,
    PhysicsContext,
};
use time::Time;

pub const PLAYER_SPEED: f32 = 7.;
pub const CAMERA_ZOOM_SPEED: f32 = 100.;
pub const CAMERA_ROTATE_SPEED: f32 = 3.;
pub const MAX_CAMERA_ZOOM: f32 = 400.;
pub const MAP_SIZE: f32 = 1000.0; // 1km squared
const RENDER_DEBUG_LINES: bool = false;

// required due to reasons
#[no_mangle]
pub fn init() -> Game {
    Game::new()
}

#[no_mangle]
pub fn tick(game: &mut Game, _gui_state: &mut GUIState) -> bool {
    while game.time.start_update() {
        game.debug_lines.clear();
        camera_target_controller(game);
        update_camera(game);

        if !game.game_over {
            train_system(game);
        }

        update_position_system(game);
        transform_hierarchy_system(game);
        reset_mouse_clicks(&mut game.input.mouse_state);
    }

    if let Some(last_ray) = game.last_ray {
        let origin = from_na(last_ray.origin);
        let direction: Vec3 = from_na(last_ray.dir);
        let end = origin + direction * 100.;

        game.debug_lines.push(Line {
            start: origin,
            end,
            colour: [1., 0., 1.].into(),
        });
    }

    if !RENDER_DEBUG_LINES {
        game.debug_lines.clear();
    }

    false
}

#[no_mangle]
pub fn handle_winit_event(game: &mut Game, event: winit::event::WindowEvent) {
    input::handle_winit_event(game, event);
}

pub struct Game {
    pub world: hecs::World,
    pub time: Time,
    pub train: hecs::Entity,
    pub input: Input,
    pub camera: Camera,
    pub physics_context: PhysicsContext,
    pub window_size: winit::dpi::PhysicalSize<u32>,
    pub debug_lines: Vec<Line>,
    pub last_ray: Option<Ray>,
    pub game_over: bool,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            world: Default::default(),
            time: Default::default(),
            train: hecs::Entity::DANGLING,
            input: Default::default(),
            camera: Default::default(),
            physics_context: Default::default(),
            window_size: Default::default(),
            debug_lines: Default::default(),
            last_ray: None,
            game_over: false,
        }
    }
}

impl Game {
    pub fn new() -> Self {
        let mut world = hecs::World::default();
        world.spawn((
            GLTFAsset::new("map.glb"),
            Transform {
                scale: Vec3::splat(MAP_SIZE / 2.0),
                ..Default::default()
            },
        ));
        world.spawn((CameraTarget, Transform::default(), Velocity::default()));
        let a = world.spawn((
            GLTFAsset::new("tracks.glb"),
            Transform::from_position([0., 0.1, 0.]),
            TrackSegment { a: None, b: None },
        ));
        create_track_segments(&mut world, a, 10);
        generate_map(&mut world);

        let train = world.spawn((
            Train { current_segment: a },
            Transform::from_position([0., 0.4, 0.]),
            GLTFAsset::new("train.glb"),
            Velocity::default(),
        ));

        let camera = Camera {
            desired_distance: MAX_CAMERA_ZOOM,
            ..Default::default()
        };

        Game {
            camera,
            world,
            train,
            ..Default::default()
        }
    }

    pub fn resized(&mut self, window_size: winit::dpi::PhysicalSize<u32>) {
        self.window_size = window_size;
        self.camera.resized(window_size);
    }

    /// **panics**
    ///
    /// This method will panic if the entity does not exist.
    pub fn position_of(&self, entity: hecs::Entity) -> Vec3 {
        let world = &self.world;
        world.get::<&Transform>(entity).unwrap().position
    }

    pub fn command_buffer(&self) -> hecs::CommandBuffer {
        hecs::CommandBuffer::new()
    }

    pub fn run_command_buffer(&mut self, mut command_buffer: hecs::CommandBuffer) {
        command_buffer.run_on(&mut self.world);
    }

    pub fn get_first_with_tag<C: hecs::Component>(&self) -> hecs::Entity {
        self.world
            .query::<hecs::With<(), &C>>()
            .iter()
            .next()
            .unwrap()
            .0
    }

    pub fn get<'a, C: hecs::Component>(&'a self, entity: hecs::Entity) -> RefMut<'_, C> {
        self.world.get::<&'a mut C>(entity).unwrap()
    }
}

fn create_track_segments(world: &mut hecs::World, start: hecs::Entity, segments_remaining: usize) {
    if segments_remaining == 0 {
        return;
    }

    let x = world.get::<&mut Transform>(start).unwrap().position.x + 2.;
    let y = world.get::<&mut Transform>(start).unwrap().position.y;
    let a = world.spawn((
        GLTFAsset::new("tracks.glb"),
        Transform::from_position([x, y, 0.]),
        TrackSegment {
            a: Some(start),
            b: None,
        },
    ));
    world.get::<&mut TrackSegment>(start).unwrap().b = Some(a);

    create_track_segments(world, a, segments_remaining - 1);
}

pub struct ECS<'a> {
    pub world: &'a hecs::World,
}

impl ECS<'_> {
    pub fn position_of(&self, entity: hecs::Entity) -> Vec3 {
        let world = &self.world;
        world.get::<&Transform>(entity).unwrap().position
    }
}

bitflags! {
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Keys: u8 {
        const W = 0b00000001;
        const A = 0b00000010;
        const S = 0b00000100;
        const D = 0b00001000;
        const Q = 0b00010000;
        const E = 0b00100000;
        const C = 0b01000000;
        const Space = 0b10000000;
    }
}

impl Keys {
    pub fn as_axis(&self, negative: Keys, positive: Keys) -> f32 {
        let negative = self.contains(negative) as i8 as f32;
        let positive = self.contains(positive) as i8 as f32;
        positive - negative
    }
}

#[derive(Clone, Debug, Default)]
pub struct MouseState {
    pub position: Option<Vec2>,
    pub left_click_state: ClickState,
    pub right_click_state: ClickState,
    pub middle_click_state: ClickState,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ClickState {
    #[default]
    Released,
    Down,
    JustReleased,
}

#[derive(Clone, Debug)]
pub struct Input {
    pub keyboard_state: Keys,
    pub mouse_state: MouseState,
    pub camera_zoom: f32,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            mouse_state: Default::default(),
            keyboard_state: Default::default(),
            camera_zoom: 0.,
        }
    }
}

impl Input {
    pub fn reset(&mut self) {
        *self = Default::default();
    }

    pub fn is_pressed(&self, key: Keys) -> bool {
        self.keyboard_state.contains(key)
    }
}

pub struct CameraTarget;

pub fn update_camera(game: &mut Game) {
    let camera_target = game
        .position_of(game.get_first_with_tag::<CameraTarget>())
        .clone();
    let camera = &mut game.camera;
    camera.target = camera_target;
    let input = &game.input;
    let dt = game.time.delta();

    let focus_radius = 1.0;
    let focus_centering = 0.5;
    let distance_to_target = camera.target.distance(camera.focus_point);

    let mut t = 1.0;
    if distance_to_target > 0.01 {
        t = ((1. - focus_centering) as f32).powf(dt);
    }
    if distance_to_target > focus_radius {
        t = t.min(focus_radius / distance_to_target);
    }
    camera.focus_point = camera.target.lerp(camera.focus_point, t);

    let camera_rotate = input.keyboard_state.as_axis(Keys::E, Keys::Q);
    camera.yaw += camera_rotate * CAMERA_ROTATE_SPEED * dt;

    set_camera_distance(input, camera, dt);

    camera.pitch = -45_f32.to_radians();
    let look_rotation = Quat::from_euler(common::glam::EulerRot::YXZ, camera.yaw, camera.pitch, 0.);
    let look_direction = look_rotation * Vec3::NEG_Z;
    let look_position = camera.focus_point - look_direction * camera.distance;

    camera.position = look_position;
}

pub fn camera_target_controller(game: &mut Game) {
    let dt = game.time.delta();
    let camera_transform = game.camera.transform();
    let input = &game.input;
    let camera_target = game.get_first_with_tag::<CameraTarget>();
    let (transform, velocity) = game
        .world
        .query_one_mut::<(&mut Transform, &mut Velocity)>(camera_target)
        .unwrap();

    let input_movement = Vec3::new(
        input.keyboard_state.as_axis(Keys::A, Keys::D),
        0.,
        input.keyboard_state.as_axis(Keys::W, Keys::S),
    )
    .normalize();

    // Camera relative controls
    let mut forward = camera_transform.transform_vector3(Vec3::Z);
    forward.y = 0.;
    forward = forward.normalize();

    let mut right = camera_transform.transform_vector3(Vec3::X);
    right.y = 0.;
    right = right.normalize();

    let mut movement = forward * input_movement.z + right * input_movement.x;
    movement = movement.normalize_or_zero();
    movement.y = input_movement.y;
    movement = movement.normalize_or_zero();

    velocity.linear = velocity.linear.lerp(movement, 0.1);

    // Velocity, baby!
    let displacement = velocity.linear * PLAYER_SPEED * (game.camera.desired_distance / 2.) * dt;
    transform.position += displacement;
    transform.position.y = transform.position.y.min(5.).max(1.);
}

fn set_camera_distance(input: &Input, camera: &mut Camera, dt: f32) {
    if input.camera_zoom.abs() > 0. {
        camera.start_distance = camera.distance;
        camera.desired_distance += input.camera_zoom;
        camera.desired_distance = camera.desired_distance.clamp(5., MAX_CAMERA_ZOOM);
    }

    let current_delta = camera.desired_distance - camera.distance;

    let epsilon = 0.01;
    if current_delta.abs() > epsilon {
        camera.distance += current_delta * CAMERA_ZOOM_SPEED * dt;
    } else {
        camera.distance = camera.desired_distance;
    }
}

fn reset_mouse_clicks(mouse_state: &mut crate::MouseState) {
    match mouse_state.left_click_state {
        ClickState::JustReleased => mouse_state.left_click_state = ClickState::Released,
        _ => {}
    };
    match mouse_state.right_click_state {
        ClickState::JustReleased => mouse_state.right_click_state = ClickState::Released,
        _ => {}
    };
    match mouse_state.middle_click_state {
        ClickState::JustReleased => mouse_state.middle_click_state = ClickState::Released,
        _ => {}
    };
}

#[derive(Debug, Clone)]
pub struct HumanNeedsState {
    pub last_updated_at: Instant,
}

impl Default for HumanNeedsState {
    fn default() -> Self {
        Self {
            last_updated_at: Instant::now(),
        }
    }
}
