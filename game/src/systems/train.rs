use crate::Game;
use common::hecs::Entity;
use components::Transform;

pub struct TrackSegment {
    pub a: Option<Entity>,
    pub b: Option<Entity>,
}

pub struct Train {
    pub current_segment: Entity,
}

const TRAIN_SPEED: f32 = 1.0;

pub fn train_system(game: &mut Game) {
    let world = &game.world;

    let mut train = world.get::<&mut Train>(game.train).unwrap();
    let mut train_transform = world.get::<&mut Transform>(game.train).unwrap();

    let mut current_segment_transform = *world
        .get::<&Transform>(train.current_segment)
        .unwrap()
        .clone();
    // We only care about the xz plane
    current_segment_transform.position.y = train_transform.position.y;

    // Are we close to the segment?
    if train_transform
        .position
        .distance(current_segment_transform.position)
        .abs()
        < 0.1
    {
        // If yes, find next segment
        let Some(next_segment) = world.get::<&TrackSegment>(train.current_segment).unwrap().b else { return };
        train.current_segment = next_segment;
        return;
    }

    // If no, towards segment
    let train_to_segment = current_segment_transform.position - train_transform.position;
    train_transform.position += train_to_segment.normalize() * TRAIN_SPEED * game.time.delta();
}
