use common::{
    enum_iterator,
    glam::Vec3,
    hecs,
    rand::{self, rngs::ThreadRng, Rng},
};
use components::{Business, Contract, GLTFAsset, MaterialOverrides, Quota, Resource, Transform};

use crate::MAP_SIZE;

fn hex_to_rgb(hex: &str) -> Vec3 {
    let hex = hex.trim_start_matches("#");

    let r = u8::from_str_radix(&hex[0..2], 16).unwrap();
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap();
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap();

    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0].into()
}

pub fn get_resource_colour(r: Resource) -> Vec3 {
    match r {
        Resource::Wood => hex_to_rgb("#795548"),
        Resource::Coal => hex_to_rgb("#000000"),
        Resource::Uranium => hex_to_rgb("#1EFC0A"),
        Resource::Boots => hex_to_rgb("#424242"),
        Resource::Fish => hex_to_rgb("#2196F3"),
        Resource::Bread => hex_to_rgb("#FFF176"),
        Resource::HorseMeat => hex_to_rgb("#B71C1C"),
        Resource::Crabs => hex_to_rgb("#FF5722"),
        Resource::Amethyst => hex_to_rgb("#9C27B0"),
        Resource::GolfBalls => hex_to_rgb("#FFFFFF"),
    }
}

const MAX_RESOURCE_COUNT: usize = 5;
const _MAX_BUSINESSES_PER_RESOURCE: usize = 5;
const MIN_DISTANCE_TO_RESOURCE: f32 = 50.;
const _MAX_DISTANCE_TO_RESOURCE: f32 = 200.;
const MINIMUM_QUOTA_AMOUNT: usize = 10;
const MAXIMUM_QUOTA_AMOUNT: usize = 50;
const MAX_CLUTTER: usize = 50;

pub fn generate_map(world: &mut hecs::World) {
    let mut rng = rand::thread_rng();
    let extent = MAP_SIZE / 2.;
    // Some basic rules.
    // 1. We have 10 resources that need to be on the map
    for resource in enum_iterator::all::<Resource>() {
        for _ in 0..rng.gen_range(0..MAX_RESOURCE_COUNT) {
            let x = rng.gen_range(-extent..extent);
            let z = rng.gen_range(-extent..extent);
            let resource_position = [x, 0., z].into();

            world.spawn((
                Transform {
                    position: resource_position,
                    scale: Vec3::splat(2.),
                    ..Default::default()
                },
                GLTFAsset::new("cube.glb"),
                resource,
                MaterialOverrides {
                    base_colour_factor: get_resource_colour(resource).extend(1.0),
                },
            ));

            // First, spawn a business that's *close* to this resource:
            spawn_business(
                world,
                resource,
                resource_position,
                MIN_DISTANCE_TO_RESOURCE,
                &mut rng,
            );

            // Now spawn some businesses a little further away
            // for _ in 0..rng.gen_range(0..MAX_BUSINESSES_PER_RESOURCE) {
            //     spawn_business(
            //         world,
            //         resource,
            //         resource_position,
            //         MAX_DISTANCE_TO_RESOURCE,
            //         &mut rng,
            //     );
            // }
        }
    }

    for _ in 0..rng.gen_range(5..MAX_CLUTTER) {
        let x = rng.gen_range(-extent..extent);
        let z = rng.gen_range(-extent..extent);

        for _ in 0..rng.gen_range(5..MAX_CLUTTER) {
            let x_offset = rng.gen_range(-10.0..10.0);
            let z_offset = rng.gen_range(-10.0..10.0);
            let clutter_position = [x + x_offset, 0., z + z_offset].into();
            world.spawn((
                Transform {
                    position: clutter_position,
                    scale: Vec3::splat(1.),
                    ..Default::default()
                },
                GLTFAsset::new("tree.glb"),
            ));
        }
    }
}

fn spawn_business(
    world: &mut hecs::World,
    near_resource: Resource,
    resource_position: Vec3,
    max_distance: f32,
    rng: &mut ThreadRng,
) {
    let distance: f32 = rng.gen_range(max_distance - 10.0..max_distance);
    let angle: f32 = rng.gen_range(0.0..360.0);

    let business_x = resource_position.x + distance * angle.to_radians().cos();
    let business_z = resource_position.z + distance * angle.to_radians().sin();
    let quota_amount = rng.gen_range(MINIMUM_QUOTA_AMOUNT..MAXIMUM_QUOTA_AMOUNT);

    world.spawn((
        Transform {
            position: [business_x, 0., business_z].into(),
            scale: Vec3::splat(3.),
            ..Default::default()
        },
        GLTFAsset::new("building.glb"),
        Business {
            name: "A Business".into(),
            contract: Contract {
                quotas: [Quota {
                    resource: near_resource,
                    amount_per_day: quota_amount,
                }]
                .into(),
            },
        },
    ));
}
