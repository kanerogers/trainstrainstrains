use components::{Transform, Velocity};

use crate::Game;

pub fn update_position_system(game: &mut Game) {
    let dt = game.time.delta();
    for (_, (transform, velocity)) in game.world.query::<(&mut Transform, &Velocity)>().iter() {
        let displacement = velocity.linear * dt;
        transform.position += displacement;
    }
}
