use std::time::Instant;

const UPDATE_RATE: f32 = 1.0 / 60.0;
const MAX_ACCUMULATOR_MS: f32 = 50.0;

/// A timestep implementation that's actually good.
///
/// Stolen with love from @lpghatguy
#[derive(Debug, Clone, Copy)]
pub struct Time {
    start_of_game: Instant,
    start_of_frame: Instant,
    delta: f32,
    accumulated: f32,
}

impl Time {
    pub fn new() -> Self {
        Self {
            start_of_game: Instant::now(),
            start_of_frame: Instant::now(),
            delta: UPDATE_RATE,
            accumulated: 0.0,
        }
    }

    /// Tells how much time has passed since we last simulated the game.
    pub fn delta(&self) -> f32 {
        self.delta
    }

    /// Tells how long the game has been running in seconds.
    pub fn total_simulated(&self) -> f32 {
        (self.start_of_frame - self.start_of_game).as_secs_f32()
    }

    /// Start a new frame, accumulating time. Within a frame, there can be zero
    /// or more updates.
    pub fn start_frame(&mut self) {
        let now = Instant::now();
        let actual_delta = (now - self.start_of_frame).as_secs_f32();

        self.accumulated = (self.accumulated + actual_delta).min(MAX_ACCUMULATOR_MS / 1000.0);
        self.start_of_frame = now;
    }

    /// Consume accumulated time and tells whether we need to run a step of the
    /// game simulation.
    pub fn start_update(&mut self) -> bool {
        if self.accumulated < UPDATE_RATE {
            return false;
        }

        self.accumulated -= UPDATE_RATE;
        true
    }
}

impl Default for Time {
    fn default() -> Self {
        Self::new()
    }
}
