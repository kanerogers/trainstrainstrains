use common::rapier3d::{na, prelude::*};
use common::{glam, hecs, log, Line};

use crate::Game;
use components::{Collider, GLTFModel, Info, Transform};

pub struct PhysicsContext {
    rigid_body_set: RigidBodySet,
    collider_set: ColliderSet,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: BroadPhase,
    narrow_phase: NarrowPhase,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    ccd_solver: CCDSolver,
    query_pipeline: QueryPipeline,
    debug: DebugRenderPipeline,
}

impl Default for PhysicsContext {
    fn default() -> Self {
        Self {
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: BroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
            debug: DebugRenderPipeline::new(
                Default::default(),
                DebugRenderMode::all() & !DebugRenderMode::COLLIDER_AABBS,
            ),
        }
    }
}

impl PhysicsContext {
    pub fn step(&mut self, dt: f32) {
        let mut integration_parameters = IntegrationParameters::default();
        integration_parameters.dt = dt;
        self.physics_pipeline.step(
            &[0., -9.81, 0.].into(),
            &integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            Some(&mut self.query_pipeline),
            &(),
            &(),
        );
    }

    pub fn cast_ray(&self, ray: &Ray) -> Option<hecs::Entity> {
        let Some((handle, toi)) = self.query_pipeline.cast_ray(
            &self.rigid_body_set,
            &self.collider_set,
            ray,
            100.,
            true,
            Default::default(),
        ) else { return None };

        println!("Ray hit at {:?}", ray.point_at(toi));

        hecs::Entity::from_bits(self.collider_set.get(handle).unwrap().user_data as _)
    }

    fn render_debug(&mut self, backend: &mut PhysicsRenderer) {
        self.debug.render(
            backend,
            &self.rigid_body_set,
            &self.collider_set,
            &self.impulse_joint_set,
            &self.multibody_joint_set,
            &self.narrow_phase,
        );
    }

    /// Quick and dirty. Is this entity intersecting with any others?
    pub(crate) fn check_for_intersections(
        &self,
        entity: hecs::Entity,
        world: &hecs::World,
    ) -> bool {
        let Ok(handle) = world.get::<&ColliderHandle>(entity) else { 
            log::warn!("Attempted to check for intersections for entity that has no collider: {entity:?}");
            return false 
        };
        for (_, _, intersecting) in self.narrow_phase.intersections_with(*handle) {
            if intersecting {
                return true;
            }
        }

        false
    }
}

pub fn physics(game: &mut Game) {
    // create colliders if they're missing
    create_missing_collider_handles(game);

    // udpate rapier colliders; world is authoritative
    update_colliders(game);

    // step
    game.physics_context.step(game.time.delta());

    debug_lines(game);
}

fn debug_lines(game: &mut Game) {
    let mut backend = PhysicsRenderer {
        lines: &mut game.debug_lines,
    };

    game.physics_context.render_debug(&mut backend);
}

struct PhysicsRenderer<'a> {
    lines: &'a mut Vec<Line>,
}

impl<'a> DebugRenderBackend for PhysicsRenderer<'a> {
    fn draw_line(
        &mut self,
        _object: DebugRenderObject,
        a: Point<Real>,
        b: Point<Real>,
        color: [f32; 4],
    ) {
        self.lines.push(Line::new(
            from_na(a),
            from_na(b),
            glam::Vec3::new(color[0], color[1], color[2]),
        ));
    }
}

fn update_colliders(game: &mut Game) {
    for (_, (collider_info, handle, transform)) in game
        .world
        .query::<(&Collider, &ColliderHandle, &Transform)>()
        .iter()
    {
        let mut collider_transform = transform.clone();
        collider_transform.position.y += collider_info.y_offset;
        let collider = game.physics_context.collider_set.get_mut(*handle).unwrap();
        collider.set_position((&collider_transform).into());
    }
}

fn create_missing_collider_handles(game: &mut Game) {
    let mut command_buffer = hecs::CommandBuffer::new();

    for (entity, (collider_info, info, transform, model)) in game
        .world
        .query::<(&mut Collider, Option<&Info>, &Transform, &GLTFModel)>()
        .without::<&ColliderHandle>()
        .iter()
    {
        let (y_offset, shape) = get_shape_from_model(model);
        let mut collider_transform = transform.clone();
        collider_transform.position.y += y_offset;
        collider_info.y_offset = y_offset;

        let collider = ColliderBuilder::new(shape)
            .position((&collider_transform).into())
            .user_data(entity.to_bits().get() as _)
            .active_collision_types(ActiveCollisionTypes::all())
            .sensor(true);

        log::info!(
            "Created collider for {} - {:?}",
            info.as_ref().map(|i| &i.name).unwrap_or(&format!("{:?}", entity)),
            collider.position
        );

        let handle = game.physics_context.collider_set.insert(collider.build());

        command_buffer.insert_one(entity, handle);
    }

    command_buffer.run_on(&mut game.world);
}

pub fn from_na<T, U>(value: U) -> T
where
    T: FromNa<U>,
{
    T::from_na(value)
}

pub trait FromNa<U> {
    fn from_na(value: U) -> Self;
}

impl FromNa<na::Point3<f32>> for glam::Vec3 {
    fn from_na(value: na::Point3<f32>) -> Self {
        Self::new(value.x, value.y, value.z)
    }
}

impl FromNa<na::Vector3<f32>> for glam::Vec3 {
    fn from_na(value: na::Vector3<f32>) -> Self {
        Self::new(value.x, value.y, value.z)
    }
}

impl FromNa<na::Translation3<f32>> for glam::Vec3 {
    fn from_na(value: na::Translation3<f32>) -> Self {
        Self::new(value.x, value.y, value.z)
    }
}

impl FromNa<na::Quaternion<f32>> for glam::Quat {
    fn from_na(value: na::Quaternion<f32>) -> Self {
        Self::from_xyzw(value.i, value.j, value.k, value.w)
    }
}

impl<T, U> FromNa<na::Unit<T>> for U
where
    U: FromNa<T>,
{
    fn from_na(value: na::Unit<T>) -> Self {
        Self::from_na(value.into_inner())
    }
}

fn get_shape_from_model(model: &GLTFModel) -> (f32, SharedShape) {
    let mut max_x = f32::NEG_INFINITY;
    let mut min_x = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;

    for primitive in model.primitives.iter() {
        for v in &primitive.vertices {
            let pos = v.position;
            min_x = min_x.min(pos.x);
            max_x = max_x.max(pos.x);
            min_y = min_y.min(pos.y);
            max_y = max_y.max(pos.y);
            min_z = min_z.min(pos.z);
            max_z = max_z.max(pos.z);
        }
    }

    let half_x = (max_x - min_x) / 2.;
    let half_y = (max_y - min_y) / 2.;
    let half_z = (max_z - min_z) / 2.;

    (half_y, SharedShape::cuboid(half_x, half_y, half_z))
}
