use std::time::Instant;

use common::hecs;

#[derive(Debug, Clone)]
pub struct CombatState {
    pub target: hecs::Entity,
    pub last_attack_time: Instant,
}

impl CombatState {
    pub fn new(target: hecs::Entity) -> Self {
        Self {
            target,
            last_attack_time: Instant::now(),
        }
    }

    pub fn time_since_last_attack(&self) -> f32 {
        self.last_attack_time.elapsed().as_secs_f32()
    }
}
